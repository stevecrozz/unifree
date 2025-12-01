use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// MAC address (6 bytes)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 6 {
            let mut arr = [0u8; 6];
            arr.copy_from_slice(&bytes[..6]);
            Some(Self(arr))
        } else {
            None
        }
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// Convert to lowercase hex string without separators (e.g., "1c0b8b8e177f")
    pub fn to_hex_string(&self) -> String {
        hex::encode(self.0)
    }

    /// Convert to colon-separated string (e.g., "1c:0b:8b:8e:17:7f")
    pub fn to_colon_string(&self) -> String {
        self.0
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(":")
    }
}

impl fmt::Debug for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MacAddress({})", self.to_colon_string())
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_colon_string())
    }
}

impl FromStr for MacAddress {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Support both "1c0b8b8e177f" and "1c:0b:8b:8e:17:7f" formats
        let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if clean.len() != 12 {
            return Err(format!("Invalid MAC address format: {}", s));
        }
        let bytes = hex::decode(&clean).map_err(|_| format!("Invalid MAC address hex: {}", s))?;
        Self::from_bytes(&bytes).ok_or_else(|| format!("Invalid MAC address bytes"))
    }
}

impl Serialize for MacAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_colon_string())
    }
}

impl<'de> Deserialize<'de> for MacAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Radio band
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    /// 2.4 GHz
    #[serde(rename = "2g", alias = "ng")]
    Band2g,
    /// 5 GHz
    #[serde(rename = "5g", alias = "na")]
    Band5g,
    /// 6 GHz (WiFi 6E)
    #[serde(rename = "6g", alias = "6e")]
    Band6g,
}

impl fmt::Display for Band {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Band::Band2g => write!(f, "2g"),
            Band::Band5g => write!(f, "5g"),
            Band::Band6g => write!(f, "6g"),
        }
    }
}

/// Security mode for a WiFi network
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")] // Use lowercase to match protocol expectations (open, wpa2, ...)
pub enum SecurityMode {
    /// Open network (no security)
    Open,
    /// WPA2-Personal
    Wpa2,
    /// WPA3-Personal (SAE)
    Wpa3,
    /// WPA2/WPA3 transitional mode
    #[serde(rename = "wpa2-wpa3")]
    Wpa2Wpa3,
}

impl Default for SecurityMode {
    fn default() -> Self {
        Self::Wpa2Wpa3
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioTableEntry {
    pub radio: String,
    pub name: String,
    pub channel: Option<serde_json::Value>,
    pub tx_power_mode: Option<String>,
    pub ht: Option<serde_json::Value>,
    pub vht: Option<serde_json::Value>,
    pub he: Option<serde_json::Value>,
    pub eht: Option<serde_json::Value>,
    #[serde(alias = "builtin_ant_gain")]
    pub antenna_gain: Option<serde_json::Value>,
    #[serde(rename = "max_tx_power")]
    pub max_tx_power: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntennaTableEntry {
    #[serde(rename = "wifi0_gain")]
    pub wifi0_gain: Option<u8>,
    #[serde(rename = "wifi1_gain")]
    pub wifi1_gain: Option<u8>,
    #[serde(rename = "wifi2_gain")]
    pub wifi2_gain: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub mac: String,
    #[serde(rename = "x_authkey")]
    pub auth_key: Option<String>,
    pub cfgversion: String,
    #[serde(rename = "syslog_key")]
    pub syslog_remote_key: Option<String>,
    #[serde(rename = "inform_url")]
    pub inform_url: Option<String>,
    #[serde(rename = "inform_ip")]
    pub inform_ip: Option<String>,
    #[serde(rename = "x_vwirekey")]
    pub vwire_key: Option<String>,
    #[serde(default)]
    pub antenna_table: Vec<AntennaTableEntry>,
    #[serde(default)]
    pub radio_table: Vec<RadioTableEntry>,
}
