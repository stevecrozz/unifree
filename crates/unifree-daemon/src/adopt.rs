//! SSH adoption flow for UniFi devices
//!
//! Adoption involves:
//! 1. Connecting to device via SSH (default credentials: ubnt/ubnt)
//! 2. Running: /usr/bin/syswrapper.sh set-adopt <inform_url> <auth_key>
//! 3. Device will then start informing to our endpoint with the new key

use std::net::IpAddr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::{
    state::{persist_snapshot, DeviceStatus},
    SharedState,
};
use unifree_adopt::{execute_ssh_command, generate_auth_key};
use unifree_protocol::MacAddress;

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
    let cmd = format!(
        "/usr/bin/syswrapper.sh set-adopt {} {}",
        inform_url, auth_key
    );

    let (exit_code, output) = execute_ssh_command(ip, port, user, pass, &cmd).await?;

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

/// Perform factory reset via SSH
pub async fn perform_ssh_reset(
    ip: IpAddr,
    port: u16,
    user: &str,
    pass: &str,
) -> anyhow::Result<()> {
    // Execute the restore-default command
    let cmd = "/usr/bin/syswrapper.sh restore-default";
    info!("Executing reset on {}: {}", ip, cmd);

    let (exit_code, output) = execute_ssh_command(ip, port, user, pass, cmd).await?;

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

/// Perform the high-level SSH adoption flow, including state updates
pub async fn perform_ssh_adoption_flow(
    shared: Arc<SharedState>,
    mac: MacAddress,
    ip: std::net::IpAddr,
    ssh_port: u16,
) {
    debug!("perform_ssh_adoption_flow called for {} at {}", mac, ip);

    // Mark as adopting (with lock check)
    let persist_result = {
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
                Some(state_guard.snapshot())
            }
            None => {
                warn!("Device {} not found in state, cannot adopt", mac);
                None
            }
        }
    };
    if let Some(result) = persist_result {
        if let Err(e) = persist_snapshot(result).await {
            error!("Failed to persist adoption state: {}", e);
        }
    }

    info!("Starting SSH adoption for {} at {}:{}", mac, ip, ssh_port);

    let (ssh_user, ssh_pass, inform_url) = {
        let config = shared.daemon_config.read().await;
        (
            config.ssh_user.clone(),
            config.ssh_pass.clone(),
            config.inform_url.clone(),
        )
    };

    // Generate key and update state BEFORE SSH to handle race condition
    // (Device informs immediately after set-adopt, potentially before SSH returns)
    let auth_key = generate_auth_key();
    let persist_result = {
        let mut state_guard = shared.app_state.write().await;
        if let Some(device) = state_guard.devices.get_mut(&mac) {
            device.status = DeviceStatus::Adopting;
            device.auth_key = Some(auth_key.clone());
            device.inform_url = Some(inform_url.clone());
            Some(state_guard.snapshot())
        } else {
            None
        }
    };
    if let Some(result) = persist_result {
        if let Err(e) = persist_snapshot(result).await {
            error!("Failed to persist adoption key: {}", e);
        }
    }

    // Perform SSH adoption directly with timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60), // Increased timeout for retry
        async {
            let res =
                perform_ssh_adoption(ip, ssh_port, &ssh_user, &ssh_pass, &inform_url, &auth_key)
                    .await;

            if res.is_err() && ssh_user != "ubnt" {
                warn!(
                    "Adoption with user '{}' failed, retrying with default 'ubnt' credentials...",
                    ssh_user
                );
                perform_ssh_adoption(ip, ssh_port, "ubnt", "ubnt", &inform_url, &auth_key).await
            } else {
                res
            }
        },
    )
    .await;

    // Update state based on result
    let persist_result = {
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
                    warn!("Auto-adoption of {} timed out after 60s", mac);
                }
            }
            Some(state_guard.snapshot())
        } else {
            None
        }
    };
    if let Some(result) = persist_result {
        if let Err(e) = persist_snapshot(result).await {
            error!("Failed to persist adoption result: {}", e);
        }
    }
}
