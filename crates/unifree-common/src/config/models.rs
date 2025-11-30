use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::types::{Band, SecurityMode, RadioTableEntry};
use crate::utils::localization;

/// SSH key configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKey {
    /// Key type (ssh-rsa, ssh-ed25519, etc.)
    pub key_type: String,
    /// Public key value (base64 encoded)
    pub value: String,
    /// Optional comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Management credentials configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManagementConfig {
    /// Username to use for SSH (both adoption and management)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// Password to use for SSH
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// SSH public key (single string from config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key: Option<String>,

    /// STUN URL (e.g. stun://192.168.1.1:3478/)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stun_url: Option<String>,
}

/// WiFi network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network name (SSID)
    pub ssid: String,

    /// Pre-shared key (password)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,

    /// Security mode
    #[serde(default)]
    pub security: SecurityMode,

    /// Radio bands to broadcast on
    #[serde(default = "default_bands")]
    pub bands: Vec<Band>,

    /// VLAN ID (None = untagged)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u16>,

    /// Hide SSID
    #[serde(default)]
    pub hidden: bool,

    /// Guest network isolation
    #[serde(default)]
    pub guest: bool,

    /// Client Device Isolation
    #[serde(default)]
    pub client_device_isolation: bool,

    /// IoT Network Optimization
    #[serde(default)]
    pub iot: bool,

    /// PMF (Protected Management Frames) mode
    /// 0 = disabled, 1 = optional, 2 = required
    #[serde(default = "default_pmf")]
    pub pmf: u8,
}

fn default_bands() -> Vec<Band> {
    vec![Band::Band2g, Band::Band5g]
}

fn default_pmf() -> u8 {
    0 // Disabled by default for compatibility
}

/// Per-device configuration overrides
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceConfig {
    /// Friendly name alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// LED override (true/false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led: Option<bool>,
}

/// Known device info (state)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInfo {
    /// Authentication Key (32 char hex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_key: Option<String>,

    /// Inform IP (last seen IP)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inform_ip: Option<String>,

    /// Inform URL (full URL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inform_url: Option<String>,

    /// Config version (last known)
    #[serde(default)]
    pub cfgversion: String,

    /// Syslog remote encryption key (if enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syslog_remote_key: Option<String>,
    
    /// Radio Table (static config override)
    #[serde(default)]
    pub radio_table: Vec<RadioTableEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProvisionConfig {
    /// Management configuration
    pub management: ManagementConfig,

    /// Country code (e.g., "US", "DE")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,

    /// Timezone (e.g., "Europe/Berlin" or UniFi string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    
    /// Global NTP servers
    #[serde(default)]
    pub ntp_servers: Vec<String>,

    /// Global SSH authorized keys to add to devices
    #[serde(default)]
    pub ssh_keys: Vec<SshKey>,

    /// Per-network configurations (keyed by network name, e.g. "guest", "iot")
    #[serde(default)]
    pub networks: HashMap<String, NetworkConfig>,

    /// Per-device overrides (keyed by MAC address, normalized to lowercase hex)
    #[serde(default)]
    pub devices: HashMap<String, DeviceConfig>,
    
    /// Known device information (keyed by MAC address)
    #[serde(default)]
    pub devices_info: HashMap<String, DeviceInfo>,
}

impl ProvisionConfig {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn load_devices_json(path: &str) -> anyhow::Result<HashMap<String, DeviceInfo>> {
        if !std::path::Path::new(path).exists() {
             return Ok(HashMap::new());
        }
        let content = std::fs::read_to_string(path)?;
        let devices: HashMap<String, DeviceInfo> = serde_json::from_str(&content)?;
        Ok(devices)
    }

    pub fn get_networks_for_device(&self, _mac: &str) -> HashMap<&String, &NetworkConfig> {
        self.networks.iter().collect()
    }

    pub fn get_led_for_device(&self, mac: &str) -> bool {
        let normalized_mac = mac.replace(":", "").to_lowercase();
        if let Some(device) = self.devices.get(&normalized_mac) {
            if let Some(led) = device.led {
                return led;
            }
        }
        true // Default enabled
    }
    
    /// Get the effective numeric country code for radios.
    /// Priority:
    /// 1. Configured country_code (mapped to numeric)
    /// 2. Default: 840 (US)
    pub fn get_effective_country_code_numeric(&self) -> u16 {
        if let Some(iso) = &self.country_code {
            localization::get_numeric_country_code(iso)
        } else {
            840 // Default US
        }
    }
    
    /// Get the effective POSIX timezone string for system.cfg.
    /// Priority:
    /// 1. Configured timezone (if matches POSIX format or is IANA mappable)
    /// 2. Configured country_code (mapped to default IANA -> POSIX)
    /// 3. System detected timezone (mapped IANA -> POSIX)
    /// 4. Default: UTC
    pub fn get_effective_timezone_posix(&self) -> String {
        // 1. Explicit Timezone
        if let Some(tz) = &self.timezone {
            if tz.contains("M") || tz.starts_with("UTC") || tz.contains("GMT") {
                 return tz.clone();
            }
            if let Some(posix) = localization::iana_to_posix_tz(tz) {
                return posix.to_string();
            }
            return tz.clone();
        }
        
        // 2. Country Code Inference
        if let Some(cc) = &self.country_code {
            let iana = localization::get_default_timezone_for_country(cc);
            if let Some(posix) = localization::iana_to_posix_tz(iana) {
                return posix.to_string();
            }
        }
        
        // 3. System Detection
        if let Some(sys_iana) = localization::detect_system_timezone() {
             if let Some(posix) = localization::iana_to_posix_tz(&sys_iana) {
                return posix.to_string();
            }
        }
        
        "UTC0".to_string()
    }
}

/// Helper to parse a standard SSH public key string (e.g. "ssh-rsa AAA... comment")
pub fn parse_ssh_pubkey(line: &str) -> Option<SshKey> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.len() >= 2 {
        Some(SshKey {
            key_type: parts[0].to_string(),
            value: parts[1].to_string(),
            comment: if parts.len() > 2 { Some(parts[2..].join(" ")) } else { None },
        })
    } else {
        None
    }
}
