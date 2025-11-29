//! unifreed - UniFi AP management daemon
//!
//! This daemon handles:
//! - Discovery packets from UniFi devices
//! - Inform requests (HTTP POST /inform)
//! - Device state management
//! - Auto-adoption and provisioning

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use clap::Parser;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use unifree_protocol::{
    crypto::AesKey,
    inform::{InformPacket, InformResponseBuilder},
    InformResponse, MacAddress,
};
use sha2::{Sha256, Digest};

mod adopt;
mod config;
mod device_config;
mod state;

use config::SshKey;
use device_config::ProvisionConfig;
use state::{AppState, DeviceStatus};

/// Daemon configuration (runtime)
#[derive(Clone)]
pub struct DaemonConfig {
    /// Auto-adopt new devices on discovery
    pub auto_adopt: bool,
    /// Inform URL to advertise to devices
    pub inform_url: String,
    /// SSH credentials for adoption
    pub ssh_user: String,
    pub ssh_pass: String,
    /// Provisioning config (networks, SSH keys, etc.)
    pub provision: ProvisionConfig,
}

#[derive(Parser, Debug)]
#[command(name = "unifreed", about = "UniFi AP management daemon")]
struct Args {
    /// HTTP listen address
    #[arg(long, default_value = "0.0.0.0:8080")]
    http_addr: SocketAddr,

    /// Discovery listen port
    #[arg(long, default_value = "10001")]
    discovery_port: u16,

    /// State directory
    #[arg(long, default_value = "/var/lib/unifree")]
    state_dir: String,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
    
    /// Auto-adopt new devices when discovered
    #[arg(long)]
    auto_adopt: bool,
    
    /// Inform URL for devices (auto-detected if not specified)
    #[arg(long)]
    inform_url: Option<String>,
    
    /// SSH public keys to provision (can be specified multiple times)
    #[arg(long = "ssh-key")]
    ssh_keys: Vec<String>,
    
    /// File containing SSH public keys (one per line)
    #[arg(long = "ssh-key-file")]
    ssh_key_files: Vec<String>,
    
    /// SSH username for adoption (default: ubnt or from config)
    #[arg(long)]
    ssh_user: Option<String>,
    
    /// SSH password for adoption (default: ubnt or from config)
    #[arg(long)]
    ssh_pass: Option<String>,
    
    /// Configuration file (JSON) for networks and device settings
    #[arg(long = "config")]
    config_file: Option<String>,
}

/// Shared state for handlers
struct SharedState {
    app_state: RwLock<AppState>,
    daemon_config: RwLock<DaemonConfig>,
    log_dir: std::path::PathBuf,
    config_hash: RwLock<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| format!("unifreed={},unifree_protocol=debug", args.log_level).into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting unifreed v{}", env!("CARGO_PKG_VERSION"));
    info!("HTTP endpoint: http://{}/inform", args.http_addr);
    info!("Discovery port: {}", args.discovery_port);

    // Load provision config from file or create default
    let mut provision = if let Some(ref config_path) = args.config_file {
        match ProvisionConfig::from_file(config_path) {
            Ok(config) => {
                info!("Loaded config from {}", config_path);
                config
            }
            Err(e) => {
                error!("Failed to load config file {}: {}", config_path, e);
                ProvisionConfig::default()
            }
        }
    } else {
        ProvisionConfig::default()
    };
    
    // Add SSH keys from command line
    for key_str in &args.ssh_keys {
        if let Some(key) = config::parse_ssh_pubkey(key_str) {
            provision.ssh_keys.push(key);
        } else {
            warn!("Invalid SSH key format: {}", key_str);
        }
    }
    
