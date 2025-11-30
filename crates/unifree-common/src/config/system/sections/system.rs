use super::{IniSection, add_line};
use crate::config::models::ProvisionConfig;
use crate::state::DeviceState;
use crate::config::system::types::{ToUnifiConfig, UnifiBool};
use std::collections::HashMap;

pub struct SystemSection;

struct UnifiConfig {
    version: String,
    anonymous_controller_id: String,
    anonymous_site_id: String,
    reporterid: String,
    siteid: String,
    idp: UnifiBool,
    mcip: String,
    key: String,
    feature_always_send_crash_logs: UnifiBool,
    cfgcap_info: String,
}

impl ToUnifiConfig for UnifiConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        vec![
            ("unifi.version".to_string(), self.version.clone()),
            ("unifi.anonymous_controller_id".to_string(), self.anonymous_controller_id.clone()),
            ("unifi.anonymous_site_id".to_string(), self.anonymous_site_id.clone()),
            ("unifi.reporterid".to_string(), self.reporterid.clone()),
            ("unifi.siteid".to_string(), self.siteid.clone()),
            ("unifi.idp".to_string(), self.idp.to_string()),
            ("unifi.mcip".to_string(), self.mcip.clone()),
            ("unifi.key".to_string(), self.key.clone()),
            ("unifi.feature.always_send_crash_logs".to_string(), self.feature_always_send_crash_logs.to_string()),
            ("unifi.cfgcap_info".to_string(), self.cfgcap_info.clone()),
        ]
    }
}

impl IniSection for SystemSection {
    fn generate(&self, config: &ProvisionConfig, _device: &DeviceState, mac: &str) -> HashMap<String, Vec<String>> {
        let mut sections = HashMap::new();
        let h = " unifi";

        let unifi_key = config.devices_info.get(&mac.replace(":", "").to_lowercase())
            .and_then(|d| d.auth_key.clone())
            .unwrap_or("cb1aca606688a98bc453add57acb3589".to_string()); 

        let cfg = UnifiConfig {
            version: "9.5.21".to_string(),
            anonymous_controller_id: "d5dcfdb7-f8e2-49e5-92b2-e7723cba1885".to_string(),
            anonymous_site_id: "ef8b8b11-ba0f-4e6f-b297-287e2ac93630".to_string(),
            reporterid: "d5dcfdb7-f8e2-49e5-92b2-e7723cba1885".to_string(),
            siteid: "692911c5ca49482e1a75e911".to_string(),
            idp: UnifiBool::Enabled,
            mcip: "239.254.127.63".to_string(),
            key: unifi_key,
            feature_always_send_crash_logs: UnifiBool::Enabled,
            cfgcap_info: "0x7".to_string(),
        };

        for (k, v) in cfg.to_config() {
            add_line(&mut sections, h, format!("{}={}", k, v));
        }

        sections
    }
}

pub struct CoreSystemSection;

struct SystemConfig {
    analytics_status: UnifiBool,
    timezone: String,
}

impl ToUnifiConfig for SystemConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        vec![
            ("system.analytics.status".to_string(), self.analytics_status.to_string()),
            ("system.timezone".to_string(), self.timezone.clone()),
            ("locale.timezone".to_string(), self.timezone.clone()),
        ]
    }
}

impl IniSection for CoreSystemSection {
    fn generate(&self, config: &ProvisionConfig, _device: &DeviceState, _mac: &str) -> HashMap<String, Vec<String>> {
        let mut sections = HashMap::new();
        let h = " system";
        
        let timezone_str = config.get_effective_timezone_posix();

        let cfg = SystemConfig {
            analytics_status: UnifiBool::Disabled,
            timezone: timezone_str,
        };
        
        for (k, v) in cfg.to_config() {
            add_line(&mut sections, h, format!("{}={}", k, v));
        }
        
        sections
    }
}

pub struct UsersSection;

struct UserConfig {
    idx: u8,
    name: String,
    password: String,
    status: UnifiBool,
    shell: Option<String>,
}

impl ToUnifiConfig for UserConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("users.{}", self.idx);
        let mut kvs = vec![
            (format!("{}.name", p), self.name.clone()),
            (format!("{}.password", p), self.password.clone()),
            (format!("{}.status", p), self.status.to_string()),
        ];
        if let Some(shell) = &self.shell {
            kvs.push((format!("{}.shell", p), shell.clone()));
        }
        kvs
    }
}

impl IniSection for UsersSection {
    fn generate(&self, config: &ProvisionConfig, _device: &DeviceState, _mac: &str) -> HashMap<String, Vec<String>> {
        let mut sections = HashMap::new();
        let h = " users";
        
        use pwhash::sha512_crypt;
        
        add_line(&mut sections, h, "users.status=enabled".to_string());
        
        let (username, password) = if let Some(u_conf) = &config.management.username {
            let u = u_conf.clone();
            let p_raw = config.management.password.as_deref().unwrap_or("ubnt");
            let p = if p_raw.starts_with("$") {
                p_raw.to_string()
            } else {
                sha512_crypt::hash(p_raw).expect("Failed to hash password")
            };
            (u, p)
        } else {
            ("ubnt".to_string(), "ubnt".to_string())
        };

        let user1 = UserConfig {
            idx: 1,
            name: username,
            password,
            status: UnifiBool::Enabled,
            shell: None,
        };

        let user2 = UserConfig {
            idx: 2,
            name: "nobody".to_string(),
            password: "x".to_string(),
            status: UnifiBool::Enabled,
            shell: Some("/bin/false".to_string()),
        };

        for (k, v) in user1.to_config() {
            add_line(&mut sections, h, format!("{}={}", k, v));
        }
        for (k, v) in user2.to_config() {
            add_line(&mut sections, h, format!("{}={}", k, v));
        }
        
        sections
    }
}