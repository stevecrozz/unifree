//! unifree - CLI tool for UniFi AP management
//!
//! This tool communicates with the unifreed daemon to manage UniFi access points.

use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::str::FromStr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use reqwest::Client;
use unifree_config::config::ProvisionConfig;
use unifree_state::{DeviceState, DeviceStatus};
use unifree_types::MacAddress;

mod adopt;

#[derive(Parser, Debug)]
#[command(name = "unifree", about = "UniFi AP management CLI")]
struct Cli {
    /// State directory (must match daemon)
    #[arg(long, default_value = "/var/lib/unifree")]
    state_dir: String,

    /// Configuration file path (optional)
    #[arg(long)]
    config: Option<String>,

    /// Daemon API URL
    #[arg(long, default_value = "http://localhost:8080")]
    daemon_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all known devices
    List,

    /// Show device status
    Status {
        /// Device MAC address
        mac: String,
    },

    /// Adopt a device
    Adopt {
        /// Device MAC address
        mac: String,

        /// Inform URL for the device to connect to
        #[arg(long)]
        inform_url: Option<String>,

        /// SSH username (default: ubnt or from config)
        #[arg(long)]
        ssh_user: Option<String>,

        /// SSH password (default: ubnt or from config)
        #[arg(long)]
        ssh_pass: Option<String>,
    },

    /// Provision a device with current config
    Provision {
        /// Device MAC address
        mac: String,
    },

    /// Upgrade device firmware
    Upgrade {
        /// Device MAC address
        mac: String,
        /// Custom firmware URL
        #[arg(long)]
        url: Option<String>,
        /// Custom version string (required if url is provided)
        #[arg(long)]
        version: Option<String>,
    },

    /// Abandon (forget) a device
    Abandon {
        /// Device MAC address
        mac: String,

        /// Factory reset the device before forgetting
        #[arg(long)]
        factory_reset: bool,
    },

    /// Reboot a device
    Reboot {
        /// Device MAC address
        mac: String,
    },

    /// Toggle device locating LED
    Locate {
        /// Device MAC address
        mac: String,

        /// Enable or disable (default: toggle)
        #[arg(long)]
        enable: Option<bool>,
    },
}

struct ResolvedCredentials {
    user: String,
    pass: String,
}

impl ResolvedCredentials {
    fn resolve(
        arg_user: Option<String>,
        arg_pass: Option<String>,
        config: &Option<ProvisionConfig>,
    ) -> Self {
        let mut user = arg_user;
        let mut pass = arg_pass;

        if let Some(cfg) = config {
            if user.is_none() {
                user = cfg.management.username.clone();
            }
            if pass.is_none() {
                pass = cfg.management.password.clone();
            }
        }

        Self {
            user: user.unwrap_or_else(|| "ubnt".to_string()),
            pass: pass.unwrap_or_else(|| "ubnt".to_string()),
        }
    }
}

#[derive(serde::Deserialize)]
struct DeviceListResponse {
    devices: Vec<DeviceState>,
}

#[derive(serde::Serialize)]
struct UpgradeRequest {
    url: Option<String>,
    version: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "unifree=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let client = Client::new();

