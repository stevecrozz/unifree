//! Device state management

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub use unifree_common::state::DeviceState;
use unifree_common::types::MacAddress;

pub use unifree_common::state::DeviceStatus;

/// Application state
pub struct AppState {
    /// State directory path
    state_dir: PathBuf,
    /// Known devices
    pub devices: HashMap<MacAddress, DeviceState>,
}

impl AppState {
    /// Create new app state, loading from disk if exists
    pub fn new(state_dir: &str) -> anyhow::Result<Self> {
        let state_dir = PathBuf::from(state_dir);
        
        // Create state directory if it doesn't exist
        fs::create_dir_all(&state_dir)?;
        
        let devices_file = state_dir.join("devices.json");
        let devices = if devices_file.exists() {
            let data = fs::read_to_string(&devices_file)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { state_dir, devices })
    }

            /// Get or create a device entry                                                               
            pub fn get_or_create_device(&mut self, mac: MacAddress) -> &mut DeviceState {                  
                self.devices.entry(mac).or_insert_with(|| DeviceState {                                    
                    mac,                                                                                   
                    ..Default::default()                                                                   
                })                                                                                         
            }                                                                                              
                                                                                                           
            /// Save state to disk                                                                         
            pub fn save(&self) -> anyhow::Result<()> {            let devices_file = self.state_dir.join("devices.json");                                    
            let data = serde_json::to_string_pretty(&self.devices)?;                                   
            fs::write(devices_file, data)?;                                                            
            Ok(())                                                                                     
        }
    }