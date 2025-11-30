use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
};
use std::sync::Arc;
use tracing::{info, warn, error, debug};
use rand::Rng;
use chrono; // Added missing chrono import


use crate::{
    state::{self, DeviceStatus},
    SharedState,
};
use unifree_protocol::{
    crypto::AesKey,
    inform::{InformPacket, InformResponseBuilder},
    InformResponse,
};
use hex; // Import hex for the config_hash

/// Health check endpoint
pub async fn health_check() -> &'static str {
    "OK"
}

/// Handle incoming inform requests
pub async fn handle_inform(
    State(shared): State<Arc<SharedState>>,
    body: Bytes,
) -> Result<Bytes, StatusCode> {
    debug!("Received inform request ({} bytes)", body.len());

    // Decode the packet
    let packet = match InformPacket::decode(&body) {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to decode inform packet: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let mac = packet.mac;
    debug!("Inform from device: {} (flags: encrypted={}, gcm={}, compressed={})",
        mac, packet.flags.encrypted, packet.flags.aes_gcm, packet.flags.zlib_compressed);

    // Log raw packet body
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f").to_string();
    let mac_clean = mac.to_string().replace(":", "");
    let log_prefix = shared.log_dir.join(format!("{}_{}", timestamp, mac_clean));

    let log_prefix_clone = log_prefix.clone();
    let body_clone = body.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::fs::write(
            log_prefix_clone.with_extension("raw.bin"),
            &body_clone
        ).await {
            error!("Failed to write raw inform log: {}", e);
        }
    });

    // Get or create device state
    let mut state_guard = shared.app_state.write().await;
    let device = state_guard.get_or_create_device(mac);

    // Get config read lock
    let config_guard = shared.daemon_config.read().await;
    let current_config_hash = shared.config_hash.read().await.clone();

    // Try to decrypt with device key, fall back to default key
    let (key, key_source) = match device.auth_key.as_ref() {
        Some(k) => match AesKey::from_hex(k) {
            Ok(key) => (key, format!("device key {}", &k[..8])),
            Err(_) => (AesKey::default_key(), "default (hex parse failed)".to_string()),
        },
        None => (AesKey::default_key(), "default".to_string()),
    };
    debug!("Using {} for decryption", key_source);

    // Decrypt and parse the inform payload
    let request = match packet.decrypt_json(&key) {
        Ok(r) => {
            // Log decrypted JSON
            if let Ok(json) = serde_json::to_string_pretty(&r) {
                let log_prefix_clone = log_prefix.clone();
                tokio::spawn(async move {
                    if let Err(e) = tokio::fs::write(
                        log_prefix_clone.with_extension("decrypted.json"),
                        json
                    ).await {
                        error!("Failed to write decrypted inform log: {}", e);
                    }
                });
            }
            r
        },
        Err(e) => {
            warn!("Failed to decrypt inform from {}: {}", mac, e);
            // Try with default key if we used a custom key
            if device.auth_key.is_some() {
                match packet.decrypt_json(&AesKey::default_key()) {
                    Ok(r) => {
                        info!("Device {} appears to have reset, clearing auth key", mac);
                        device.auth_key = None;

                        // Log decrypted JSON (retry)
                        if let Ok(json) = serde_json::to_string_pretty(&r) {
                            let log_prefix_clone = log_prefix.clone();
                            tokio::spawn(async move {
                                let _ = tokio::fs::write(
                                    log_prefix_clone.with_extension("decrypted.json"),
                                    json
                                ).await;
                            });
                        }

                        r
                    }
                    Err(e2) => {
                        warn!("Also failed with default key: {}", e2);
                        return Err(StatusCode::BAD_REQUEST);
                    }
                }
            } else {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    };

    info!(
        "Inform from {} ({}): model={}, version={}, state={}, cfgversion={}",
        mac,
        request.hostname,
        request.model,
        request.version,
        request.state,
        request.cfgversion
    );

    // Update device info
    device.model = Some(request.model.clone());
    device.version = Some(request.version.clone());
    device.hostname = Some(request.hostname.clone());
    device.last_ip = Some(request.ip);
    device.last_seen = Some(chrono::Utc::now());
    device.current_cfgversion = Some(request.cfgversion.clone());
    device.is_default = request.default;

    // Parse radio table
    if !request.radio_table.is_empty() {
        let mut parsed_radios = Vec::new();
        for radio_value in &request.radio_table {
            match serde_json::from_value::<unifree_common::types::RadioTableEntry>(radio_value.clone()) {
                Ok(entry) => parsed_radios.push(entry),
                Err(e) => warn!("Failed to parse radio table entry: {}", e),
            }
        }
        device.radio_table = parsed_radios;
    }

    // Determine response based on device state
    // If device has our auth_key and is adopted, we should push config
    let has_our_key = device.auth_key.is_some();

    // Determine encryption/compression settings early for early returns
    let use_gcm = packet.flags.aes_gcm;
    // Official controller traces show responses are NOT compressed (IsZLIB N), even if request was.
    // Enforcing ZLIB caused mcad to fail JSON parsing (decoding 0x9c).
    let use_compression = false;

    // Use the global config hash as target
    device.target_cfgversion = Some(current_config_hash.clone());

    // Check if this is an event notification rather than a full inform
    if request.inform_as_notif {
        info!(
            "Received notification from {}: reason={:?} payload={:?}", 
            mac, 
            request.notif_reason,
            request.notif_payload
        );
        // Immediate response for notifications
        return Ok(Bytes::from(
            InformResponseBuilder::new(mac, key)
                .use_gcm(use_gcm)
                .use_compression(use_compression)
                .build(&InformResponse::noop(0))
                .unwrap()
        ));
    }

    let should_update = has_our_key && !request.default && should_push_config(device, &request.cfgversion);

    let response = if has_our_key && request.default {
        // Stage 1: Device is in default state but has our key (we just did SSH adoption)
        // We MUST send mgmt_cfg to persist the auth key and inform URL.
        info!("Device {} is default, pushing mgmt_cfg to finalize adoption", mac);
        device.status = DeviceStatus::Provisioning;

        let mac_str = mac.to_string();
        // Use the magic ADOPT_CFG_VERSION for the initial mgmt config to signal readiness for system_cfg
        let cfgversion = unifree_protocol::ADOPT_CFG_VERSION.to_string();

        // Include auth_key and inform_url in mgmt_cfg for adoption
        let auth_key = device.auth_key.as_deref();
        let inform_url = Some(config_guard.inform_url.as_str());
        let mgmt_cfg = config_guard.provision.generate_mgmt_cfg_with_auth(
            &mac_str,
            auth_key,
            inform_url,
            &cfgversion,
        );

        // Log mgmt.ini
        let mgmt_cfg_clone = mgmt_cfg.clone();
        let log_prefix_clone = log_prefix.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::write(log_prefix_clone.with_extension("mgmt.ini"), &mgmt_cfg_clone).await {
                error!("Failed to write mgmt.ini log: {}", e);
            }
        });
        debug!("mgmt_cfg:\n{}", mgmt_cfg);

        // Send ONLY mgmt_cfg, NO system_cfg, NO top-level cfgversion, NO interval
        InformResponse::set_config_with_version(
            None, // No top-level cfgversion for initial mgmt push
            None, // No system_cfg
            Some(mgmt_cfg),
            None
        )
    } else if should_update {
        // Stage 2: Provisioning / System Config
        
        if device.radio_table.is_empty() {
            info!("Device {} needs config update but has not reported radio table yet. Sending noop.", mac);
            InformResponse::noop(10)
        } else {
            // Device is not default, but config differs
            info!("Pushing system config update to device {}", mac);
            device.status = DeviceStatus::Provisioning;

            let mac_str = mac.to_string();
            let system_cfg = config_guard.provision.generate_system_ini(&mac_str, device);

            // Use the global config hash
            let cfgversion = current_config_hash;

            // Log system.ini
            let system_cfg_clone = system_cfg.clone();
            let log_prefix_clone = log_prefix.clone();
            tokio::spawn(async move {
                if let Err(e) = tokio::fs::write(log_prefix_clone.with_extension("system.ini"), &system_cfg_clone).await {
                    error!("Failed to write system.ini log: {}", e);
                }
            });
            debug!("system_cfg:\n{}", system_cfg);

            // Also generate mgmt_cfg with the new version to ensure device updates its state
            // The device needs cfgversion in mgmt_cfg to persist the new version
            let auth_key = device.auth_key.as_deref();
            let inform_url = Some(config_guard.inform_url.as_str());
            let mgmt_cfg = config_guard.provision.generate_mgmt_cfg_with_auth(
                &mac_str,
                auth_key,
                inform_url,
                &cfgversion,
            );

            // Log mgmt.ini (Stage 2)
            let mgmt_cfg_clone_2 = mgmt_cfg.clone();
            let log_prefix_clone_2 = log_prefix.clone();
            tokio::spawn(async move {
                if let Err(e) = tokio::fs::write(log_prefix_clone_2.with_extension("mgmt_stage2.ini"), &mgmt_cfg_clone_2).await {
                    error!("Failed to write mgmt_stage2.ini log: {}", e);
                }
            });

            // Send system_cfg AND mgmt_cfg
            InformResponse::set_config_with_version(
                Some(cfgversion),
                Some(system_cfg),
                Some(mgmt_cfg), 
                None
            )
        }
    } else if !has_our_key && request.default {
        // Device has no auth key and is in default state - truly awaiting adoption
        info!("Device {} is in default state, waiting for adoption (no auth key)", mac);
        InformResponse::noop(10)
    } else {
        // Just acknowledge the inform
        if device.status == DeviceStatus::Provisioning {
            // Config was pushed, mark as adopted
            device.status = DeviceStatus::Adopted;
            device.target_cfgversion = device.current_cfgversion.clone();
            info!("Device {} provisioning complete", mac);
        }
        InformResponse::noop(10)
    };

    // Save state
    if let Err(e) = state_guard.save() {
        error!("Failed to save state: {}", e);
    }

    // Log the response JSON before encryption
    let response_json = serde_json::to_string_pretty(&response).unwrap_or_default();
    debug!("Response JSON:\n{}", response_json);

    // Build encrypted response - match the device's encryption mode
    debug!("Responding with crypto: GCM={}, Compressed={}", use_gcm, use_compression);

    let builder = InformResponseBuilder::new(mac, key)
        .use_gcm(use_gcm)
        .use_compression(use_compression);

    let response_bytes = match builder.build(&response) {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to build response: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(Bytes::from(response_bytes))
}

/// Check if we should push config to this device
pub fn should_push_config(device: &state::DeviceState, device_reported_cfgversion: &str) -> bool {
    // If device reports ADOPT_CFG_VERSION, it means it's done with mgmt_cfg and ready for system_cfg
    if device_reported_cfgversion == unifree_protocol::ADOPT_CFG_VERSION {
        return true;
    }

    // Push if current config differs from target
    if let (Some(current), Some(target)) = (&device.current_cfgversion, &device.target_cfgversion) {
        if current != target {
            return true;
        }
    }

    // Also push if target is set but current is None (first connect)
    if device.current_cfgversion.is_none() && device.target_cfgversion.is_some() {
        return true;
    }

    false
}