    let config = if let Some(path) = &cli.config {
        match ProvisionConfig::from_file(path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Warning: Failed to load config from {}: {}", path, e);
                None
            }
        }
    } else {
        None
    };

    match cli.command {
        Commands::List => {
            let url = format!("{}/api/devices", cli.daemon_url);
            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let list: DeviceListResponse = resp.json().await?;
                        print_device_list(&list.devices);
                    } else {
                        eprintln!("Daemon returned error: {}", resp.status());
                    }
                }
                Err(e) => {
                    eprintln!("Failed to contact daemon at {}: {}", cli.daemon_url, e);
                    eprintln!("Trying local state file fallback...");
                    list_devices_local(&cli.state_dir)?;
                }
            }
        }
        Commands::Status { mac } => {
            show_status(&cli.state_dir, &mac)?;
        }
        Commands::Adopt {
            mac,
            inform_url,
            ssh_user,
            ssh_pass,
        } => {
            let creds = ResolvedCredentials::resolve(ssh_user, ssh_pass, &config);

            // Try API adoption first
            let api_url = format!("{}/api/devices/{}/adopt", cli.daemon_url, mac);
            match client.post(&api_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Adoption triggered via Daemon API. Check logs for progress.");
                }
                Ok(resp) => {
                    eprintln!(
                        "Daemon API returned error: {}. Falling back to local adoption...",
                        resp.status()
                    );
                    adopt_device_local(
                        &cli.state_dir,
                        &mac,
                        inform_url.as_deref(),
                        &creds.user,
                        &creds.pass,
                    )
                    .await?;
                }
                Err(e) => {
                    eprintln!(
                        "Failed to contact daemon: {}. Falling back to local adoption...",
                        e
                    );
                    adopt_device_local(
                        &cli.state_dir,
                        &mac,
                        inform_url.as_deref(),
                        &creds.user,
                        &creds.pass,
                    )
                    .await?;
                }
            }
        }
        Commands::Upgrade { mac, url, version } => {
            let api_url = format!("{}/api/devices/{}/upgrade", cli.daemon_url, mac);
            let payload = UpgradeRequest { url, version };
            let resp = client.post(&api_url).json(&payload).send().await?;
            if resp.status().is_success() {
                println!("Upgrade command sent for {}", mac);
            } else {
                eprintln!("Failed to send upgrade command: {}", resp.status());
            }
        }
        Commands::Provision { mac } => {
            println!("Provisioning device {}...", mac);
            println!("Not yet implemented via CLI");
        }
        Commands::Abandon { mac, factory_reset } => {
            let mut api_url = format!("{}/api/devices/{}", cli.daemon_url, mac);
            if factory_reset {
                api_url.push_str("?reset=true");
            }

            match client.delete(&api_url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Device {} abandoned (reset={}) via API", mac, factory_reset);
                    } else {
                        eprintln!("Failed to abandon device via API: {}", resp.status());
                        if factory_reset {
                            eprintln!("Attempting local reset/abandon fallback...");
                            let creds = ResolvedCredentials::resolve(None, None, &config);
                            reset_and_forget_local(&cli.state_dir, &mac, &creds.user, &creds.pass)
                                .await?;
                        } else {
                            eprintln!("Attempting local forget fallback...");
                            forget_device_local(&cli.state_dir, &mac)?;
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to contact daemon: {}, falling back to local file",
                        e
                    );
                    if factory_reset {
                        let creds = ResolvedCredentials::resolve(None, None, &config);
                        reset_and_forget_local(&cli.state_dir, &mac, &creds.user, &creds.pass)
                            .await?;
                    } else {
                        forget_device_local(&cli.state_dir, &mac)?;
                    }
                }
            }
        }
        Commands::Reboot { mac } => {
            println!("Rebooting device {}...", mac);
            println!("Not yet implemented");
        }
        Commands::Locate { mac, enable } => {
            let action = match enable {
                Some(true) => "enabling",
                Some(false) => "disabling",
                None => "toggling",
            };
            println!("{} locating for device {}...", action, mac);
            println!("Not yet implemented");
        }
    }

    Ok(())
}

fn load_devices_local(state_dir: &str) -> anyhow::Result<HashMap<MacAddress, DeviceState>> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");

    if !devices_file.exists() {
        return Ok(HashMap::new());
    }

    let data = std::fs::read_to_string(&devices_file)?;
    let devices: HashMap<MacAddress, DeviceState> = serde_json::from_str(&data)?;
    Ok(devices)
}

fn save_devices_local(
    state_dir: &str,
    devices: &HashMap<MacAddress, DeviceState>,
) -> anyhow::Result<()> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");
    let data = serde_json::to_string_pretty(devices)?;
    std::fs::write(&devices_file, data)?;
    Ok(())
}

fn print_device_list(devices: &[DeviceState]) {
    if devices.is_empty() {
        println!("No devices found");
        return;
    }

    println!(
        "{:<20} {:<15} {:<15} {:<15} {:<12} {:<10}",
        "MAC", "IP", "MODEL", "VERSION", "STATUS", "ONLINE"
    );
    println!("{}", "-".repeat(90));

    for device in devices {
        let ip = device
            .last_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "-".to_string());
        let model = device.model.as_deref().unwrap_or("-");
        let version = device.version.as_deref().unwrap_or("-");
        let status = device.status.to_string();

        let online = if device.is_online() { "yes" } else { "no" };

        println!(
            "{:<20} {:<15} {:<15} {:<15} {:<12} {:<10}",
            device.mac, ip, model, version, status, online
        );
    }
}

fn list_devices_local(state_dir: &str) -> anyhow::Result<()> {
    let devices_map = load_devices_local(state_dir)?;
    let devices: Vec<DeviceState> = devices_map.values().cloned().collect();
    print_device_list(&devices);
    Ok(())
}

