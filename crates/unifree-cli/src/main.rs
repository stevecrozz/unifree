//! unifree - CLI tool for UniFi AP management
//! 
//! This tool communicates with the unifreed daemon to manage UniFi access points.

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::collections::HashMap;
use std::str::FromStr;

use unifree_common::state::{DeviceState, DeviceStatus};
use unifree_common::types::MacAddress;

mod adopt;

#[derive(Parser, Debug)]
#[command(name = "unifree", about = "UniFi AP management CLI")]
struct Cli {
    /// State directory (must match daemon)
    #[arg(long, default_value = "/var/lib/unifree")]
    state_dir: String,

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
        
        /// SSH username (default: ubnt)
        #[arg(long, default_value = "ubnt")]
        ssh_user: String,
        
        /// SSH password (default: ubnt)
        #[arg(long, default_value = "ubnt")]
        ssh_pass: String,
    },

    /// Provision a device with current config
    Provision {
        /// Device MAC address
        mac: String,
    },

    /// Forget a device
    Forget {
        /// Device MAC address
        mac: String,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "unifree=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::List => {
            list_devices(&cli.state_dir)?;
        }
        Commands::Status { mac } => {
            show_status(&cli.state_dir, &mac)?;
        }
        Commands::Adopt { mac, inform_url, ssh_user, ssh_pass } => {
            adopt_device(&cli.state_dir, &mac, inform_url.as_deref(), &ssh_user, &ssh_pass).await?;
        }
        Commands::Provision { mac } => {
            println!("Provisioning device {}...", mac);
            // TODO: Implement
            println!("Not yet implemented");
        }
        Commands::Forget { mac } => {
            forget_device(&cli.state_dir, &mac)?;
        }
        Commands::Reboot { mac } => {
            println!("Rebooting device {}...", mac);
            // TODO: Implement
            println!("Not yet implemented");
        }
        Commands::Locate { mac, enable } => {
            let action = match enable {
                Some(true) => "enabling",
                Some(false) => "disabling",
                None => "toggling",
            };
            println!("{} locating for device {}...", action, mac);
            // TODO: Implement
            println!("Not yet implemented");
        }
    }

    Ok(())
}

fn load_devices(state_dir: &str) -> anyhow::Result<HashMap<MacAddress, DeviceState>> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");
    
    if !devices_file.exists() {
        return Ok(HashMap::new());
    }

    let data = std::fs::read_to_string(&devices_file)?;
    let devices: HashMap<MacAddress, DeviceState> = serde_json::from_str(&data)?;
    Ok(devices)
}

fn save_devices(state_dir: &str, devices: &HashMap<MacAddress, DeviceState>) -> anyhow::Result<()> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");
    let data = serde_json::to_string_pretty(devices)?;
    std::fs::write(&devices_file, data)?;
    Ok(())
}

fn list_devices(state_dir: &str) -> anyhow::Result<()> {
    let devices = load_devices(state_dir)?;

    if devices.is_empty() {
        println!("No devices found");
        return Ok(())
    }

    println!("{:<20} {:<15} {:<15} {:<12} {:<10}", 
        "MAC", "IP", "MODEL", "STATUS", "ONLINE");
    println!("{}", "-".repeat(75));

    for (mac, device) in devices {
        let ip = device.last_ip.map(|ip| ip.to_string()).unwrap_or_else(|| "-".to_string());
        let model = device.model.as_deref().unwrap_or("-");
        let status = device.status.to_string();
        
        let online = if device.is_online() { "yes" } else { "no" };

        println!("{:<20} {:<15} {:<15} {:<12} {:<10}", 
            mac, ip, model, status, online);
    }

    Ok(())
}

fn show_status(state_dir: &str, mac_str: &str) -> anyhow::Result<()> {
    let devices = load_devices(state_dir)?;
    let mac = MacAddress::from_str(mac_str).map_err(|e| anyhow::anyhow!(e))?;

    if let Some(device) = devices.get(&mac) {
        println!("{}", serde_json::to_string_pretty(device)?);
    } else {
        println!("Device {} not found", mac_str);
    }

    Ok(())
}

async fn adopt_device(
    state_dir: &str, 
    mac_str: &str, 
    inform_url: Option<&str>,
    ssh_user: &str,
    ssh_pass: &str,
) -> anyhow::Result<()> {
    let mut devices = load_devices(state_dir)?;
    if devices.is_empty() {
        anyhow::bail!("No devices found - run the daemon first to discover devices");
    }

    let mac = MacAddress::from_str(mac_str).map_err(|e| anyhow::anyhow!(e))?;

    let device = devices.get_mut(&mac)
        .ok_or_else(|| anyhow::anyhow!("Device {} not found", mac_str))?;

    let ip = device.last_ip
        .ok_or_else(|| anyhow::anyhow!("Device has no known IP address"))?;
    
    let ssh_port = device.ssh_port.unwrap_or(22);

    // Determine inform URL - try to auto-detect from daemon or use provided
    let inform_url = match inform_url {
        Some(url) => url.to_string(),
        None => {
            // Try to determine local IP that can reach the device
            format!("http://{}:8080/inform", get_local_ip_for(ip.to_string().as_str())?)
        }
    };

    println!("Adopting device {} ({}) ...", mac, ip);
    println!("  Inform URL: {}", inform_url);
    println!("  SSH: {}@{}:{}", ssh_user, ip, ssh_port);

    let result = adopt::perform_adoption(
        ip,
        ssh_port,
        ssh_user,
        ssh_pass,
        &inform_url,
    ).await;

    match result {
        Ok(auth_key) => {
            println!("✓ Adoption successful!");
            println!("  Auth key: {}", auth_key);

            // Update state
            device.status = DeviceStatus::Adopted;
            device.auth_key = Some(auth_key);
            device.inform_url = Some(inform_url.clone());
            device.adopted_at = Some(chrono::Utc::now());

            // Save state
            save_devices(state_dir, &devices)?;

            println!("\nDevice will now inform to {}. Make sure the daemon is running!", inform_url);
        }
        Err(e) => {
            println!("✗ Adoption failed: {}", e);
        }
    }

    Ok(())
}

fn forget_device(state_dir: &str, mac_str: &str) -> anyhow::Result<()> {
    let mut devices = load_devices(state_dir)?;
    let mac = MacAddress::from_str(mac_str).map_err(|e| anyhow::anyhow!(e))?;

    if devices.remove(&mac).is_some() {
        save_devices(state_dir, &devices)?;
        println!("Device {} forgotten", mac_str);
    } else {
        println!("Device {} not found", mac_str);
    }

    Ok(())
}

/// Try to determine the local IP address that can reach a given destination
fn get_local_ip_for(dest_ip: &str) -> anyhow::Result<String> {
    use std::net::UdpSocket;
    
    // Create a UDP socket and "connect" to the destination
    // This doesn't actually send anything but lets us find the local IP
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect(format!("{}:10001", dest_ip))?;
    
    let local_addr = socket.local_addr()?;
    Ok(local_addr.ip().to_string())
}