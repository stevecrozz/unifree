use std::sync::Arc;
use tracing::{info, error, warn};
use sha2::{Sha256, Digest};
use hex;

use crate::{SharedState};
use crate::config::ProvisionConfig;
use crate::state::DeviceStatus;

/// Run the config reloader
pub async fn run_config_reloader(path: String, shared: Arc<SharedState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        interval.tick().await;

        // Attempt to load and hash config
        match std::fs::read(&path) {
            Ok(config_bytes) => {
                // Calculate hash of raw bytes
                let mut hasher = Sha256::new();
                hasher.update(&config_bytes);
                let new_hash = hex::encode(&hasher.finalize()[0..8]);

                let current_hash = shared.config_hash.read().await.clone();

                if new_hash != current_hash {
                    // Try to parse the config to ensure it's valid before applying
                    match serde_json::from_slice::<ProvisionConfig>(&config_bytes) {
                        Ok(new_provision) => {
                            info!("Configuration changed (hash: {} -> {}), reloading...", current_hash, new_hash);

                            // Update state
                            {
                                let mut config_guard = shared.daemon_config.write().await;
                                let mut hash_guard = shared.config_hash.write().await;

                                // Preserve runtime options
                                config_guard.provision = new_provision;
                                *hash_guard = new_hash.clone();
                            }

                            info!("Configuration reloaded successfully");
                        },
                        Err(e) => {
                            error!("Detected config change but failed to parse {}: {}", path, e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to read config file {}: {}", path, e);
            }
        }
    }
}

/// Run stale adoption cleanup task
pub async fn run_stale_adoption_cleanup(shared: Arc<SharedState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        let mut devices_to_reset = Vec::new();
        let now = chrono::Utc::now();

        // Check for stale adoptions
        {
            let state_guard = shared.app_state.read().await;
            for (mac, device) in &state_guard.devices {
                if device.status == DeviceStatus::Adopting {
                    // If adopting for more than 5 minutes, reset
                    // We use last_seen as a proxy for "activity"
                    if let Some(last_seen) = device.last_seen {
                        if (now - last_seen).num_minutes() > 5 {
                             devices_to_reset.push(mac.clone());
                        }
                    } else {
                        // If never seen (unlikely if adopting, but strictly speaking)
                        devices_to_reset.push(mac.clone());
                    }
                }
            }
        }

        if !devices_to_reset.is_empty() {
             let mut state_guard = shared.app_state.write().await;
             for mac in devices_to_reset {
                 if let Some(device) = state_guard.devices.get_mut(&mac) {
                     // Double check status hasn't changed
                     if device.status == DeviceStatus::Adopting {
                         warn!("Resetting stale adoption state for {}", mac);
                         device.status = DeviceStatus::Discovered;
                         device.auth_key = None;
                     }
                 }
             }
             if let Err(e) = state_guard.save() {
                 error!("Failed to save state during stale cleanup: {}", e);
             }
        }
    }
}
