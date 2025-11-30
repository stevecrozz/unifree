//! SSH adoption flow for UniFi devices
//!
//! Adoption involves:
//! 1. Connecting to device via SSH (default credentials: ubnt/ubnt)
//! 2. Running: /usr/bin/syswrapper.sh set-adopt <inform_url> <auth_key>
//! 3. Device will then start informing to our endpoint with the new key

use std::net::IpAddr;
use std::sync::Arc;
use tracing::{info, warn, debug};

use crate::{SharedState, state::DeviceStatus};
use unifree_protocol::MacAddress;

// Re-export generate_auth_key from common
pub use unifree_common::ssh::generate_auth_key;

/// Simple SSH adoption - executes set-adopt with provided key
pub async fn perform_ssh_adoption(
    ip: IpAddr,
    port: u16,
    user: &str,
    pass: &str,
    inform_url: &str,
    auth_key: &str,
) -> anyhow::Result<()> {
    // Execute the set-adopt command
    let cmd = format!("/usr/bin/syswrapper.sh set-adopt {} {}", inform_url, auth_key);
    
    let (exit_code, output) = unifree_common::ssh::execute_ssh_command(
        ip,
        port,
        user,
        pass,
        &cmd
    ).await?;

    debug!("Command output: {}", output.trim());
    debug!("Exit code: {:?}", exit_code);

    match exit_code {
        Some(0) => Ok(()),
        Some(code) => anyhow::bail!("set-adopt exited with code {}: {}", code, output.trim()),
        None => {
            if output.to_lowercase().contains("error") {
                anyhow::bail!("set-adopt may have failed: {}", output.trim());
            }
            Ok(())
        }
    }
}

/// Perform the high-level SSH adoption flow, including state updates
pub async fn perform_ssh_adoption_flow(
    shared: Arc<SharedState>,
    mac: MacAddress,
    ip: std::net::IpAddr,
    ssh_port: u16,
) {
    debug!("perform_ssh_adoption_flow called for {} at {}", mac, ip);

    // Mark as adopting (with lock check)
    {
        let mut state_guard = shared.app_state.write().await;
        match state_guard.devices.get_mut(&mac) {
            Some(device) => {
                debug!("Device {} found with status {:?}", mac, device.status);
                if device.status == DeviceStatus::Adopting {
                    debug!("Device {} already being adopted, skipping", mac);
                    return;
                }
                if device.status == DeviceStatus::Adopted {
                    debug!("Device {} already adopted, skipping", mac);
                    return;
                }
                device.status = DeviceStatus::Adopting;
                let _ = state_guard.save();
            }
            None => {
                warn!("Device {} not found in state, cannot adopt", mac);
                return;
            }
        }
    }

    info!("Starting SSH adoption for {} at {}:{}", mac, ip, ssh_port);

    let (ssh_user, ssh_pass, inform_url) = {
        let config = shared.daemon_config.read().await;
        (config.ssh_user.clone(), config.ssh_pass.clone(), config.inform_url.clone())
    };

    // Generate key and update state BEFORE SSH to handle race condition
    // (Device informs immediately after set-adopt, potentially before SSH returns)
    let auth_key = generate_auth_key();
    {
        let mut state_guard = shared.app_state.write().await;
        if let Some(device) = state_guard.devices.get_mut(&mac) {
            device.status = DeviceStatus::Adopting;
            device.auth_key = Some(auth_key.clone());
            device.inform_url = Some(inform_url.clone());
            let _ = state_guard.save();
        }
    }

    // Perform SSH adoption directly with timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60), // Increased timeout for retry
        async {
            let res = perform_ssh_adoption(
                ip,
                ssh_port,
                &ssh_user,
                &ssh_pass,
                &inform_url,
                &auth_key,
            ).await;

            if res.is_err() && ssh_user != "ubnt" {
                warn!("Adoption with user '{}' failed, retrying with default 'ubnt' credentials...", ssh_user);
                perform_ssh_adoption(
                    ip,
                    ssh_port,
                    "ubnt",
                    "ubnt",
                    &inform_url,
                    &auth_key,
                ).await
            } else {
                res
            }
        }
    ).await;

    // Update state based on result
    let mut state_guard = shared.app_state.write().await;
    if let Some(device) = state_guard.devices.get_mut(&mac) {
        match result {
            Ok(Ok(())) => {
                device.status = DeviceStatus::Adopted;
                // auth_key is already set
                device.adopted_at = Some(chrono::Utc::now());
                info!("Auto-adoption of {} successful!", mac);
            }
            Ok(Err(e)) => {
                device.status = DeviceStatus::Discovered;
                device.auth_key = None; // Clear invalid key
                warn!("Auto-adoption of {} failed: {}", mac, e);
            }
            Err(_) => {
                device.status = DeviceStatus::Discovered;
                device.auth_key = None; // Clear invalid key
                warn!("Auto-adoption of {} timed out after 30s", mac);
            }
        }
        let _ = state_guard.save();
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
    }
}
