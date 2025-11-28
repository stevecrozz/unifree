//! Device configuration generation
//!
//! Generates INI-format configuration files for UniFi devices.

use serde::{Deserialize, Serialize};

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

fn default_true() -> bool { true }
fn default_interface() -> String { "br0".to_string() }

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

impl MgmtConfig {
    /// Generate INI format string
    pub fn to_ini(&self) -> String {
        let mut lines = Vec::new();
        
        lines.push(format!("cfgversion={}", self.cfgversion));
        
        if let Some(ref url) = self.mgmt_url {
            lines.push(format!("mgmt_url={}", url));
        }
        
        if let Some(ref url) = self.stun_url {
            lines.push(format!("stun_url={}", url));
        }
        
        lines.push(format!("led_enabled={}", self.led_enabled));
        lines.push(format!("use_aes_gcm={}", self.use_aes_gcm));
        
        // SSH keys in mgmt_cfg format
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

impl SystemConfig {
    /// Generate INI format string (partial - just the sections we manage)
    pub fn to_ini(&self) -> String {
        let mut lines = Vec::new();
        
        // SSH daemon
        lines.push(format!("sshd.status={}", if self.sshd.enabled { "enabled" } else { "disabled" }));
        lines.push(format!("sshd.1.ifname={}", self.sshd.interface));
        lines.push(format!("sshd.1.status={}", if self.sshd.enabled { "enabled" } else { "disabled" }));
        lines.push(format!("sshd.auth.passwd={}", if self.sshd.password_auth { "enabled" } else { "disabled" }));
        
        // Users
        if !self.users.is_empty() {
            lines.push("users.status=enabled".to_string());
            
            for (i, user) in self.users.iter().enumerate() {
                let idx = i + 1;
                lines.push(format!("users.{}.name={}", idx, user.name));
                
                if let Some(ref pass) = user.password {
                    // Password should already be hashed (SHA-512)
                    lines.push(format!("users.{}.password={}", idx, pass));
                }
                
                lines.push(format!("users.{}.status=enabled", idx));
                
                // User SSH keys
                for (j, key) in user.ssh_keys.iter().enumerate() {
                    let key_idx = j + 1;
                    lines.push(format!("users.{}.sshkeys.{}.type={}", idx, key_idx, key.key_type));
                    lines.push(format!("users.{}.sshkeys.{}.value={}", idx, key_idx, key.value));
                    if let Some(ref comment) = key.comment {
                        lines.push(format!("users.{}.sshkeys.{}.comment={}", idx, key_idx, comment));
                    }
                }
            }
        }
        
        // Timezone
        if let Some(ref tz) = self.timezone {
            lines.push(format!("system.timezone={}", tz));
        }
        
        lines.join("\n")
    }
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

/// Hash a password using SHA-512 crypt format (like mkpasswd -m sha-512)
pub fn hash_password(password: &str) -> String {
    // Generate a random salt
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    // Simple salt generation (in production, use a proper random source)
    let salt: String = format!("{:x}", timestamp)
        .chars()
        .take(16)
        .collect();
    
    // Note: For proper SHA-512 crypt, you'd need the `sha-crypt` crate
    // For now, return a placeholder that indicates hashing is needed
    format!("$6${}$<HASH_PLACEHOLDER>", salt)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_ssh_pubkey_rsa() {
        let key = "ssh-rsa AAAAB3NzaC1yc2EAAAA user@host";
        let parsed = parse_ssh_pubkey(key).unwrap();
        
        assert_eq!(parsed.key_type, "ssh-rsa");
        assert_eq!(parsed.value, "AAAAB3NzaC1yc2EAAAA");
        assert_eq!(parsed.comment, Some("user@host".to_string()));
    }
    
    #[test]
    fn test_parse_ssh_pubkey_ed25519() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA";
        let parsed = parse_ssh_pubkey(key).unwrap();
        
        assert_eq!(parsed.key_type, "ssh-ed25519");
        assert_eq!(parsed.value, "AAAAC3NzaC1lZDI1NTE5AAAA");
        assert_eq!(parsed.comment, None);
    }
    
    #[test]
    fn test_mgmt_config_to_ini() {
        let config = MgmtConfig {
            cfgversion: "abc123".to_string(),
            led_enabled: true,
            use_aes_gcm: true,
            ssh_keys: vec![
                SshKey {
                    key_type: "ssh-ed25519".to_string(),
                    value: "AAAAC3NzaC1lZDI1NTE5AAAA".to_string(),
                    comment: Some("admin".to_string()),
                },
            ],
            ..Default::default()
        };
        
        let ini = config.to_ini();
        assert!(ini.contains("cfgversion=abc123"));
        assert!(ini.contains("mgmt.sshkeys.1.type=ssh-ed25519"));
        assert!(ini.contains("mgmt.sshkeys.1.value=AAAAC3NzaC1lZDI1NTE5AAAA"));
        assert!(ini.contains("mgmt.sshkeys.1.comment=admin"));
    }
}