    // Add SSH keys from files
    for file_path in &args.ssh_key_files {
        match std::fs::read_to_string(file_path) {
            Ok(contents) => {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some(key) = config::parse_ssh_pubkey(line) {
                        provision.ssh_keys.push(key);
                    } else {
                        warn!("Invalid SSH key in {}: {}", file_path, line);
                    }
                }
            }
            Err(e) => {
                error!("Failed to read SSH key file {}: {}", file_path, e);
            }
        }
    }
    
    // Log what we have
    if !provision.ssh_keys.is_empty() {
        info!("Loaded {} SSH key(s) for provisioning", provision.ssh_keys.len());
    }
    if !provision.networks.is_empty() {
        info!("Loaded {} default network(s)", provision.networks.len());
        for (name, net) in &provision.networks {
            info!("  - {}: SSID={}, security={:?}, bands={:?}", 
                name, net.ssid, net.security, net.bands);
        }
    }
    
    // Determine inform URL
    let inform_url = args.inform_url.unwrap_or_else(|| {
        format!("http://{}:{}/inform", 
            get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string()),
            args.http_addr.port())
    });
    info!("Inform URL: {}", inform_url);
    
    if args.auto_adopt {
        info!("Auto-adopt enabled: new devices will be automatically adopted");
    }

    // Resolve credentials
    let ssh_user = args.ssh_user
        .or_else(|| provision.management.username.clone())
        .unwrap_or_else(|| "ubnt".to_string());

    let ssh_pass = args.ssh_pass
        .or_else(|| provision.management.password.clone())
        .unwrap_or_else(|| "ubnt".to_string());

    info!("Using SSH credentials: user={}", ssh_user);

    // Create session log directory
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let log_dir = std::path::Path::new("logs").join(&timestamp);
    std::fs::create_dir_all(&log_dir)?;
    info!("Session logs will be written to: {}", log_dir.display());

    // Calculate config hash (SHA256 of the ProvisionConfig content)
    // We hash the raw file content to ensure stability and avoid JSON serialization order issues
    let config_path = args.config_file.as_deref().unwrap_or("examples/config.json");
    let config_bytes = match std::fs::read(config_path) {
        Ok(b) => b,
        Err(e) => {
            // If file doesn't exist (using default), serialize the default config
            warn!("Could not read config file {} for hashing: {}, using default serialization", config_path, e);
            serde_json::to_vec(&provision).unwrap_or_default()
        }
    };
    
    let mut hasher = Sha256::new();
    hasher.update(&config_bytes);
    let config_hash = hex::encode(&hasher.finalize()[0..8]);
    info!("Initial config hash: {}", config_hash);

    // Build daemon config
    let daemon_config = DaemonConfig {
        auto_adopt: args.auto_adopt,
        inform_url,
        ssh_user,
        ssh_pass,
        provision,
    };

    // Initialize state
    let shared_state = Arc::new(SharedState {
        app_state: RwLock::new(AppState::new(&args.state_dir)?),
        daemon_config: RwLock::new(daemon_config),
        log_dir,
        config_hash: RwLock::new(config_hash),
    });

    // Build HTTP router
    let app = Router::new()
        .route("/inform", post(handle_inform))
        .route("/health", get(health_check))
        .with_state(shared_state.clone());

    // Start discovery listener in background
    let discovery_state = shared_state.clone();
    let discovery_port = args.discovery_port;
    tokio::spawn(async move {
        if let Err(e) = run_discovery_listener(discovery_port, discovery_state).await {
            error!("Discovery listener error: {}", e);
        }
    });

    // Start config reloader in background
    if let Some(config_path) = args.config_file.clone() {
        let reloader_state = shared_state.clone();
        tokio::spawn(async move {
            run_config_reloader(config_path, reloader_state).await;
        });
    }

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(&args.http_addr).await?;
    info!("Listening on {}", args.http_addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}

