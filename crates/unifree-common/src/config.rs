use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::{Band, SecurityMode};

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
    
    /// PMF (Protected Management Frames) mode
    /// 0 = disabled, 1 = optional, 2 = required
    #[serde(default = "default_pmf")]
    pub pmf: u8,
}

fn default_bands() -> Vec<Band> {
    vec![Band::Band2g, Band::Band5g]
}

fn default_pmf() -> u8 {
    1 // optional
}

/// Device-specific configuration override
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceOverride {
    /// Human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    /// Network overrides (key = network name from defaults)
    #[serde(default)]
    pub networks: HashMap<String, NetworkConfig>,
    
    /// Disable specific default networks
    #[serde(default)]
    pub disabled_networks: Vec<String>,
    
    /// LED enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led_enabled: Option<bool>,
}

/// Complete provisioning configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvisionConfig {
    /// Default networks applied to all devices
    #[serde(default)]
    pub networks: HashMap<String, NetworkConfig>,
    
    /// SSH keys to provision
    #[serde(default)]
    pub ssh_keys: Vec<SshKey>,
    
    /// Per-device overrides (key = MAC address without colons, lowercase)
    #[serde(default)]
    pub devices: HashMap<String, DeviceOverride>,
    
    /// Default LED state
    #[serde(default = "default_true")]
    pub led_enabled: bool,
    
    /// Country code (e.g., "US", "DE")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
}

fn default_true() -> bool { true }
fn default_interface() -> String { "br0".to_string() }

impl ProvisionConfig {
    /// Load from a JSON file
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&contents)?;
        Ok(config)
    }
    
    /// Get effective networks for a device (merging defaults with overrides)
    pub fn get_networks_for_device(&self, mac: &str) -> HashMap<String, NetworkConfig> {
        let normalized_mac = mac.replace(":", "").replace("-", "").to_lowercase();
        
        // Start with defaults
        let mut networks = self.networks.clone();
        
        // Apply device-specific overrides
        if let Some(device) = self.devices.get(&normalized_mac) {
            // Remove disabled networks
            for name in &device.disabled_networks {
                networks.remove(name);
            }
            
            // Override/add networks
            for (name, config) in &device.networks {
                networks.insert(name.clone(), config.clone());
            }
        }
        
        networks
    }
    
    /// Get LED state for a device
    pub fn get_led_for_device(&self, mac: &str) -> bool {
        let normalized_mac = mac.replace(":", "").replace("-", "").to_lowercase();
        
        if let Some(device) = self.devices.get(&normalized_mac) {
            device.led_enabled.unwrap_or(self.led_enabled)
        } else {
            self.led_enabled
        }
    }
    
    /// Generate system_cfg INI for a device
    pub fn generate_system_cfg(&self, mac: &str) -> String {
        let networks = self.get_networks_for_device(mac);
        let mut lines = Vec::new();
        
        // Radio configuration (simplified - real config would need model-specific settings)
        lines.push("# Radio configuration".to_string());
        lines.push("radio.1.status=enabled".to_string());
        lines.push("radio.1.channel=auto".to_string());
        lines.push("radio.1.txpower=auto".to_string());
        
        lines.push("radio.2.status=enabled".to_string());
        lines.push("radio.2.channel=auto".to_string());
        lines.push("radio.2.txpower=auto".to_string());
        
        lines.push("radio.3.status=enabled".to_string());
        lines.push("radio.3.channel=auto".to_string());
        lines.push("radio.3.txpower=auto".to_string());
        
        // AAA (authentication) and wireless config for each network
        lines.push("\n# Wireless networks".to_string());
        
        let mut aaa_idx = 1;
        let mut wireless_idx = 1;
        
        for (name, network) in &networks {
            let (auth_mode, cipher) = network.security.to_ini_values();
            
            // Generate config for each band the network is on
            for band in &network.bands {
                let radio = match band {
                    Band::Band2g => "wifi0",
                    Band::Band5g => "wifi1",
                    Band::Band6g => "wifi2",
                };
                
                // AAA entry
                lines.push(format!("aaa.{}.status=enabled", aaa_idx));
                lines.push(format!("aaa.{}.essid={}", aaa_idx, network.ssid));
                lines.push(format!("aaa.{}.hide_ssid={}", aaa_idx, if network.hidden { "true" } else { "false" }));
                
                if network.security != SecurityMode::Open {
                    if let Some(ref psk) = network.passphrase {
                        lines.push(format!("aaa.{}.wpa.psk={}", aaa_idx, psk));
                    }
                    lines.push(format!("aaa.{}.wpa.key.1.mgmt={}", aaa_idx, auth_mode));
                    lines.push(format!("aaa.{}.wpa.key.1.cipher={}", aaa_idx, cipher));
                }
                
                if let Some(vlan) = network.vlan {
                    lines.push(format!("aaa.{}.vlan={}", aaa_idx, vlan));
                }
                
                if network.guest {
                    lines.push(format!("aaa.{}.l2_isolation=enabled", aaa_idx));
                }
                
                // Wireless entry
                lines.push(format!("wireless.{}.status=enabled", wireless_idx));
                lines.push(format!("wireless.{}.devname={}", wireless_idx, radio));
                lines.push(format!("wireless.{}.security={}", wireless_idx, aaa_idx));
                
                aaa_idx += 1;
                wireless_idx += 1;
            }
        }
        
        // SSH keys
        if !self.ssh_keys.is_empty() {
            lines.push("\n# SSH configuration".to_string());
            lines.push("sshd.status=enabled".to_string());
            lines.push("sshd.1.ifname=br0".to_string());
            lines.push("sshd.1.status=enabled".to_string());
            lines.push("sshd.auth.passwd=enabled".to_string());
        }
        
        lines.join("\n")
    }
    
    /// Generate mgmt_cfg INI for a device
    pub fn generate_mgmt_cfg(&self, mac: &str) -> String {
        self.generate_mgmt_cfg_with_auth(mac, None, None)
    }
    
    /// Generate mgmt_cfg INI for a device with optional auth key and inform URL
    pub fn generate_mgmt_cfg_with_auth(
        &self, 
        mac: &str, 
        auth_key: Option<&str>,
        inform_url: Option<&str>,
    ) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let led_enabled = self.get_led_for_device(mac);
        
        // Generate a config version hash
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cfgversion = format!("{:016x}", timestamp);
        
        let mut lines = Vec::new();
        
        // Required capability flags
        lines.push("capability=notif,fastapply-bg,notif-assoc-stat".to_string());
        lines.push(format!("cfgversion={}", cfgversion));
        lines.push(format!("led_enabled={}", led_enabled));
        lines.push("report_crash=true".to_string());
        lines.push("selfrun_guest_mode=pass".to_string());
        lines.push("use_aes_gcm=true".to_string());
        
        // Auth key for device authentication
        if let Some(key) = auth_key {
            lines.push(format!("authkey={}", key));
        }
        
        // Inform URL
        if let Some(url) = inform_url {
            lines.push(format!("inform_url={}", url));
        }
        
        // SSH keys in mgmt_cfg
        for (i, key) in self.ssh_keys.iter().enumerate() {
            let idx = i + 1;
            lines.push(format!("mgmt.sshkeys.{}.type={}", idx, key.key_type));
            lines.push(format!("mgmt.sshkeys.{}.value={}", idx, key.value));
            if let Some(ref comment) = key.comment {
                lines.push(format!("mgmt.sshkeys.{}.comment={}", idx, comment));
            }
        }
        
        lines.join("\n")
    }
}

