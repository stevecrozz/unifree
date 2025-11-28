//! SSH adoption for UniFi devices

use std::net::IpAddr;
use tracing::{info, debug};

/// Generate a random 32-character hex auth key
pub fn generate_auth_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    let random1 = (timestamp ^ (std::process::id() as u128)) as u64;
    let random2 = timestamp.wrapping_mul(0x5851F42D4C957F2D) as u64;
    
    format!("{:016x}{:016x}", random1, random2)
}

/// Perform SSH adoption
pub async fn perform_adoption(
    ip: IpAddr,
    port: u16,
    user: &str,
    pass: &str,
    inform_url: &str,
) -> anyhow::Result<String> {
    use russh::*;
    use std::sync::Arc;

    let auth_key = generate_auth_key();

    debug!("Connecting to {}:{} as {}", ip, port, user);

    // SSH client configuration
    let config = Arc::new(client::Config::default());
    
    // Create client handler
    struct AdoptHandler;

    #[async_trait::async_trait]
    impl client::Handler for AdoptHandler {
        type Error = anyhow::Error;

        async fn check_server_key(
            &mut self,
            _server_public_key: &ssh_key::PublicKey,
        ) -> Result<bool, Self::Error> {
            // Accept any server key
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
        anyhow::bail!("SSH authentication rejected - wrong credentials?");
    }
    debug!("Authentication successful");

    // Execute the set-adopt command
    let cmd = format!("/usr/bin/syswrapper.sh set-adopt {} {}", inform_url, auth_key);
    info!("Executing: {}", cmd);

    let mut channel = session.channel_open_session()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open SSH channel: {}", e))?;

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

    // Close session
    let _ = session.disconnect(Disconnect::ByApplication, "", "").await;

    // Check result
    match exit_code {
        Some(0) => Ok(auth_key),
        Some(code) => anyhow::bail!("Command exited with code {}: {}", code, output.trim()),
        None => {
            // No exit code but command ran - check output for errors
            if output.to_lowercase().contains("error") {
                anyhow::bail!("Command may have failed: {}", output.trim());
            }
            Ok(auth_key)
        }
    }
}

/// Perform factory reset via SSH
pub async fn perform_reset(
    ip: IpAddr,
    port: u16,
    user: &str,
    pass: &str,
) -> anyhow::Result<()> {
    use russh::*;
    use std::sync::Arc;

    debug!("Connecting to {}:{} as {}", ip, port, user);

    // SSH client configuration
    let config = Arc::new(client::Config::default());
    
    // Create client handler
    struct ResetHandler;

    #[async_trait::async_trait]
    impl client::Handler for ResetHandler {
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
    let mut session = client::connect(config, &addr, ResetHandler)
        .await
        .map_err(|e| anyhow::anyhow!("SSH connection failed: {}", e))?;

    // Authenticate
    debug!("Authenticating as {}", user);
    let auth_ok = session.authenticate_password(user, pass)
        .await
        .map_err(|e| anyhow::anyhow!("SSH authentication error: {}", e))?;
    
    if !auth_ok {
        anyhow::bail!("SSH authentication rejected - wrong credentials?");
    }
    debug!("Authentication successful");

    // Execute the restore-default command
    let cmd = "/usr/bin/syswrapper.sh restore-default";
    info!("Executing: {}", cmd);

    let mut channel = session.channel_open_session()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open SSH channel: {}", e))?;

    channel.exec(true, cmd)
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

    // Close session
    let _ = session.disconnect(Disconnect::ByApplication, "", "").await;
    
    // If exit code is non-zero, it failed. 
    // If it's None (connection dropped), it likely succeeded (rebooted).
    if let Some(code) = exit_code {
        if code != 0 {
            anyhow::bail!("Command exited with code {}: {}", code, output.trim());
        }
    }

    Ok(())
}
