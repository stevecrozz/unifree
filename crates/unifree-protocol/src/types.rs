//! Core types for the UniFi protocol

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

// Re-export common types
pub use unifree_common::types::{Band, MacAddress, SecurityMode};

/// Device state in the adoption lifecycle (protocol level)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceState {
    /// Device is in default state (not adopted)
    Unknown = 0,
    /// Device is connected but pending adoption
    Connected = 1,
    /// Device is adopted and provisioned
    Provisioned = 2,
    /// Device is upgrading firmware
    Upgrading = 4,
    /// Device is being provisioned
    Provisioning = 5,
}

impl From<u8> for DeviceState {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Unknown,
            1 => Self::Connected,
            2 => Self::Provisioned,
            4 => Self::Upgrading,
            5 => Self::Provisioning,
            _ => Self::Unknown,
        }
    }
}

/// Inform packet flags
#[derive(Debug, Clone, Copy, Default)]
pub struct InformFlags {
    /// Payload is AES encrypted
    pub encrypted: bool,
    /// Payload is ZLIB compressed
    pub zlib_compressed: bool,
    /// Payload is Snappy compressed
    pub snappy_compressed: bool,
    /// Using AES-GCM (vs AES-CBC)
    pub aes_gcm: bool,
}

impl InformFlags {
    /// Parse flags from raw u16 value
    pub fn from_raw(raw: u16) -> Self {
        Self {
            encrypted: (raw & 0x01) != 0,
            zlib_compressed: (raw & 0x02) != 0,
            snappy_compressed: (raw & 0x04) != 0,
            aes_gcm: (raw & 0x08) != 0,
        }
    }

    /// Convert to raw u16 value
    pub fn to_raw(&self) -> u16 {
        let mut flags = 0u16;
        if self.encrypted {
            flags |= 0x01;
        }
        if self.zlib_compressed {
            flags |= 0x02;
        }
        if self.snappy_compressed {
            flags |= 0x04;
        }
        if self.aes_gcm {
            flags |= 0x08;
        }
        flags
    }
}

/// Discovery payload types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PayloadType {
    Mac = 0x01,
    MacIp = 0x02,
    Firmware = 0x03,
    Uptime = 0x0a,
    Hostname = 0x0b,
    ShortModel2 = 0x0c,
    Sequence = 0x12,
    Serial = 0x13,
    ShortModel = 0x15,
    Version = 0x16,
    IsDefault = 0x17,
    IsLocating = 0x18,
    IsDhcp = 0x19,
    IsDhcpBound = 0x20,
    SshPort = 0x1c,
    Unknown(u8),
}

impl From<u8> for PayloadType {
    fn from(value: u8) -> Self {
        match value {
            0x01 => Self::Mac,
            0x02 => Self::MacIp,
            0x03 => Self::Firmware,
            0x0a => Self::Uptime,
            0x0b => Self::Hostname,
            0x0c => Self::ShortModel2,
            0x12 => Self::Sequence,
            0x13 => Self::Serial,
            0x15 => Self::ShortModel,
            0x16 => Self::Version,
            0x17 => Self::IsDefault,
            0x18 => Self::IsLocating,
            0x19 => Self::IsDhcp,
            0x20 => Self::IsDhcpBound,
            0x1c => Self::SshPort,
            other => Self::Unknown(other),
        }
    }
}

impl From<PayloadType> for u8 {
    fn from(value: PayloadType) -> Self {
        match value {
            PayloadType::Mac => 0x01,
            PayloadType::MacIp => 0x02,
            PayloadType::Firmware => 0x03,
            PayloadType::Uptime => 0x0a,
            PayloadType::Hostname => 0x0b,
            PayloadType::ShortModel2 => 0x0c,
            PayloadType::Sequence => 0x12,
            PayloadType::Serial => 0x13,
            PayloadType::ShortModel => 0x15,
            PayloadType::Version => 0x16,
            PayloadType::IsDefault => 0x17,
            PayloadType::IsLocating => 0x18,
            PayloadType::IsDhcp => 0x19,
            PayloadType::IsDhcpBound => 0x20,
            PayloadType::SshPort => 0x1c,
            PayloadType::Unknown(v) => v,
        }
    }
}

