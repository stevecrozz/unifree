use super::{add_line, IniSection};
use crate::config::models::ProvisionConfig;
use crate::config::system::sections::radio::get_phys_radios;
use crate::config::system::types::{ToUnifiConfig, UnifiBool};
use std::collections::HashMap;
use unifree_state::DeviceState;

pub struct NetworkSection;

struct NetconfConfig {
    idx: u8,
    devname: String,
    ip: String,
    status: UnifiBool,
    up: UnifiBool,
    autoip_status: UnifiBool,
    promisc: Option<UnifiBool>,
}

impl ToUnifiConfig for NetconfConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("netconf.{}", self.idx);
        let mut kvs = vec![
            (format!("{}.devname", p), self.devname.clone()),
            (format!("{}.ip", p), self.ip.clone()),
            (format!("{}.status", p), self.status.to_string()),
            (format!("{}.up", p), self.up.to_string()),
            (
                format!("{}.autoip.status", p),
                self.autoip_status.to_string(),
            ),
        ];
        if let Some(promisc) = &self.promisc {
            kvs.push((format!("{}.promisc", p), promisc.to_string()));
        }
        kvs
    }
}

struct BridgePortConfig {
    idx: u8,
    devname: String,
}

struct BridgeConfig {
    idx: u8,
    devname: String,
    fd: u8,
    stp_status: UnifiBool,
    ports: Vec<BridgePortConfig>,
}

impl ToUnifiConfig for BridgeConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("bridge.{}", self.idx);
        let mut kvs = vec![
            (format!("{}.devname", p), self.devname.clone()),
            (format!("{}.fd", p), self.fd.to_string()),
            (format!("{}.stp.status", p), self.stp_status.to_string()),
        ];
        for port in &self.ports {
            kvs.push((
                format!("{}.port.{}.devname", p, port.idx),
                port.devname.clone(),
            ));
        }
        kvs
    }
}

struct VlanConfig {
    idx: u8,
    devname: String,
    id: u16,
}

impl ToUnifiConfig for VlanConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("vlan.{}", self.idx);
        vec![
            (format!("{}.devname", p), self.devname.clone()),
            (format!("{}.id", p), self.id.to_string()),
        ]
    }
}

struct EbtablesConfig {
    idx: u8,
    cmd: String,
}

impl ToUnifiConfig for EbtablesConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("ebtables.{}", self.idx);
        vec![(format!("{}.cmd", p), self.cmd.clone())]
    }
}

