use std::fmt;

/// Trait for types that can be converted to a flat list of UniFi config key-value pairs.
pub trait ToUnifiConfig {
    /// Convert to a list of (key, value) pairs.
    /// The keys usually include the index (e.g., "radio.1.status"), which is often handled by the caller 
    /// or passed into a method on the struct if the struct knows its own index.
    /// 
    /// Here we assume the caller handles prefixing if needed, or the struct produces fully qualified keys.
    fn to_config(&self) -> Vec<(String, String)>;
}

/// UniFi boolean that serializes to "enabled" or "disabled".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiBool {
    Enabled,
    Disabled,
}

impl From<bool> for UnifiBool {
    fn from(b: bool) -> Self {
        if b { UnifiBool::Enabled } else { UnifiBool::Disabled }
    }
}

impl fmt::Display for UnifiBool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnifiBool::Enabled => write!(f, "enabled"),
            UnifiBool::Disabled => write!(f, "disabled"),
        }
    }
}

/// Helper to convert generic types to String for value
pub trait ToUnifiValue {
    fn to_unifi_value(&self) -> String;
}

impl ToUnifiValue for String {
    fn to_unifi_value(&self) -> String { self.clone() }
}

impl ToUnifiValue for &str {
    fn to_unifi_value(&self) -> String { self.to_string() }
}

impl ToUnifiValue for u8 {
    fn to_unifi_value(&self) -> String { self.to_string() }
}

impl ToUnifiValue for u32 {
    fn to_unifi_value(&self) -> String { self.to_string() }
}

impl ToUnifiValue for i32 {
    fn to_unifi_value(&self) -> String { self.to_string() }
}

impl ToUnifiValue for UnifiBool {
    fn to_unifi_value(&self) -> String { self.to_string() }
}

impl<T: ToUnifiValue> ToUnifiValue for Option<T> {
    fn to_unifi_value(&self) -> String {
        match self {
            Some(v) => v.to_unifi_value(),
            None => "".to_string(), // Or skip key? Usually skip.
        }
    }
}
