use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{info, debug, error};

use crate::{SharedState, state::DeviceStatus};
use unifree_protocol::discovery::DiscoveryPacket;

/// Run the discovery listener
pub async fn run_discovery_listener(
    port: u16,
    shared: Arc<SharedState>,
) -> anyhow::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    socket.set_broadcast(true)?;

    info!("Discovery listener started on port {}", port);

    let mut buf = [0u8; 2048];
    loop {
        let (len, src) = socket.recv_from(&mut buf).await?;

        // Skip small packets (probes)
        if len < 5 {
            debug!("Received probe packet from {}", src);
            continue;
        }

        match DiscoveryPacket::decode(&buf[..len]) {
            Ok(packet) => {
                if let (Some(mac), Some(ip)) = (packet.mac(), packet.ip()) {
                    let is_default = packet.is_default();
                    let model = packet.model();
                    let hostname = packet.hostname();
                    let ssh_port = packet.ssh_port().unwrap_or(22);

                    info!(
                        "Discovery from {}: mac={}, model={:?}, default={}",
                        src, mac, model, is_default
                    );

                    // Update device state
                    let should_adopt = {
                        let mut state_guard = shared.app_state.write().await;
                        let config_guard = shared.daemon_config.read().await;
                        let device = state_guard.get_or_create_device(mac);

                        let was_unknown = device.status == DeviceStatus::Discovered && device.last_seen.is_none();
                        let current_status = device.status;

                        device.last_ip = Some(ip);
                        device.model = model.clone();
                        device.hostname = hostname;
                        device.is_default = is_default;
                        device.ssh_port = Some(ssh_port);
                        device.last_seen = Some(chrono::Utc::now());

                        // Determine if we should auto-adopt (before save to avoid borrow issues)
                        let adopt = config_guard.auto_adopt
                            && is_default
                            && (was_unknown || current_status == DeviceStatus::Discovered);

                        // Save state to disk
                        if let Err(e) = state_guard.save() {
                            error!("Failed to save state: {}", e);
                        }

                        adopt
                    };

                    // Auto-adopt if enabled and device is in default state
                    if should_adopt {
                        info!("Auto-adopting device {} at {}", mac, ip);

                        let state_clone = shared.clone();
                        let mac_clone = mac;

                        // Spawn adoption in background to not block discovery
                        tokio::spawn(async move {
                            crate::adopt::perform_ssh_adoption_flow(state_clone, mac_clone, ip, ssh_port).await;
                        });
                    }
                }
            }
            Err(e) => {
                debug!("Failed to decode discovery packet from {}: {}", src, e);
            }
        }
    }
}