impl IniSection for NetworkSection {
    fn generate(
        &self,
        config: &ProvisionConfig,
        device_state: &DeviceState,
        mac: &str,
    ) -> HashMap<String, Vec<String>> {
        let mut sections = HashMap::new();

        let radios = get_phys_radios(config, device_state, mac);
        let networks = config.get_networks_for_device(mac);
        let mut sorted_networks: Vec<_> = networks.iter().collect();
        sorted_networks.sort_by(|(_k1, n1), (_k2, n2)| {
            n1.vlan.cmp(&n2.vlan).then_with(|| n1.ssid.cmp(&n2.ssid))
        });

        // 1. Prepare Bridge & Netconf structures
        let mut br0_ports = vec!["eth0".to_string()];
        // Using tuples for intermediate processing before creating structs
        let mut netconf_raw = vec![
            (
                "br0".to_string(),
                "0.0.0.0".to_string(),
                "enabled".to_string(),
                "enabled".to_string(),
            ),
            (
                "eth0".to_string(),
                "0.0.0.0".to_string(),
                "enabled".to_string(),
                "enabled".to_string(),
            ),
        ];

        let mut vlan_defs: Vec<(u16, String)> = Vec::new(); // (id, devname)
        let mut vlans_seen: std::collections::HashSet<u16> = std::collections::HashSet::new();
        let mut ebtables_rules = Vec::new();

        // Collect VAPs and assign to VLANs/Bridges
        for radio in &radios {
            let mut radio_networks = Vec::new();
            for (name, net_config) in &sorted_networks {
                if net_config.bands.contains(&radio.band) {
                    radio_networks.push((name, net_config));
                }
            }

            let mut vap_idx_on_radio = 0;
            for (_net_name, net_config) in radio_networks {
                let dev_idx = (radio.idx as usize - 1) * 3 + vap_idx_on_radio;
                let devname = format!("ath{}", dev_idx);
                vap_idx_on_radio += 1;

                netconf_raw.push((
                    devname.clone(),
                    "0.0.0.0".to_string(),
                    "enabled".to_string(),
                    "disabled".to_string(),
                ));

                ebtables_rules.push(format!(
                    "-t nat -A PREROUTING --in-interface {} -d BGA -j DROP",
                    devname
                ));
                ebtables_rules.push(format!(
                    "-t nat -A POSTROUTING --out-interface {} -d BGA -j DROP",
                    devname
                ));

                if let Some(vlan_id) = net_config.vlan {
                    vlans_seen.insert(vlan_id);
                    vlan_defs.push((vlan_id, devname.clone()));
                    ebtables_rules.push(format!(
                        "-t broute -A BROUTING -i {} -p 802_1Q -j DROP",
                        devname
                    ));
                } else {
                    br0_ports.push(devname.clone());
                }
            }
        }

        // --- VLANs ---
        if !vlans_seen.is_empty() {
            add_line(&mut sections, " vlan", "vlan.status=enabled".to_string());
            let mut vlan_counter = 1;
            let mut sorted_vids: Vec<u16> = vlans_seen.iter().cloned().collect();
            sorted_vids.sort();

            for vid in &sorted_vids {
                let cfg = VlanConfig {
                    idx: vlan_counter,
                    devname: "eth0".to_string(),
                    id: *vid,
                };
                for (k, v) in cfg.to_config() {
                    add_line(&mut sections, " vlan", format!("{}={}", k, v));
                }

                vlan_defs.push((*vid, "eth0".to_string()));
                vlan_counter += 1;

                netconf_raw.push((
                    format!("eth0.{}", vid),
                    "0.0.0.0".to_string(),
                    "enabled".to_string(),
                    "enabled".to_string(),
                ));
            }
        } else {
            add_line(&mut sections, " vlan", "vlan.status=disabled".to_string());
        }

        // --- Bridge ---
        add_line(
            &mut sections,
            " bridge",
            "bridge.status=enabled".to_string(),
        );

        // br0
        let mut br0_ports_cfg = Vec::new();
        for (i, port) in br0_ports.iter().enumerate() {
            br0_ports_cfg.push(BridgePortConfig {
                idx: (i + 1) as u8,
                devname: port.clone(),
            });
        }
        let br0 = BridgeConfig {
            idx: 1,
            devname: "br0".to_string(),
            fd: 1,
            stp_status: UnifiBool::Disabled,
            ports: br0_ports_cfg,
        };
        for (k, v) in br0.to_config() {
            add_line(&mut sections, " bridge", format!("{}={}", k, v));
        }

        // VLAN Bridges
        if !vlans_seen.is_empty() {
            let mut bridge_idx = 2;
            let mut sorted_vids: Vec<u16> = vlans_seen.iter().cloned().collect();
            sorted_vids.sort();

            for vid in sorted_vids {
                let br_name = format!("br0.{}", vid);
                let mut br_ports = Vec::new();
                let mut port_idx = 1;

                // Find all devnames associated with this VID in vlan_defs
                for (v, dev) in &vlan_defs {
                    if *v == vid && dev != "eth0" {
                        br_ports.push(BridgePortConfig {
                            idx: port_idx,
                            devname: dev.clone(),
                        });
                        port_idx += 1;
                    }
                }
                // Add eth0.VID
                br_ports.push(BridgePortConfig {
                    idx: port_idx,
                    devname: format!("eth0.{}", vid),
                });

                let br = BridgeConfig {
                    idx: bridge_idx,
                    devname: br_name.clone(),
                    fd: 1,
                    stp_status: UnifiBool::Disabled,
                    ports: br_ports,
                };

                for (k, v) in br.to_config() {
                    add_line(&mut sections, " bridge", format!("{}={}", k, v));
                }

                netconf_raw.push((
                    br_name,
                    "0.0.0.0".to_string(),
                    "enabled".to_string(),
                    "enabled".to_string(),
                ));
                ebtables_rules.push(format!(
                    "-t broute -A BROUTING --vlan-id {} -p 802_1Q -j DROP",
                    vid
                ));

                bridge_idx += 1;
            }
        }

        // --- Netconf ---
        add_line(
            &mut sections,
            " netconf",
            "netconf.status=enabled".to_string(),
        );
        for (i, (dev, ip, status, up)) in netconf_raw.iter().enumerate() {
            let idx = (i + 1) as u8;
            let is_wireless = dev.contains("wifi")
                || dev.contains("ath")
                || dev.contains("vwire")
                || dev.contains("scan");
            let final_up = if is_wireless {
                UnifiBool::Disabled
            } else {
                if up == "enabled" {
                    UnifiBool::Enabled
                } else {
                    UnifiBool::Disabled
                }
            };
            let status_bool = if status == "enabled" {
                UnifiBool::Enabled
            } else {
                UnifiBool::Disabled
            };

            let promisc = if dev == "eth0" || dev.starts_with("eth0.") || is_wireless {
                Some(UnifiBool::Enabled)
            } else {
                None
            };

            let cfg = NetconfConfig {
                idx,
                devname: dev.clone(),
                ip: ip.clone(),
                status: status_bool,
                up: final_up,
                autoip_status: UnifiBool::Disabled,
                promisc,
            };

            for (k, v) in cfg.to_config() {
                add_line(&mut sections, " netconf", format!("{}={}", k, v));
            }
        }

        // --- Ebtables ---
        add_line(
            &mut sections,
            " ebtables",
            "ebtables.status=enabled".to_string(),
        );
        add_line(
            &mut sections,
            " ebtables",
            "ebtables.add_vlan.status=disabled".to_string(),
        );
        for (i, rule) in ebtables_rules.iter().enumerate() {
            let cfg = EbtablesConfig {
                idx: (i + 1) as u8,
                cmd: rule.clone(),
            };
            for (k, v) in cfg.to_config() {
                add_line(&mut sections, " ebtables", format!("{}={}", k, v));
            }
        }

        sections
    }
}