/// Get local IP address (for auto-detecting inform URL)
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Connect to a public IP to determine local interface
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// Handle incoming inform requests
async fn handle_inform(
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
    
    if let Err(e) = std::fs::write(
        log_prefix.with_extension("raw.bin"), 
        &body
    ) {
        error!("Failed to write raw inform log: {}", e);
    }

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
                if let Err(e) = std::fs::write(
                    log_prefix.with_extension("decrypted.json"),
                    json
                ) {
                    error!("Failed to write decrypted inform log: {}", e);
                }
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
                            let _ = std::fs::write(
                                log_prefix.with_extension("decrypted.json"),
                                json
                            );
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

    // Determine response based on device state
    // If device has our auth_key and is adopted, we should push config (even if request.default is true)
    let has_our_key = device.auth_key.is_some();
    let should_adopt_now = has_our_key && device.status == DeviceStatus::Adopted && request.default;
    
    // Use the global config hash as target
    device.target_cfgversion = Some(current_config_hash.clone());
    
    let should_update = has_our_key && device.status == DeviceStatus::Adopted && 
        !request.default && should_push_config(device);
    
    let response = if should_adopt_now || should_update {
        // Device needs configuration - either initial adoption or update
        if should_adopt_now {
            info!("Device {} is ready for adoption, pushing initial configuration", mac);
        } else {
            info!("Pushing configuration update to device {}", mac);
        }
        device.status = DeviceStatus::Provisioning;
        
        let mac_str = mac.to_string();
        let system_cfg = config_guard.provision.generate_system_ini(&mac_str);
        
        // Use the global config hash
        let cfgversion = current_config_hash;
        
        // Include auth_key and inform_url in mgmt_cfg for adoption
        let auth_key = device.auth_key.as_deref();
        let inform_url = Some(config_guard.inform_url.as_str());
        let mgmt_cfg = config_guard.provision.generate_mgmt_cfg_with_auth(
            &mac_str, 
            auth_key, 
            inform_url,
            &cfgversion,
        );
        
        // Log configs to files for debugging
        if let Err(e) = std::fs::write(
            log_prefix.with_extension("system.ini"), 
            &system_cfg
        ) {
            error!("Failed to write system.ini log: {}", e);
        }
        if let Err(e) = std::fs::write(
            log_prefix.with_extension("mgmt.ini"), 
            &mgmt_cfg
        ) {
            error!("Failed to write mgmt.ini log: {}", e);
        }

        debug!("system_cfg:\n{}", system_cfg);
        debug!("mgmt_cfg:\n{}", mgmt_cfg);
        
        // Send both system_cfg and mgmt_cfg with top-level cfgversion
        // Pass None for interval to match official controller behavior for setparam
        InformResponse::set_config_with_version(
            Some(cfgversion), 
            Some(system_cfg), 
            Some(mgmt_cfg), 
            None
        )
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
    
    // Build encrypted response - match the device's encryption and compression modes
    let use_gcm = packet.flags.aes_gcm;
    let use_compression = packet.flags.zlib_compressed;
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
fn should_push_config(device: &state::DeviceState) -> bool {
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

/// Run the discovery listener
async fn run_discovery_listener(
    port: u16,
    shared: Arc<SharedState>,
) -> anyhow::Result<()> {
    use std::net::Ipv4Addr;
    use tokio::net::UdpSocket;
    use unifree_protocol::discovery::DiscoveryPacket;

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
                            auto_adopt_device(state_clone, mac_clone, ip, ssh_port).await;
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

/// Run the config reloader
async fn run_config_reloader(path: String, shared: Arc<SharedState>) {
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

/// Auto-adopt a device via SSH
async fn auto_adopt_device(
    shared: Arc<SharedState>,
    mac: MacAddress,
    ip: std::net::IpAddr,
    ssh_port: u16,
) {
    debug!("auto_adopt_device called for {} at {}", mac, ip);
    
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

    // Perform SSH adoption directly with timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        adopt::perform_ssh_adoption(
            ip,
            ssh_port,
            &ssh_user,
            &ssh_pass,
            &inform_url,
        )
    ).await;
    
    // Update state based on result
    let mut state_guard = shared.app_state.write().await;
    if let Some(device) = state_guard.devices.get_mut(&mac) {
        match result {
            Ok(Ok(auth_key)) => {
                device.status = DeviceStatus::Adopted;
                device.auth_key = Some(auth_key);
                device.inform_url = Some(inform_url);
                device.adopted_at = Some(chrono::Utc::now());
                info!("Auto-adoption of {} successful!", mac);
            }
            Ok(Err(e)) => {
                device.status = DeviceStatus::Discovered;
                warn!("Auto-adoption of {} failed: {}", mac, e);
            }
            Err(_) => {
                device.status = DeviceStatus::Discovered;
                warn!("Auto-adoption of {} timed out after 30s", mac);
            }
        }
        let _ = state_guard.save();
    }
}