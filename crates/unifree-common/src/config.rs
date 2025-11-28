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

/// Management credentials configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManagementConfig {
    /// Username to use for SSH (both adoption and management)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    
    /// Password to use for SSH
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
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

    /// Management credentials
    #[serde(default)]
    pub management: ManagementConfig,
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
    
    /// Generate system_ini for a device (modern format for U6/U7)
    pub fn generate_system_ini(&self, mac: &str) -> String {
        let networks = self.get_networks_for_device(mac);
        let mut lines = Vec::new();
        
        // 1. Core System & Users
        lines.push("# system".to_string());
        lines.push("system.analytics.status=disabled".to_string());
        if let Some(tz) = &self.country_code {
             // Simplified timezone logic - ideally map country code to TZ
             // For now hardcode a reasonable default or allow override
             let tz_str = "CET-1CEST,M3.5.0,M10.5.0/3"; // Default from dump
             lines.push(format!("system.timezone={}", tz_str));
             lines.push(format!("locale.timezone={}", tz_str));
        }

        // Users - typically handled via ProvisionConfig management struct if we expanded it
        // For now, generate the default admin user from management config
        lines.push("# users".to_string());
        lines.push("users.status=enabled".to_string());
        
        let username = self.management.username.as_deref().unwrap_or("ubnt");
        let password = self.management.password.as_deref().unwrap_or("ubnt");
        // Note: Password should be hashed, but for simplicity/demo we're using plaintext or pre-hashed if provided
        // In reality, we'd want to check if it looks like a hash ($6$...)
        
        lines.push(format!("users.1.name={}", username));
        // If it starts with $6$, assume it's already a hash. If not, we should probably hash it (but we don't have crypto here easily)
        // For now, just pass it through. WARNING: Plaintext passwords might not work if device expects hash
        lines.push(format!("users.1.password={}", password));
        lines.push("users.1.status=enabled".to_string());
        
        // 2. Connectivity & Bridge
        lines.push("# connectivity".to_string());
        lines.push("connectivity.status=enabled".to_string());
        lines.push("connectivity.uplink_bridge=br0".to_string());
        lines.push("connectivity.uplink_eth=eth0".to_string());

        lines.push("# bridge".to_string());
        lines.push("bridge.status=enabled".to_string());
        
        // Default bridge br0
        lines.push("bridge.1.devname=br0".to_string());
        lines.push("bridge.1.fd=1".to_string());
        lines.push("bridge.1.stp.status=disabled".to_string());
        
        // Bridge ports accumulator
        let mut br0_ports = vec!["eth0".to_string()];
        
        // 3. Network Interfaces (Netconf)
        lines.push("# netconf".to_string());
        lines.push("netconf.status=enabled".to_string());
        
        // br0 (management interface)
        lines.push("netconf.1.devname=br0".to_string());
        lines.push("netconf.1.ip=0.0.0.0".to_string()); // DHCP handles this typically via dhcpc
        lines.push("netconf.1.status=enabled".to_string());
        lines.push("netconf.1.up=enabled".to_string());
        
        // eth0
        lines.push("netconf.2.devname=eth0".to_string());
        lines.push("netconf.2.promisc=enabled".to_string());
        lines.push("netconf.2.status=enabled".to_string());
        lines.push("netconf.2.up=enabled".to_string());

        // DHCP Client
        lines.push("# dhcpc".to_string());
        lines.push("dhcpc.status=enabled".to_string());
        lines.push("dhcpc.1.devname=br0".to_string());
        lines.push("dhcpc.1.status=enabled".to_string());

        // 4. Radio & Wireless Configuration
        lines.push("# wlans (radio)".to_string());
        lines.push("radio.status=enabled".to_string());
        lines.push("aaa.status=enabled".to_string());
        lines.push("wireless.status=enabled".to_string());
        
        // Define physical radios
        // U7 Pro Max typically has:
        // radio.1 = 2.4GHz (wifi0)
        // radio.2 = 5GHz (wifi1)
        // radio.3 = 6GHz (wifi2)
        
        struct PhysRadio {
            idx: u8,
            phyname: &'static str,
            mode: &'static str,
            band: Band,
        }
        
        let radios = vec![
            PhysRadio { idx: 1, phyname: "wifi0", mode: "11nght20", band: Band::Band2g },
            PhysRadio { idx: 2, phyname: "wifi1", mode: "11naht40", band: Band::Band5g },
            PhysRadio { idx: 3, phyname: "wifi2", mode: "11naht160", band: Band::Band6g },
        ];

        // Track global unique IDs for aaa/wireless sections
        let mut wireless_idx_counter = 1;
        
        // We also need to track virtual interfaces per radio to assign `virtual.X`
        let mut radio_vap_counters: HashMap<u8, u8> = HashMap::new();

        for radio in &radios {
            lines.push(format!("radio.{}.status=enabled", radio.idx));
            lines.push(format!("radio.{}.phyname={}", radio.idx, radio.phyname));
            lines.push(format!("radio.{}.mode=master", radio.idx));
            lines.push(format!("radio.{}.ieee_mode={}", radio.idx, radio.mode));
            lines.push(format!("radio.{}.channel=auto", radio.idx));
            lines.push(format!("radio.{}.txpower=auto", radio.idx));
            
            // Find networks for this band
            let mut radio_networks: Vec<(&String, &NetworkConfig)> = networks.iter()
                .filter(|(_, n)| n.bands.contains(&radio.band))
                .collect();
            
            // Sort by name for stability
            radio_networks.sort_by_key(|(name, _)| *name);

            // Assign VAPs
            for (i, (net_name, net_config)) in radio_networks.iter().enumerate() {
                // Generate unique devname: wifiXapY
                // We need a unique 'ap' suffix. The dump uses global unique suffixes?
                // wifi0ap0, wifi1ap1, wifi2ap3, wifi0ap5...
                // Let's use a global atomic counter logic for "ap" suffix to keep it simple and unique
                let ap_suffix = wireless_idx_counter - 1; // 0-based suffix for devname
                let devname = format!("{}ap{}", radio.phyname, ap_suffix);
                
                // If it's the first network on this radio, it's the primary devname
                if i == 0 {
                    lines.push(format!("radio.{}.devname={}", radio.idx, devname));
                } else {
                    // It's a virtual interface
                    let vap_idx = radio_vap_counters.entry(radio.idx).or_insert(0);
                    *vap_idx += 1;
                    lines.push(format!("radio.{}.virtual.{}.status=enabled", radio.idx, vap_idx));
                    lines.push(format!("radio.{}.virtual.{}.devname={}", radio.idx, vap_idx, devname));
                    lines.push(format!("radio.{}.virtual.{}.mode=master", radio.idx, vap_idx));
                }
                
                // Add to bridge
                br0_ports.push(devname.clone());
                
                // Wireless Section
                let w_idx = wireless_idx_counter;
                lines.push(format!("wireless.{}.devname={}", w_idx, devname));
                lines.push(format!("wireless.{}.status=enabled", w_idx));
                lines.push(format!("wireless.{}.ssid={}", w_idx, net_config.ssid));
                lines.push(format!("wireless.{}.mode=master", w_idx));
                lines.push(format!("wireless.{}.security=none", w_idx)); // Security logic moved to AAA
                lines.push(format!("wireless.{}.hide_ssid={}", w_idx, net_config.hidden));
                
                // Link to parent wifi physical interface
                // The dump shows `wireless.1.parent=wifi0`
                lines.push(format!("wireless.{}.parent={}", w_idx, radio.phyname));
                
                // AAA Section (Security)
                lines.push(format!("aaa.{}.devname={}", w_idx, devname));
                lines.push(format!("aaa.{}.status=enabled", w_idx));
                lines.push(format!("aaa.{}.ssid={}", w_idx, net_config.ssid));
                
                // Generate a unique ID (random hex) for the network
                use std::time::{SystemTime, UNIX_EPOCH};
                // Deterministic ID based on mac + net_name + band to allow stability?
                // For now, random-ish is fine as long as we don't restart constantly
                let unique_id = format!("{:x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() + w_idx as u128);
                lines.push(format!("aaa.{}.id={}", w_idx, unique_id));

                match net_config.security {
                    SecurityMode::Open => {
                        lines.push(format!("aaa.{}.wpa=0", w_idx));
                    },
                    SecurityMode::Wpa2 => {
                        lines.push(format!("aaa.{}.wpa=2", w_idx));
                        lines.push(format!("aaa.{}.wpa.key.1.mgmt=WPA-PSK", w_idx));
                        lines.push(format!("aaa.{}.wpa.psk={}", w_idx, net_config.passphrase.as_deref().unwrap_or("")));
                        lines.push(format!("aaa.{}.wpa.1.pairwise=CCMP", w_idx));
                    },
                    SecurityMode::Wpa3 => {
                        lines.push(format!("aaa.{}.wpa=2", w_idx));
                        lines.push(format!("aaa.{}.wpa.key.1.mgmt=SAE", w_idx));
                        lines.push(format!("aaa.{}.wpa.psk={}", w_idx, net_config.passphrase.as_deref().unwrap_or("")));
                        lines.push(format!("aaa.{}.wpa.1.pairwise=CCMP", w_idx));
                        lines.push(format!("aaa.{}.wpa3.support=enabled", w_idx));
                        lines.push(format!("aaa.{}.wpa3.transition=disabled", w_idx)); // WPA3 only
                        lines.push(format!("aaa.{}.pmf.status=enabled", w_idx));
                        lines.push(format!("aaa.{}.pmf.mode=2", w_idx)); // Required
                    },
                    SecurityMode::Wpa2Wpa3 => {
                        lines.push(format!("aaa.{}.wpa=2", w_idx));
                        lines.push(format!("aaa.{}.wpa.key.1.mgmt=WPA-PSK SAE", w_idx)); // Both
                        lines.push(format!("aaa.{}.wpa.psk={}", w_idx, net_config.passphrase.as_deref().unwrap_or("")));
                        lines.push(format!("aaa.{}.wpa.1.pairwise=CCMP", w_idx));
                        lines.push(format!("aaa.{}.wpa3.support=enabled", w_idx));
                        lines.push(format!("aaa.{}.wpa3.transition=enabled", w_idx));
                        lines.push(format!("aaa.{}.pmf.status=enabled", w_idx));
                        lines.push(format!("aaa.{}.pmf.mode=1", w_idx)); // Optional
                    }
                }
                
                wireless_idx_counter += 1;
            }
        }
        
        // Finalize bridge ports
        for (i, port) in br0_ports.iter().enumerate() {
            lines.push(format!("bridge.1.port.{}.devname={}", i + 1, port));
        }
        
        // SSH Keys
        if !self.ssh_keys.is_empty() {
            lines.push("# sshd".to_string());
            lines.push("sshd.status=enabled".to_string());
            lines.push("sshd.1.ifname=br0".to_string());
            lines.push("sshd.1.status=enabled".to_string());
            for (i, key) in self.ssh_keys.iter().enumerate() {
                let idx = i + 1;
                lines.push(format!("sshd.auth.key.{}.status=enabled", idx));
                lines.push(format!("sshd.auth.key.{}.type={}", idx, key.key_type));
                lines.push(format!("sshd.auth.key.{}.value={}", idx, key.value));
                if let Some(comment) = &key.comment {
                    lines.push(format!("sshd.auth.key.{}.comment={}", idx, comment));
                }
            }
        }

        lines.join("\n")
    }
    
    // Kept for backward compatibility if needed, but redirects to new logic
    pub fn generate_system_cfg(&self, mac: &str) -> String {
        self.generate_system_ini(mac)
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