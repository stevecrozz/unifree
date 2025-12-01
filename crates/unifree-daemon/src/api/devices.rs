use crate::{state::persist_snapshot, SharedState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;
use unifree_state::{DeviceState, FirmwareUpdateInfo};
use unifree_types::MacAddress;

#[derive(Deserialize)]
pub struct UpgradeRequest {
    pub url: Option<String>,
    pub version: Option<String>,
}

#[derive(Deserialize)]
pub struct ForgetParams {
    pub reset: Option<bool>,
}

#[derive(Serialize)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceState>,
}

pub async fn list_devices(State(shared): State<Arc<SharedState>>) -> Json<DeviceListResponse> {
    let state = shared.app_state.read().await;
    let devices = state.devices.values().cloned().collect();
    Json(DeviceListResponse { devices })
}

pub async fn adopt_device(
    State(shared): State<Arc<SharedState>>,
    Path(mac_str): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let mac: MacAddress = mac_str.parse().map_err(|_| StatusCode::BAD_REQUEST)?;

    let (ip, ssh_port) = {
        let state = shared.app_state.read().await;
        if let Some(device) = state.devices.get(&mac) {
            if let Some(ip) = device.last_ip {
                (ip, device.ssh_port.unwrap_or(22))
            } else {
                return Err(StatusCode::PRECONDITION_FAILED);
            }
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    };

    // Trigger adoption flow in background
    let shared_clone = shared.clone();
    tokio::spawn(async move {
        crate::adopt::perform_ssh_adoption_flow(shared_clone, mac, ip, ssh_port).await;
    });

    Ok(StatusCode::ACCEPTED)
}

pub async fn upgrade_device(
    State(shared): State<Arc<SharedState>>,
    Path(mac_str): Path<String>,
    Json(payload): Json<UpgradeRequest>,
) -> Result<StatusCode, StatusCode> {
    let mac: MacAddress = mac_str.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let manual_override = match (payload.url, payload.version) {
        (Some(url), Some(version)) => Some((url, version)),
        _ => None,
    };

    if let Some((url, version)) = manual_override {
        let snapshot = {
            let mut state = shared.app_state.write().await;
            if let Some(device) = state.devices.get_mut(&mac) {
                device.target_firmware = Some(FirmwareUpdateInfo {
                    version,
                    url,
                    md5: None,
                });
                Some(state.snapshot())
            } else {
                None
            }
        };

        match snapshot {
            Some(snapshot) => {
                if let Err(e) = persist_snapshot(snapshot).await {
                    error!("Failed to persist manual upgrade target: {}", e);
                }
                Ok(StatusCode::ACCEPTED)
            }
            None => Err(StatusCode::NOT_FOUND),
        }
    } else {
        let device_info = {
            let state = shared.app_state.read().await;
            match state.devices.get(&mac) {
                Some(device) => {
                    if let Some(model) = device.model.clone() {
                        let current = device
                            .version
                            .clone()
                            .unwrap_or_else(|| "0.0.0".to_string());
                        Some((model, current))
                    } else {
                        None
                    }
                }
                None => return Err(StatusCode::NOT_FOUND),
            }
        };

        let (model, current_version) = device_info.ok_or(StatusCode::PRECONDITION_FAILED)?;

        match shared
            .firmware_manager
            .get_update_for_device(&model, &current_version)
            .await
        {
            Some(update) => {
                let snapshot = {
                    let mut state = shared.app_state.write().await;
                    if let Some(device) = state.devices.get_mut(&mac) {
                        device.target_firmware = Some(FirmwareUpdateInfo {
                            version: update.version,
                            url: update.url,
                            md5: update.md5,
                        });
                        Some(state.snapshot())
                    } else {
                        None
                    }
                };

                match snapshot {
                    Some(snapshot) => {
                        if let Err(e) = persist_snapshot(snapshot).await {
                            error!("Failed to persist auto-upgrade target: {}", e);
                        }
                        Ok(StatusCode::ACCEPTED)
                    }
                    None => Err(StatusCode::NOT_FOUND),
                }
            }
            None => Err(StatusCode::NOT_FOUND),
        }
    }
}

pub async fn forget_device(
    State(shared): State<Arc<SharedState>>,
    Path(mac_str): Path<String>,
    Query(params): Query<ForgetParams>,
) -> Result<StatusCode, StatusCode> {
    let mac: MacAddress = mac_str.parse().map_err(|_| StatusCode::BAD_REQUEST)?;

    if params.reset == Some(true) {
        // Get IP and credentials
        let (ssh_user, ssh_pass) = {
            let config = shared.daemon_config.read().await;
            (config.ssh_user.clone(), config.ssh_pass.clone())
        };

        let (ip, ssh_port) = {
            let state = shared.app_state.read().await;
            if let Some(device) = state.devices.get(&mac) {
                if let Some(ip) = device.last_ip {
                    (ip, device.ssh_port.unwrap_or(22))
                } else {
                    return Err(StatusCode::PRECONDITION_FAILED); // No IP to reset
                }
            } else {
                return Err(StatusCode::NOT_FOUND);
            }
        };

        // Perform SSH reset
        if let Err(e) = crate::adopt::perform_ssh_reset(ip, ssh_port, &ssh_user, &ssh_pass).await {
            tracing::error!("Failed to factory reset {}: {}", mac, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    let mut state = shared.app_state.write().await;
    if state.devices.remove(&mac).is_some() {
        let snapshot = state.snapshot();
        drop(state);
        if let Err(e) = persist_snapshot(snapshot).await {
            error!("Failed to persist forget operation: {}", e);
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
