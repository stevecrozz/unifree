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
    routing::{get, post},
    Router,
};
use clap::Parser;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use sha2::{Sha256, Digest};

mod adopt;
mod api;
mod config;
mod discovery;
mod state;
mod tasks;
mod utils;

use crate::api::inform::{handle_inform};
use crate::api::inform::health_check;
use crate::discovery::run_discovery_listener;
use crate::tasks::{run_config_reloader, run_stale_adoption_cleanup};
use crate::utils::get_local_ip;
use config::ProvisionConfig;
use state::AppState;

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
pub struct SharedState {
    pub app_state: RwLock<AppState>,
    pub daemon_config: RwLock<DaemonConfig>,
    pub log_dir: std::path::PathBuf,
    pub config_hash: RwLock<String>,
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
        if let Some(key) = crate::config::models::parse_ssh_pubkey(key_str) {
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
                    if let Some(key) = crate::config::models::parse_ssh_pubkey(line) {
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

    // Start stale adoption cleanup in background
    let cleanup_state = shared_state.clone();
    tokio::spawn(async move {
        run_stale_adoption_cleanup(cleanup_state).await;
    });

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(&args.http_addr).await?;
    info!("Listening on {}", args.http_addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}
