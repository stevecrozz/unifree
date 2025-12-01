use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use unifree_types::{MacAddress, RadioTableEntry};

/// Firmware update information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirmwareUpdateInfo {
    /// Target version string
    pub version: String,
    /// Download URL
    pub url: String,
    /// MD5 checksum (optional but recommended)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
}

/// State of a single device
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceState {
    /// Device MAC address
    pub mac: MacAddress,

    /// Device status
    #[serde(default)]
    pub status: DeviceStatus,

    /// Encryption key (after adoption)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_key: Option<String>,

    /// Inform URL configured for this device
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inform_url: Option<String>,

    /// Last known IP address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ip: Option<IpAddr>,

    /// Last seen timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<DateTime<Utc>>,

    /// Device model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Firmware version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Device hostname
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,

    /// Whether device is in default (factory) state
    #[serde(default)]
    pub is_default: bool,

    /// SSH port
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,

    /// Current configuration version hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_cfgversion: Option<String>,

    /// Target configuration version hash (what we want to push)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_cfgversion: Option<String>,

    /// Target firmware update (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_firmware: Option<FirmwareUpdateInfo>,

    /// When the device was adopted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adopted_at: Option<DateTime<Utc>>,

    /// Radio table reported by the device
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radio_table: Vec<RadioTableEntry>,
}

impl DeviceState {
    /// Check if device needs configuration update
    pub fn needs_config_update(&self) -> bool {
        match (&self.current_cfgversion, &self.target_cfgversion) {
            (Some(current), Some(target)) => current != target,
            (None, Some(_)) => true,
            _ => false,
        }
    }

    /// Check if device is online (seen in last 60 seconds)
    pub fn is_online(&self) -> bool {
        self.last_seen
            .map(|t| Utc::now().signed_duration_since(t).num_seconds() < 60)
            .unwrap_or(false)
    }
}

/// Device adoption/management status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    /// Device discovered but not adopted
    #[default]
    Discovered,
    /// Adoption in progress
    Adopting,
    /// Device adopted and managed
    Adopted,
    /// Device is being provisioned with new config
    Provisioning,
    /// Device is upgrading firmware
    Upgrading,
    /// Device offline (not seen recently)
    Offline,
}

impl std::fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceStatus::Discovered => write!(f, "discovered"),
            DeviceStatus::Adopting => write!(f, "adopting"),
            DeviceStatus::Adopted => write!(f, "adopted"),
            DeviceStatus::Provisioning => write!(f, "provisioning"),
            DeviceStatus::Upgrading => write!(f, "upgrading"),
            DeviceStatus::Offline => write!(f, "offline"),
        }
    }
}
