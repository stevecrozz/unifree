use unifree_common::config::models::{ProvisionConfig, DeviceInfo};
use unifree_common::state::DeviceState;
use std::collections::{HashSet, HashMap};
use std::fs;
use std::path::Path;

#[test]
fn test_generate_system_ini_matches_reference() {
    // 1. Load config from fixtures
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures_dir = manifest_dir.join("tests/fixtures");
    
    let config_path = fixtures_dir.join("config.json");
    
    let mut config = ProvisionConfig::from_file(config_path.to_str().unwrap())
        .expect("Failed to load config.json from fixtures");

    // 2. Load Device Info (Fixture is a single device inform payload)
    let device_path = fixtures_dir.join("device.json");
    let device_content = fs::read_to_string(device_path).unwrap();
    let device_info: DeviceInfo = serde_json::from_str(&device_content).expect("Failed to parse device.json fixture");
    
    let mut devices_map = HashMap::new();
    // Normalize MAC: remove colons and lowercase
    // The fixture mac is "1c:0b:8b:8e:17:7f"
    // We assume the caller or config uses normalized keys
    devices_map.insert("1c0b8b8e177f".to_string(), device_info);
    config.devices_info = devices_map;

    // Create a default DeviceState for the test
    let dummy_device_state = DeviceState::default();

    // 3. Generate Config
    // Use the MAC address from config.json's device section (or hardcoded if that matches the fixture)
    let generated = config.generate_system_ini("1c:0b:8b:8e:17:7f", &dummy_device_state);
    
    // 4. Load Reference
    let reference_content = fs::read_to_string(fixtures_dir.join("golden_system.ini"))
        .expect("Failed to read golden_system.ini");

    // 5. Filter and Compare
    let normalize = |line: &str| -> String {
        let l = line.trim();
        if l.is_empty() || l.starts_with("#") { return String::new(); }
        
        // Filter out dynamic keys
        if l.contains(".id=") || l.contains(".iapp_key=") 
           || l.contains("unifi.anonymous_controller_id=") 
           || l.contains("unifi.anonymous_site_id=") 
           || l.contains("unifi.reporterid=") 
           || l.contains("unifi.siteid=") 
           || l.contains("unifi.key=") 
           || l.contains("users.1.password=") 
           || l.contains("syslog.") 
           || l.contains("netconsole.") { return String::new(); }

        let s = l.to_string();
        if s.starts_with("netconf.") || s.starts_with("ebtables.") || s.starts_with("vlan.") {
             let parts: Vec<&str> = s.split('.').collect();
             if parts.len() >= 3 && parts[1].chars().all(|c| c.is_digit(10)) {
                 return format!("{}.X.{}", parts[0], parts[2..].join("."));
             }
        }
        if s.starts_with("bridge.") {
             // bridge.2.port.3.devname=... -> bridge.2.port.X.devname=...
             // indices are at parts[1] (bridge index) and parts[3] (port index)
             let parts: Vec<&str> = s.split('.').collect();
             if parts.len() >= 5 && parts[2] == "port" && parts[3].chars().all(|c| c.is_digit(10)) {
                 // Keep bridge index, normalize port index
                 return format!("{}.{}.{}.X.{}", parts[0], parts[1], parts[2], parts[4..].join("."));
             }
        }
        s
    };

    let gen_lines: HashSet<String> = generated.lines()
        .map(|l| normalize(l))
        .filter(|l| !l.is_empty())
        .collect();
        
    let ref_lines: HashSet<String> = reference_content.lines()
        .map(|l| normalize(l))
        .filter(|l| !l.is_empty())
        .collect();

    let missing_in_gen: Vec<&String> = ref_lines.difference(&gen_lines).collect();
    let extra_in_gen: Vec<&String> = gen_lines.difference(&ref_lines).collect();

    if !missing_in_gen.is_empty() || !extra_in_gen.is_empty() {
        println!("Mismatch detected!");
        
        if !missing_in_gen.is_empty() {
            println!("\nMissing in generated config ({} lines):", missing_in_gen.len());
            for l in missing_in_gen {
                println!("  - {}", l);
            }
        }
        
        if !extra_in_gen.is_empty() {
            println!("\nExtra in generated config ({} lines):", extra_in_gen.len());
            for l in extra_in_gen {
                println!("  + {}", l);
            }
        }
        
        panic!("Generated config does not match reference sys_config/run6.txt");
    }
}
