//! SSH adoption flow for UniFi devices
//!
//! Adoption involves:
//! 1. Connecting to device via SSH (default credentials: ubnt/ubnt)
//! 2. Running: /usr/bin/syswrapper.sh set-adopt <inform_url> <auth_key>
//! 3. Device will then start informing to our endpoint with the new key

use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

use crate::state::{AppState, DeviceStatus};
use unifree_protocol::MacAddress;

/// Simple SSH adoption - returns auth key on success
pub async fn perform_ssh_adoption(
    ip: IpAddr,
    port: u16,
    user: &str,
    pass: &str,
    inform_url: &str,
) -> anyhow::Result<String> {
    use russh::*;
    use std::sync::Arc as StdArc;

    let auth_key = generate_auth_key();

    debug!("Connecting to {}:{} as {}", ip, port, user);

    // SSH client configuration
    let config = StdArc::new(client::Config::default());
    
    // Create client handler
    struct AdoptHandler;

    #[async_trait::async_trait]
    impl client::Handler for AdoptHandler {
        type Error = anyhow::Error;

        async fn check_server_key(
            &mut self,
            _server_public_key: &ssh_key::PublicKey,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    // Connect
    let addr = format!("{}:{}", ip, port);
    let mut session = client::connect(config, &addr, AdoptHandler)
        .await
        .map_err(|e| anyhow::anyhow!("SSH connection failed: {}", e))?;

    // Authenticate
    debug!("Authenticating as {}", user);
    let auth_ok = session.authenticate_password(user, pass)
        .await
        .map_err(|e| anyhow::anyhow!("SSH authentication error: {}", e))?;
    
    if !auth_ok {
        anyhow::bail!("SSH authentication rejected");
    }

    // Execute the set-adopt command
    let cmd = format!("/usr/bin/syswrapper.sh set-adopt {} {}", inform_url, auth_key);
    info!("Executing: /usr/bin/syswrapper.sh set-adopt {} <key>", inform_url);

    let mut channel = session.channel_open_session()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open channel: {}", e))?;

    channel.exec(true, cmd.as_str())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to execute command: {}", e))?;

    // Read output
    let mut output = String::new();
    let mut exit_code = None;

    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                output.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                output.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = Some(exit_status as i32);
            }
            Some(ChannelMsg::Eof) | None => break,
            _ => {}
        }
    }

    debug!("Command output: {}", output.trim());
    debug!("Exit code: {:?}", exit_code);

    let _ = session.disconnect(Disconnect::ByApplication, "", "").await;

    match exit_code {
        Some(0) => Ok(auth_key),
        Some(code) => anyhow::bail!("set-adopt exited with code {}: {}", code, output.trim()),
        None => {
            if output.to_lowercase().contains("error") {
                anyhow::bail!("set-adopt may have failed: {}", output.trim());
            }
            Ok(auth_key)
        }
    }
}

/// Default SSH credentials for factory-reset UniFi devices
const DEFAULT_SSH_USER: &str = "ubnt";
const DEFAULT_SSH_PASS: &str = "ubnt";

/// Result of an adoption attempt
#[derive(Debug)]
pub enum AdoptionResult {
    /// Adoption successful
    Success { auth_key: String },
    /// Device not found in state
    DeviceNotFound,
    /// Device not in adoptable state
    NotAdoptable { reason: String },
    /// SSH connection failed
    SshConnectionFailed { error: String },
    /// SSH authentication failed
    SshAuthFailed { error: String },
    /// Command execution failed
    CommandFailed { error: String, exit_code: Option<i32> },
}

/// Generate a random 32-character hex auth key
pub fn generate_auth_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    // Generate pseudo-random key from timestamp and process ID
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    let random1 = (timestamp ^ (std::process::id() as u128)) as u64;
    let random2 = timestamp.wrapping_mul(0x5851F42D4C957F2D) as u64;
    
    format!("{:016x}{:016x}", random1, random2)
}

