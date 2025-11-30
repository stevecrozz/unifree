use super::{IniSection, add_line};
use crate::config::models::ProvisionConfig;
use crate::state::DeviceState;
use crate::config::system::types::{ToUnifiConfig, UnifiBool};
use std::collections::HashMap;

pub struct ServicesSection;

struct DhcpcConfig {
    idx: u8,
    devname: String,
    status: UnifiBool,
}

impl ToUnifiConfig for DhcpcConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("dhcpc.{}", self.idx);
        vec![
            (format!("{}.devname", p), self.devname.clone()),
            (format!("{}.status", p), self.status.to_string()),
        ]
    }
}

struct NtpClientConfig {
    idx: u8,
    server: String,
    status: UnifiBool,
}

impl ToUnifiConfig for NtpClientConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("ntpclient.{}", self.idx);
        vec![
            (format!("{}.server", p), self.server.clone()),
            (format!("{}.status", p), self.status.to_string()),
        ]
    }
}

struct SshdKeyConfig {
    idx: u8,
    status: UnifiBool,
    value: String,
    key_type: String,
    comment: Option<String>,
}

impl ToUnifiConfig for SshdKeyConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("sshd.auth.key.{}", self.idx);
        let mut kvs = vec![
            (format!("{}.status", p), self.status.to_string()),
            (format!("{}.value", p), self.value.clone()),
            (format!("{}.type", p), self.key_type.clone()),
        ];
        if let Some(c) = &self.comment {
            kvs.push((format!("{}.comment", p), c.clone()));
        }
        kvs
    }
}

struct SshdInterfaceConfig {
    idx: u8,
    status: UnifiBool,
    ifname: String,
}

impl ToUnifiConfig for SshdInterfaceConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("sshd.{}", self.idx);
        vec![
            (format!("{}.status", p), self.status.to_string()),
            (format!("{}.ifname", p), self.ifname.clone()),
        ]
    }
}

struct SyslogRemoteConfig {
    status: UnifiBool,
    ip: String,
    port: u16,
    encrypt: UnifiBool,
    key: Option<String>,
}

impl ToUnifiConfig for SyslogRemoteConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let mut kvs = vec![
            ("syslog.remote.status".to_string(), self.status.to_string()),
            ("syslog.remote.ip".to_string(), self.ip.clone()),
            ("syslog.remote.port".to_string(), self.port.to_string()),
            ("syslog.remote.encrypt".to_string(), self.encrypt.to_string()),
        ];
        if let Some(k) = &self.key {
            kvs.push(("syslog.remote.key".to_string(), k.clone()));
        }
        kvs
    }
}

struct NetconsoleConfig {
    status: UnifiBool,
    host: String,
    port: u16,
}

impl ToUnifiConfig for NetconsoleConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        vec![
            ("netconsole.status".to_string(), self.status.to_string()),
            ("netconsole.host".to_string(), self.host.clone()),
            ("netconsole.port".to_string(), self.port.to_string()),
        ]
    }
}

struct ResolvHostConfig {
    idx: u8,
    name: String,
}

impl ToUnifiConfig for ResolvHostConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        vec![
            (format!("resolv.host.{}.name", self.idx), self.name.clone()),
        ]
    }
}

