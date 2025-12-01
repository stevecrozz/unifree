use crate::config::models::ProvisionConfig;
use std::collections::HashMap;
use unifree_state::DeviceState;

pub mod network;
pub mod radio;
pub mod services;
pub mod system;
pub mod wireless;

/// Trait for generating INI configuration sections
pub trait IniSection {
    /// Generate the INI lines for this section, returning a map of Header -> Lines
    fn generate(
        &self,
        config: &ProvisionConfig,
        device: &DeviceState,
        mac: &str,
    ) -> HashMap<String, Vec<String>>;
}

/// Helper to add a single line to a specific header
pub fn add_line(sections: &mut HashMap<String, Vec<String>>, header: &str, line: String) {
    sections.entry(header.to_string()).or_default().push(line);
}
