//! unifree - CLI tool for UniFi AP management
//!
//! This tool communicates with the unifreed daemon to manage UniFi access points.

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

fn list_devices(state_dir: &str) -> anyhow::Result<()> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");
    
    if !devices_file.exists() {
        println!("No devices found (state file doesn't exist)");
        return Ok(());
    }

    let data = std::fs::read_to_string(&devices_file)?;
    let devices: std::collections::HashMap<String, serde_json::Value> = 
        serde_json::from_str(&data)?;

    if devices.is_empty() {
        println!("No devices found");
        return Ok(());
    }

    println!("{:<20} {:<15} {:<15} {:<12} {:<10}", 
        "MAC", "IP", "MODEL", "STATUS", "ONLINE");
    println!("{}", "-".repeat(75));

    for (mac, device) in devices {
        let ip = device["last_ip"]
            .as_str()
            .unwrap_or("-");
        let model = device["model"]
            .as_str()
            .unwrap_or("-");
        let status = device["status"]
            .as_str()
            .unwrap_or("unknown");
        
        // Check if online based on last_seen
        let online = if let Some(last_seen) = device["last_seen"].as_str() {
            if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(last_seen) {
                let age = chrono::Utc::now().signed_duration_since(ts);
                if age.num_seconds() < 60 { "yes" } else { "no" }
            } else {
                "?"
            }
        } else {
            "-"
        };

        println!("{:<20} {:<15} {:<15} {:<12} {:<10}", 
            mac, ip, model, status, online);
    }

    Ok(())
}

fn show_status(state_dir: &str, mac: &str) -> anyhow::Result<()> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");
    
    if !devices_file.exists() {
        println!("No devices found");
        return Ok(());
    }

    let data = std::fs::read_to_string(&devices_file)?;
    let devices: std::collections::HashMap<String, serde_json::Value> = 
        serde_json::from_str(&data)?;

    // Normalize MAC for lookup
    let normalized_mac = mac.replace(":", "").replace("-", "").to_lowercase();
    let search_mac = format!("{}:{}:{}:{}:{}:{}",
        &normalized_mac[0..2], &normalized_mac[2..4], &normalized_mac[4..6],
        &normalized_mac[6..8], &normalized_mac[8..10], &normalized_mac[10..12]);

    if let Some(device) = devices.get(&search_mac) {
        println!("{}", serde_json::to_string_pretty(device)?);
    } else {
        println!("Device {} not found", mac);
    }

    Ok(())
}

async fn adopt_device(
    state_dir: &str, 
    mac: &str, 
    inform_url: Option<&str>,
    ssh_user: &str,
    ssh_pass: &str,
) -> anyhow::Result<()> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");
    
    if !devices_file.exists() {
        anyhow::bail!("No devices found - run the daemon first to discover devices");
    }

    let data = std::fs::read_to_string(&devices_file)?;
    let mut devices: std::collections::HashMap<String, serde_json::Value> = 
        serde_json::from_str(&data)?;

    // Normalize MAC for lookup
    let normalized_mac = mac.replace(":", "").replace("-", "").to_lowercase();
    let search_mac = format!("{}:{}:{}:{}:{}:{}",
        &normalized_mac[0..2], &normalized_mac[2..4], &normalized_mac[4..6],
        &normalized_mac[6..8], &normalized_mac[8..10], &normalized_mac[10..12]);

    let device = devices.get(&search_mac)
        .ok_or_else(|| anyhow::anyhow!("Device {} not found", mac))?;

    let ip = device["last_ip"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Device has no known IP address"))?;
    
    let ssh_port = device["ssh_port"].as_u64().unwrap_or(22) as u16;

    // Determine inform URL - try to auto-detect from daemon or use provided
    let inform_url = match inform_url {
        Some(url) => url.to_string(),
        None => {
            // Try to determine local IP that can reach the device
            format!("http://{}:8080/inform", get_local_ip_for(ip)?)
        }
    };

    println!("Adopting device {} ({}) ...", mac, ip);
    println!("  Inform URL: {}", inform_url);
    println!("  SSH: {}@{}:{}", ssh_user, ip, ssh_port);

    let result = adopt::perform_adoption(
        ip.parse()?,
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
            if let Some(device) = devices.get_mut(&search_mac) {
                device["status"] = serde_json::json!("adopted");
                device["auth_key"] = serde_json::json!(auth_key);
                device["inform_url"] = serde_json::json!(inform_url);
                device["adopted_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            }

            // Save state
            let data = serde_json::to_string_pretty(&devices)?;
            std::fs::write(&devices_file, data)?;

            println!("\nDevice will now inform to {}. Make sure the daemon is running!", inform_url);
        }
        Err(e) => {
            println!("✗ Adoption failed: {}", e);
        }
    }

    Ok(())
}

fn forget_device(state_dir: &str, mac: &str) -> anyhow::Result<()> {
    let devices_file = std::path::Path::new(state_dir).join("devices.json");
    
    if !devices_file.exists() {
        println!("No devices found");
        return Ok(());
    }

    let data = std::fs::read_to_string(&devices_file)?;
    let mut devices: std::collections::HashMap<String, serde_json::Value> = 
        serde_json::from_str(&data)?;

    // Normalize MAC for lookup
    let normalized_mac = mac.replace(":", "").replace("-", "").to_lowercase();
    let search_mac = format!("{}:{}:{}:{}:{}:{}",
        &normalized_mac[0..2], &normalized_mac[2..4], &normalized_mac[4..6],
        &normalized_mac[6..8], &normalized_mac[8..10], &normalized_mac[10..12]);

    if devices.remove(&search_mac).is_some() {
        let data = serde_json::to_string_pretty(&devices)?;
        std::fs::write(&devices_file, data)?;
        println!("Device {} forgotten", mac);
    } else {
        println!("Device {} not found", mac);
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
