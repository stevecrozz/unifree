//! SSH adoption for UniFi devices

use std::net::IpAddr;
use tracing::{debug, info};

pub use unifree_adopt::generate_auth_key;

/// Perform SSH adoption
pub async fn perform_adoption(
    ip: IpAddr,
    port: u16,
    user: &str,
    pass: &str,
    inform_url: &str,
) -> anyhow::Result<String> {
    let auth_key = generate_auth_key();

    // Execute the set-adopt command
    let cmd = format!(
        "/usr/bin/syswrapper.sh set-adopt {} {}",
        inform_url, auth_key
    );
    info!("Executing: {}", cmd);

    let (exit_code, output) =
        unifree_adopt::execute_ssh_command(ip, port, user, pass, &cmd).await?;

    debug!("Command output: {}", output.trim());
    debug!("Exit code: {:?}", exit_code);

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
pub async fn perform_reset(ip: IpAddr, port: u16, user: &str, pass: &str) -> anyhow::Result<()> {
    // Execute the restore-default command
    let cmd = "/usr/bin/syswrapper.sh restore-default";
    info!("Executing: {}", cmd);

    let (exit_code, output) = unifree_adopt::execute_ssh_command(ip, port, user, pass, cmd).await?;

    debug!("Command output: {}", output.trim());
    debug!("Exit code: {:?}", exit_code);

    // If exit code is non-zero, it failed.
    // If it's None (connection dropped), it likely succeeded (rebooted).
    if let Some(code) = exit_code {
        if code != 0 {
            anyhow::bail!("Command exited with code {}: {}", code, output.trim());
        }
    }

    Ok(())
}