/// Parsed inform request from a device
#[derive(Debug, Clone, Deserialize)]
pub struct InformRequest {
    pub mac: String,
    pub ip: IpAddr,
    pub model: String,
    pub version: String,
    #[serde(default)]
    pub state: u8,
    #[serde(default)]
    pub cfgversion: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub hostname: String,
    pub inform_url: String,
    #[serde(default)]
    pub model_display: String,
    #[serde(default)]
    pub serial: String,
    #[serde(default)]
    pub isolated: bool,
    #[serde(default)]
    pub uptime: u64,
    #[serde(default)]
    pub locating: bool,
    #[serde(default)]
    pub fingerprint_req: bool,
    
    // Optional detailed info
    #[serde(default)]
    pub radio_table: Vec<serde_json::Value>,
    #[serde(default)]
    pub vap_table: Vec<serde_json::Value>,
    #[serde(default)]
    pub if_table: Vec<serde_json::Value>,
    
    // System stats
    #[serde(default, rename = "sys_stats")]
    pub sys_stats: Option<serde_json::Value>,
    #[serde(default, rename = "system-stats")]
    pub system_stats: Option<serde_json::Value>,
    
    // Catch all other fields
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Response to send back to a device
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "_type", rename_all = "lowercase")]
pub enum InformResponse {
    /// No-op heartbeat response
    Noop {
        interval: u32,
        server_time_in_utc: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fingerprint: Option<Vec<String>>,
    },
    /// Configuration update
    Setparam {
        #[serde(skip_serializing_if = "Option::is_none")]
        cfgversion: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        system_cfg: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mgmt_cfg: Option<String>,
        interval: u32,
        server_time_in_utc: String,
    },
    /// Command execution
    Cmd {
        cmd: String,
        server_time_in_utc: String,
        #[serde(flatten)]
        params: std::collections::HashMap<String, serde_json::Value>,
    },
    /// Firmware upgrade
    Upgrade {
        url: String,
        server_time_in_utc: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        md5: Option<String>,
    },
}

/// Get current UTC timestamp as string
fn utc_timestamp() -> String {
    chrono::Utc::now().timestamp().to_string()
}

impl InformResponse {
    /// Create a simple noop response
    pub fn noop(interval: u32) -> Self {
        Self::Noop {
            interval,
            server_time_in_utc: utc_timestamp(),
            fingerprint: None,
        }
    }

    /// Create a noop response with SSH fingerprint
    pub fn noop_with_fingerprint(interval: u32, fingerprint: Vec<String>) -> Self {
        Self::Noop {
            interval,
            server_time_in_utc: utc_timestamp(),
            fingerprint: Some(fingerprint),
        }
    }

    /// Create a configuration update response
    pub fn set_config(system_cfg: Option<String>, mgmt_cfg: Option<String>, interval: u32) -> Self {
        Self::Setparam {
            cfgversion: None,
            system_cfg,
            mgmt_cfg,
            interval,
            server_time_in_utc: utc_timestamp(),
        }
    }

    /// Create a configuration update response with explicit cfgversion
    pub fn set_config_with_version(
        cfgversion: Option<String>,
        system_cfg: Option<String>,
        mgmt_cfg: Option<String>,
        interval: u32,
    ) -> Self {
        Self::Setparam {
            cfgversion,
            system_cfg,
            mgmt_cfg,
            interval,
            server_time_in_utc: utc_timestamp(),
        }
    }

    /// Create a reboot command
    pub fn reboot() -> Self {
        Self::Cmd {
            cmd: "reboot".to_string(),
            server_time_in_utc: utc_timestamp(),
            params: Default::default(),
        }
    }

    /// Create a locate (LED flash) command
    pub fn locate(enabled: bool) -> Self {
        Self::Cmd {
            cmd: if enabled { "set-locate" } else { "unset-locate" }.to_string(),
            server_time_in_utc: utc_timestamp(),
            params: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_address_parsing() {
        let mac1: MacAddress = "1c0b8b8e177f".parse().unwrap();
        let mac2: MacAddress = "1c:0b:8b:8e:17:7f".parse().unwrap();
        let mac3: MacAddress = "1C:0B:8B:8E:17:7F".parse().unwrap();
        
        assert_eq!(mac1, mac2);
        assert_eq!(mac2, mac3);
        assert_eq!(mac1.to_hex_string(), "1c0b8b8e177f");
        assert_eq!(mac1.to_colon_string(), "1c:0b:8b:8e:17:7f");
    }

    #[test]
    fn test_inform_flags() {
        let flags = InformFlags::from_raw(0x0B); // encrypted + zlib + gcm
        assert!(flags.encrypted);
        assert!(flags.zlib_compressed);
        assert!(!flags.snappy_compressed);
        assert!(flags.aes_gcm);
        assert_eq!(flags.to_raw(), 0x0B);
    }
}