fn show_status(state_dir: &str, mac_str: &str) -> anyhow::Result<()> {
    let devices = load_devices_local(state_dir)?;
    let mac = MacAddress::from_str(mac_str).map_err(|e| anyhow::anyhow!(e))?;

    if let Some(device) = devices.get(&mac) {
        println!("{}", serde_json::to_string_pretty(device)?);
    } else {
        println!("Device {} not found", mac_str);
    }

    Ok(())
}

async fn adopt_device_local(
    state_dir: &str,
    mac_str: &str,
    inform_url: Option<&str>,
    ssh_user: &str,
    ssh_pass: &str,
) -> anyhow::Result<()> {
    let mut devices = load_devices_local(state_dir)?;

    let mac = MacAddress::from_str(mac_str).map_err(|e| anyhow::anyhow!(e))?;
    let device = devices.entry(mac).or_default();
    device.mac = mac;

    let ip = device.last_ip.ok_or_else(|| {
        anyhow::anyhow!(
            "Device {} has no known IP address in state. Cannot adopt locally.",
            mac_str
        )
    })?;

    let ssh_port = device.ssh_port.unwrap_or(22);

    let inform_url = match inform_url {
        Some(url) => url.to_string(),
        None => format!(
            "http://{}:8080/inform",
            get_local_ip_for(ip.to_string().as_str())?
        ),
    };

    println!("Adopting device {} ({}) locally...", mac, ip);
    println!("  Inform URL: {}", inform_url);
    println!("  SSH: {}@{}:{}", ssh_user, ip, ssh_port);

    let result = adopt::perform_adoption(ip, ssh_port, ssh_user, ssh_pass, &inform_url).await;

    match result {
        Ok(auth_key) => {
            println!("✓ Local adoption successful!");
            println!("  Auth key: {}", auth_key);

            device.status = DeviceStatus::Adopted;
            device.auth_key = Some(auth_key);
            device.inform_url = Some(inform_url.clone());
            device.adopted_at = Some(chrono::Utc::now());

            save_devices_local(state_dir, &devices)?;

            println!(
                "\nDevice will now inform to {}. Make sure the daemon is running!",
                inform_url
            );
        }
        Err(e) => {
            println!("✗ Adoption failed: {}", e);
        }
    }

    Ok(())
}

fn forget_device_local(state_dir: &str, mac_str: &str) -> anyhow::Result<()> {
    let mut devices = load_devices_local(state_dir)?;
    let mac = MacAddress::from_str(mac_str).map_err(|e| anyhow::anyhow!(e))?;

    if devices.remove(&mac).is_some() {
        save_devices_local(state_dir, &devices)?;
        println!("Device {} forgotten (local)", mac_str);
    } else {
        println!("Device {} not found (local)", mac_str);
    }

    Ok(())
}

fn get_local_ip_for(dest_ip: &str) -> anyhow::Result<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect(format!("{}:10001", dest_ip))?;
    let local_addr = socket.local_addr()?;
    Ok(local_addr.ip().to_string())
}

async fn reset_and_forget_local(
    state_dir: &str,
    mac_str: &str,
    ssh_user: &str,
    ssh_pass: &str,
) -> anyhow::Result<()> {
    let mut devices = load_devices_local(state_dir)?;
    let mac = MacAddress::from_str(mac_str).map_err(|e| anyhow::anyhow!(e))?;

    let device = devices
        .get(&mac)
        .ok_or_else(|| anyhow::anyhow!("Device {} not found", mac_str))?;

    let ip = device
        .last_ip
        .ok_or_else(|| anyhow::anyhow!("Device has no known IP address"))?;

    let ssh_port = device.ssh_port.unwrap_or(22);

    println!("Factory resetting device {} ({}) locally...", mac, ip);

    match adopt::perform_reset(ip, ssh_port, ssh_user, ssh_pass).await {
        Ok(_) => {
            println!("✓ Reset command sent successfully!");
            println!("Device is rebooting to factory defaults.");

            if devices.remove(&mac).is_some() {
                save_devices_local(state_dir, &devices)?;
                println!("Device {} forgotten from local state.", mac_str);
            }
        }
        Err(e) => {
            println!("✗ Reset failed: {}", e);
            anyhow::bail!("Failed to reset device");
        }
    }

    Ok(())
}