impl IniSection for ServicesSection {
    fn generate(&self, config: &ProvisionConfig, _device_state: &DeviceState, mac: &str) -> HashMap<String, Vec<String>> {
        let mut sections = HashMap::new();
        
        // --- connectivity ---
        add_line(&mut sections, " connectivity", "connectivity.status=disabled".to_string());
        
        // --- dhcpc ---
        add_line(&mut sections, " dhcpc", "dhcpc.status=enabled".to_string());
        let dhcpc = DhcpcConfig { idx: 1, devname: "br0".to_string(), status: UnifiBool::Enabled };
        for (k, v) in dhcpc.to_config() { add_line(&mut sections, " dhcpc", format!("{}={}", k, v)); }
        
        // --- dnsmasq ---
        add_line(&mut sections, " dnsmasq", "dnsmasq.status=disabled".to_string());
        
        // --- DPI Fingerprint ---
        add_line(&mut sections, " DPI Fingerprint", "".to_string());
        
        // --- ipset ---
        add_line(&mut sections, " ipset", "ipset.status=disabled".to_string());
        
        // --- iptables ---
        add_line(&mut sections, " iptables", "iptables.status=disabled".to_string());
        add_line(&mut sections, " iptables", "ip6tables.status=disabled".to_string());
        
        // --- mac acl ---
        add_line(&mut sections, " mac acl", "macacl.status=disabled".to_string());
        
        // --- mesh ---
        add_line(&mut sections, " mesh", "mesh.status=disabled".to_string());
        
        // --- misc ---
        add_line(&mut sections, " misc", "".to_string());
        
        // --- ntpclient ---
        add_line(&mut sections, " ntpclient", "ntpclient.status=enabled".to_string());
        if !config.ntp_servers.is_empty() {
            for (i, server) in config.ntp_servers.iter().enumerate() {
                let ntp = NtpClientConfig { idx: (i + 1) as u8, server: server.clone(), status: UnifiBool::Enabled };
                for (k, v) in ntp.to_config() { add_line(&mut sections, " ntpclient", format!("{}={}", k, v)); }
            }
        } else {
            for i in 1..=4 {
                let ntp = NtpClientConfig { idx: i as u8, server: format!("{}.ubnt.pool.ntp.org", i-1), status: UnifiBool::Enabled };
                for (k, v) in ntp.to_config() { add_line(&mut sections, " ntpclient", format!("{}={}", k, v)); }
            }
        }
        
        // --- qos ---
        add_line(&mut sections, " qos", "qos.status=disabled".to_string());
        
        // --- redirector ---
        add_line(&mut sections, " redirector", "redirector.status=disabled".to_string());
        
        // --- resolv ---
        add_line(&mut sections, " resolv", "resolv.status=enabled".to_string());
        let normalized_mac = mac.replace(":", "").to_lowercase();
        if let Some(device) = config.devices.get(&normalized_mac) {
             if let Some(name) = &device.name {
                 let rhost = ResolvHostConfig { idx: 1, name: name.clone() };
                 for (k, v) in rhost.to_config() { add_line(&mut sections, " resolv", format!("{}={}", k, v)); }
             }
        }
        add_line(&mut sections, " resolv", "resolv.nameserver.1.status=disabled".to_string());
        add_line(&mut sections, " resolv", "resolv.nameserver.2.status=disabled".to_string());
        
        // --- route ---
        add_line(&mut sections, " route", "route.status=enabled".to_string());
        
        // --- sshd ---
        add_line(&mut sections, " sshd", "sshd.status=enabled".to_string());
        add_line(&mut sections, " sshd", "sshd.auth.passwd=enabled".to_string());
        
        let mut all_keys = config.ssh_keys.clone();
        if let Some(k) = &config.management.ssh_key {
            if let Some(parsed) = crate::config::models::parse_ssh_pubkey(k) {
                all_keys.push(parsed);
            }
        }
        
        for (i, key) in all_keys.iter().enumerate() {
            let kcfg = SshdKeyConfig {
                idx: (i + 1) as u8,
                status: UnifiBool::Enabled,
                value: key.value.clone(),
                key_type: key.key_type.clone(),
                comment: key.comment.clone(),
            };
            for (k, v) in kcfg.to_config() { add_line(&mut sections, " sshd", format!("{}={}", k, v)); }
        }
        let sshd_if = SshdInterfaceConfig { idx: 1, status: UnifiBool::Enabled, ifname: "br0".to_string() };
        for (k, v) in sshd_if.to_config() { add_line(&mut sections, " sshd", format!("{}={}", k, v)); }
        
        // --- stamgr ---
        add_line(&mut sections, " stamgr", "stamgr.status=disabled".to_string());
        
        // --- switch ---
        add_line(&mut sections, " switch", "switch.status=disabled".to_string());
        
        // --- syslog ---
        add_line(&mut sections, " syslog", "syslog.status=enabled".to_string());
        add_line(&mut sections, " syslog", "syslog.level=7".to_string());
        
        if let Some(device) = config.devices_info.get(&normalized_mac) {
             if let Some(ip) = &device.inform_ip {
                let syslog = SyslogRemoteConfig {
                    status: UnifiBool::Enabled,
                    ip: ip.clone(),
                    port: 5514,
                    encrypt: UnifiBool::Enabled,
                    key: device.syslog_remote_key.clone(),
                };
                for (k, v) in syslog.to_config() { add_line(&mut sections, " syslog", format!("{}={}", k, v)); }
                
                let netconsole = NetconsoleConfig {
                    status: UnifiBool::Enabled,
                    host: ip.clone(),
                    port: 5514,
                };
                for (k, v) in netconsole.to_config() { add_line(&mut sections, " syslog", format!("{}={}", k, v)); }
             }
        }
        
        sections
    }
}