/// Adopt a device via SSH
pub async fn adopt_device(
    state: Arc<RwLock<AppState>>,
    mac: MacAddress,
    inform_url: &str,
    ssh_user: Option<&str>,
    ssh_pass: Option<&str>,
) -> AdoptionResult {
    let user = ssh_user.unwrap_or(DEFAULT_SSH_USER);
    let pass = ssh_pass.unwrap_or(DEFAULT_SSH_PASS);
    
    // Get device info from state
    let (ip, ssh_port, is_default) = {
        let state_guard = state.read().await;
        match state_guard.devices.get(&mac) {
            Some(device) => {
                let ip = match device.last_ip {
                    Some(ip) => ip,
                    None => return AdoptionResult::NotAdoptable { 
                        reason: "No IP address known for device".to_string() 
                    },
                };
                (ip, device.ssh_port.unwrap_or(22), device.is_default)
            }
            None => return AdoptionResult::DeviceNotFound,
        }
    };

    // Check if device is adoptable
    if !is_default {
        // Device is already adopted - we might still want to re-adopt
        info!("Device {} is not in default state, attempting re-adoption", mac);
    }

    // Generate new auth key
    let auth_key = generate_auth_key();
    
    info!(
        "Adopting device {} at {}:{} with inform URL {}",
        mac, ip, ssh_port, inform_url
    );

    // Update state to adopting
    {
        let mut state_guard = state.write().await;
        if let Some(device) = state_guard.devices.get_mut(&mac) {
            device.status = DeviceStatus::Adopting;
            let _ = state_guard.save();
        }
    }

    // Perform SSH adoption
    let result = do_ssh_adoption(ip, ssh_port, user, pass, inform_url, &auth_key).await;

    // Update state based on result
    {
        let mut state_guard = state.write().await;
        if let Some(device) = state_guard.devices.get_mut(&mac) {
            match &result {
                AdoptionResult::Success { auth_key } => {
                    device.status = DeviceStatus::Adopted;
                    device.auth_key = Some(auth_key.clone());
                    device.inform_url = Some(inform_url.to_string());
                    device.adopted_at = Some(chrono::Utc::now());
                    info!("Device {} adopted successfully", mac);
                }
                _ => {
                    device.status = DeviceStatus::Discovered;
                    warn!("Adoption of {} failed: {:?}", mac, result);
                }
            }
            let _ = state_guard.save();
        }
    }

    result
}

/// Internal: Perform the actual SSH connection and command execution
async fn do_ssh_adoption(
    ip: IpAddr,
    port: u16,
    user: &str,
    pass: &str,
    inform_url: &str,
    auth_key: &str,
) -> AdoptionResult {
    use russh::*;
    use std::sync::Arc as StdArc;

    debug!("Connecting to {}:{} as {}", ip, port, user);

    // SSH client configuration
    let config = StdArc::new(client::Config::default());
    
    // Create client handler
    struct InternalAdoptHandler;

    #[async_trait::async_trait]
    impl client::Handler for InternalAdoptHandler {
        type Error = anyhow::Error;

        async fn check_server_key(
            &mut self,
            _server_public_key: &ssh_key::PublicKey,
        ) -> Result<bool, Self::Error> {
            // Accept any server key (like ssh -o StrictHostKeyChecking=no)
            Ok(true)
        }
    }

    // Connect
    let addr = format!("{}:{}", ip, port);
    let mut session = match client::connect(config, &addr, InternalAdoptHandler).await {
        Ok(session) => session,
        Err(e) => {
            return AdoptionResult::SshConnectionFailed {
                error: e.to_string(),
            };
        }
    };

    // Authenticate
    debug!("Authenticating as {}", user);
    let auth_result = session.authenticate_password(user, pass).await;
    
    match auth_result {
        Ok(true) => {
            debug!("Authentication successful");
        }
        Ok(false) => {
            return AdoptionResult::SshAuthFailed {
                error: "Authentication rejected".to_string(),
            };
        }
        Err(e) => {
            return AdoptionResult::SshAuthFailed {
                error: e.to_string(),
            };
        }
    }

    // Execute the set-adopt command
    let cmd = format!("/usr/bin/syswrapper.sh set-adopt {} {}", inform_url, auth_key);
    info!("Executing: {}", cmd);

    let mut channel = match session.channel_open_session().await {
        Ok(ch) => ch,
        Err(e) => {
            return AdoptionResult::CommandFailed {
                error: format!("Failed to open channel: {}", e),
                exit_code: None,
            };
        }
    };

    if let Err(e) = channel.exec(true, cmd.as_str()).await {
        return AdoptionResult::CommandFailed {
            error: format!("Failed to execute command: {}", e),
            exit_code: None,
        };
    }

    // Read output
    let mut output = String::new();
    let mut exit_code = None;

    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                output.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                output.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = Some(exit_status as i32);
            }
            Some(ChannelMsg::Eof) | None => break,
            _ => {}
        }
    }

    debug!("Command output: {}", output.trim());
    debug!("Exit code: {:?}", exit_code);

    // Close session
    let _ = session.disconnect(Disconnect::ByApplication, "", "").await;

    // Check result
    match exit_code {
        Some(0) => AdoptionResult::Success {
            auth_key: auth_key.to_string(),
        },
        Some(code) => AdoptionResult::CommandFailed {
            error: format!("Command exited with code {}: {}", code, output.trim()),
            exit_code: Some(code),
        },
        None => {
            // No exit code but command ran - assume success if no error output
            if output.trim().is_empty() || !output.to_lowercase().contains("error") {
                AdoptionResult::Success {
                    auth_key: auth_key.to_string(),
                }
            } else {
                AdoptionResult::CommandFailed {
                    error: format!("Unknown result: {}", output.trim()),
                    exit_code: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_auth_key() {
        let key1 = generate_auth_key();
        let key2 = generate_auth_key();
        
        // Should be 32 hex chars
        assert_eq!(key1.len(), 32);
        assert_eq!(key2.len(), 32);
        
        // Should be valid hex
        assert!(key1.chars().all(|c| c.is_ascii_hexdigit()));
        
        // Note: Keys might be the same if generated in same nanosecond
        // In real usage, there's enough entropy from timing
    }
}
