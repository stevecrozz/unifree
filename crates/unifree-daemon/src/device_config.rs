//! Device configuration types and generation
//!
//! This module handles the declarative configuration for UniFi APs,
//! including WiFi networks, radio settings, and system configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::SshKey;

/// Security mode for a WiFi network
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityMode {
    /// Open network (no security)
    Open,
    /// WPA2-Personal
    Wpa2,
    /// WPA3-Personal (SAE)
    Wpa3,
    /// WPA2/WPA3 transitional mode
    #[default]
    Wpa2Wpa3,
}

impl SecurityMode {
    /// Get the INI config values for this security mode
    pub fn to_ini_values(&self) -> (&'static str, &'static str) {
        match self {
            SecurityMode::Open => ("none", "none"),
            SecurityMode::Wpa2 => ("WPA-PSK", "CCMP"),
            SecurityMode::Wpa3 => ("SAE", "CCMP"),
            SecurityMode::Wpa2Wpa3 => ("WPA-PSK SAE", "CCMP"),
        }
    }
}

/// Radio band
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    /// 2.4 GHz
    #[serde(rename = "2g")]
    Band2g,
    /// 5 GHz
    #[serde(rename = "5g")]
    Band5g,
    /// 6 GHz (WiFi 6E)
    #[serde(rename = "6g")]
    Band6g,
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_provision_config_defaults() {
        let mut config = ProvisionConfig::default();
        
        config.networks.insert("home".to_string(), NetworkConfig {
            ssid: "MyHome".to_string(),
            passphrase: Some("secret123".to_string()),
            security: SecurityMode::Wpa2Wpa3,
            bands: vec![Band::Band2g, Band::Band5g],
            vlan: None,
            hidden: false,
            guest: false,
            pmf: 1,
        });
        
        config.networks.insert("guest".to_string(), NetworkConfig {
            ssid: "Guest".to_string(),
            passphrase: Some("guestpass".to_string()),
            security: SecurityMode::Wpa2,
            bands: vec![Band::Band2g],
            vlan: Some(100),
            hidden: false,
            guest: true,
            pmf: 0,
        });
        
        // Test default networks
        let networks = config.get_networks_for_device("aa:bb:cc:dd:ee:ff");
        assert_eq!(networks.len(), 2);
        assert!(networks.contains_key("home"));
        assert!(networks.contains_key("guest"));
    }
    
    #[test]
    fn test_device_override() {
        let mut config = ProvisionConfig::default();
        
        config.networks.insert("home".to_string(), NetworkConfig {
            ssid: "MyHome".to_string(),
            passphrase: Some("secret123".to_string()),
            security: SecurityMode::Wpa2Wpa3,
            bands: vec![Band::Band2g, Band::Band5g],
            vlan: None,
            hidden: false,
            guest: false,
            pmf: 1,
        });
        
        // Device override - different SSID
        config.devices.insert("aabbccddeeff".to_string(), DeviceOverride {
            name: Some("Living Room AP".to_string()),
            networks: {
                let mut m = HashMap::new();
                m.insert("home".to_string(), NetworkConfig {
                    ssid: "MyHome-LivingRoom".to_string(),
                    passphrase: Some("secret123".to_string()),
                    security: SecurityMode::Wpa2Wpa3,
                    bands: vec![Band::Band2g, Band::Band5g],
                    vlan: None,
                    hidden: false,
                    guest: false,
                    pmf: 1,
                });
                m
            },
            disabled_networks: vec![],
            led_enabled: Some(false),
        });
        
        // Test device-specific config
        let networks = config.get_networks_for_device("aa:bb:cc:dd:ee:ff");
        assert_eq!(networks.len(), 1);
        assert_eq!(networks.get("home").unwrap().ssid, "MyHome-LivingRoom");
        
        // Test LED override
        assert!(!config.get_led_for_device("aa:bb:cc:dd:ee:ff"));
        assert!(config.get_led_for_device("11:22:33:44:55:66")); // default
    }
}
