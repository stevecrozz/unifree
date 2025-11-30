use crate::config::models::ProvisionConfig;
use crate::state::DeviceState;
use std::collections::HashMap;

use super::sections::IniSection;
use super::sections::system::{SystemSection, CoreSystemSection, UsersSection};
use super::sections::radio::RadioSection;
use super::sections::wireless::WirelessSection;
use super::sections::network::NetworkSection;
use super::sections::services::ServicesSection;

impl ProvisionConfig {
    /// Generate system_ini for a device (modern format for U6/U7)
    pub fn generate_system_ini(&self, mac: &str, device_state: &DeviceState) -> String {
        let mut sections: HashMap<String, Vec<String>> = HashMap::new();
        
        let generators: Vec<Box<dyn IniSection>> = vec![
            Box::new(SystemSection),
            Box::new(CoreSystemSection),
            Box::new(UsersSection),
            Box::new(RadioSection),
            Box::new(WirelessSection),
            Box::new(NetworkSection),
            Box::new(ServicesSection),
        ];
        
        for generator in generators {
            let new_sections = generator.generate(self, device_state, mac);
            for (header, lines) in new_sections {
                sections.entry(header).or_default().extend(lines);
            }
        }

        // --- SORT AND JOIN ---
        let mut headers: Vec<&String> = sections.keys().collect();
        headers.sort();
        
        let mut final_lines = Vec::new();
        for header in headers {
            if !header.trim().is_empty() {
                final_lines.push(format!("#{}", header));
            }
            if let Some(lines) = sections.get(header) {
                final_lines.extend(lines.clone());
            }
        }
        
        final_lines.join("\n")
    }
}