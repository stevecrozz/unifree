use crate::config::models::ProvisionConfig;
use crate::state::DeviceState;
use std::collections::HashMap;

pub mod system;
pub mod radio;
pub mod wireless;
pub mod network;
pub mod services;

/// Trait for generating INI configuration sections
pub trait IniSection {
    /// Generate the INI lines for this section, returning a map of Header -> Lines
    fn generate(&self, config: &ProvisionConfig, device: &DeviceState, mac: &str) -> HashMap<String, Vec<String>>;
}

/// Helper to add a single line to a specific header
pub fn add_line(
    sections: &mut HashMap<String, Vec<String>>,
    header: &str,
    line: String,
) {
    sections.entry(header.to_string()).or_default().push(line);
}