/// User configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// Username
    pub name: String,
    /// Password (will be hashed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// SSH public keys for this user
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ssh_keys: Vec<SshKey>,
}

/// SSH daemon configuration  
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshdConfig {
    /// Enable SSH
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Enable password authentication
    #[serde(default = "default_true")]
    pub password_auth: bool,
    /// Interface to bind to
    #[serde(default = "default_interface")]
    pub interface: String,
}

/// Management configuration (mgmt_cfg)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MgmtConfig {
    /// Configuration version (hash)
    pub cfgversion: String,
    /// Management URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mgmt_url: Option<String>,
    /// STUN URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stun_url: Option<String>,
    /// Enable LED
    #[serde(default = "default_true")]
    pub led_enabled: bool,
    /// Use AES-GCM for inform encryption
    #[serde(default = "default_true")]
    pub use_aes_gcm: bool,
    /// Management SSH keys (pushed via mgmt_cfg)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ssh_keys: Vec<SshKey>,
}

/// System configuration (system_cfg)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemConfig {
    /// SSH daemon config
    #[serde(default)]
    pub sshd: SshdConfig,
    /// Users
    #[serde(default)]
    pub users: Vec<UserConfig>,
    /// Timezone
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Parse an SSH public key string into components
/// Format: "ssh-rsa AAAAB3... comment" or "ssh-ed25519 AAAAC3... comment"
pub fn parse_ssh_pubkey(pubkey: &str) -> Option<SshKey> {
    let parts: Vec<&str> = pubkey.trim().splitn(3, ' ').collect();
    
    if parts.len() < 2 {
        return None;
    }
    
    Some(SshKey {
        key_type: parts[0].to_string(),
        value: parts[1].to_string(),
        comment: parts.get(2).map(|s| s.to_string()),
    })
}
