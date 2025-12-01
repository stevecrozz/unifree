use super::{add_line, IniSection};
use crate::config::models::ProvisionConfig;
use crate::config::system::sections::radio::get_phys_radios;
use crate::config::system::types::{ToUnifiConfig, UnifiBool};
use md5;
use std::collections::HashMap;
use unifree_state::DeviceState;
use unifree_types::{Band, SecurityMode};

pub struct WirelessSection;

// Domain Model for Wireless Config
struct WirelessConfig {
    idx: u8,
    devname: String,
    status: UnifiBool,
    ssid: String,
    mode: String,
    security: String,
    hide_ssid: bool,
    parent: String,
    usage: String,
    l2_isolation: UnifiBool,
    mac_acl_status: UnifiBool,
    mac_acl_policy: String,
    wmm: UnifiBool,
    uapsd: UnifiBool,
    authmode: u8,
    autowds: UnifiBool,
    beacon_rate: Option<u32>,
    dtim_period: u8,
    element_adopt: UnifiBool,
    is_guest: bool,
    mcast_enhance: u8,
    mcastrate: String,
    mgmt_rate: Option<u32>,
    minrate_cck_rates_status: Option<bool>,
    minrate_data: Option<u32>,
    no2ghz_oui: UnifiBool,
    puren: u8,
    pureg: u8,
    schedule_enabled: UnifiBool,
    vport: UnifiBool,
    vwire: UnifiBool,
    wds: UnifiBool,
    addmtikie: UnifiBool,
    iot: Option<UnifiBool>,
    qbssload: Option<UnifiBool>,
    unique_id: String,
}

impl ToUnifiConfig for WirelessConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("wireless.{}", self.idx);
        let mut kvs = vec![
            (format!("{}.devname", p), self.devname.clone()),
            (format!("{}.status", p), self.status.to_string()),
            (format!("{}.ssid", p), self.ssid.clone()),
            (format!("{}.mode", p), self.mode.clone()),
            (format!("{}.security", p), self.security.clone()),
            (format!("{}.hide_ssid", p), self.hide_ssid.to_string()),
            (format!("{}.parent", p), self.parent.clone()),
            (format!("{}.usage", p), self.usage.clone()),
            (format!("{}.l2_isolation", p), self.l2_isolation.to_string()),
            (
                format!("{}.mac_acl.status", p),
                self.mac_acl_status.to_string(),
            ),
            (format!("{}.mac_acl.policy", p), self.mac_acl_policy.clone()),
            (format!("{}.wmm", p), self.wmm.to_string()),
            (format!("{}.uapsd", p), self.uapsd.to_string()),
            (format!("{}.authmode", p), self.authmode.to_string()),
            (format!("{}.autowds", p), self.autowds.to_string()),
            (format!("{}.dtim_period", p), self.dtim_period.to_string()),
            (
                format!("{}.element_adopt", p),
                self.element_adopt.to_string(),
            ),
            (format!("{}.is_guest", p), self.is_guest.to_string()),
            (
                format!("{}.mcast.enhance", p),
                self.mcast_enhance.to_string(),
            ),
            (format!("{}.mcastrate", p), self.mcastrate.clone()),
            (format!("{}.no2ghz_oui", p), self.no2ghz_oui.to_string()),
            (format!("{}.puren", p), self.puren.to_string()),
            (format!("{}.pureg", p), self.pureg.to_string()),
            (
                format!("{}.schedule_enabled", p),
                self.schedule_enabled.to_string(),
            ),
            (format!("{}.vport", p), self.vport.to_string()),
            (format!("{}.vwire", p), self.vwire.to_string()),
            (format!("{}.wds", p), self.wds.to_string()),
            (format!("{}.addmtikie", p), self.addmtikie.to_string()),
            (format!("{}.id", p), self.unique_id.clone()),
        ];

        if let Some(br) = self.beacon_rate {
            kvs.push((format!("{}.beacon_rate", p), br.to_string()));
        }
        if let Some(mr) = self.mgmt_rate {
            kvs.push((format!("{}.mgmt_rate", p), mr.to_string()));
        }
        if let Some(mrc) = self.minrate_cck_rates_status {
            kvs.push((format!("{}.minrate_cck_rates.status", p), mrc.to_string()));
        }
        if let Some(mrd) = self.minrate_data {
            kvs.push((format!("{}.minrate_data", p), mrd.to_string()));
        }
        if let Some(iot) = self.iot {
            kvs.push((format!("{}.iot", p), iot.to_string()));
        }
        if let Some(qbss) = self.qbssload {
            kvs.push((format!("{}.qbssload", p), qbss.to_string()));
        }

        kvs
    }
}

// Domain Model for AAA Config
struct AaaConfig {
    idx: u8,
    devname: String,
    status: UnifiBool,
    ssid: String,
    br_devname: String,
    driver: String,
    verbose: u8,
    k11_status: UnifiBool,
    bss_transition: UnifiBool,
    country_beacon: UnifiBool,
    eapol_version: u8,
    ft_status: UnifiBool,
    hide_ssid: bool,
    is_guest: bool,
    p2p: UnifiBool,
    p2p_cross_connect: UnifiBool,
    proxy_arp: UnifiBool,
    radius_macacl_status: UnifiBool,
    tdls_prohibit: UnifiBool,
    wpa_group_rekey: u32,
    unique_id: String,

    // Security fields
    wpa: u8,
    wpa_key_mgmt: Option<String>,
    wpa_psk: Option<String>,
    wpa_pairwise: Option<String>,
    wpa3_support: Option<UnifiBool>,
    wpa3_transition: Option<UnifiBool>,
    wpa3_ft_status: Option<UnifiBool>,
    pmf_status: UnifiBool,
    pmf_mode: u8,
    pmf_cipher: String,

    sae_psk_mac: Option<String>,
    sae_psk_psk: Option<String>,
    sae_anti_clogging: Option<u8>,
    sae_sync: Option<u8>,
}

impl ToUnifiConfig for AaaConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("aaa.{}", self.idx);
        let mut kvs = vec![
            (format!("{}.devname", p), self.devname.clone()),
            (format!("{}.status", p), self.status.to_string()),
            (format!("{}.ssid", p), self.ssid.clone()),
            (format!("{}.br.devname", p), self.br_devname.clone()),
            (format!("{}.driver", p), self.driver.clone()),
            (format!("{}.verbose", p), self.verbose.to_string()),
            (format!("{}.11k.status", p), self.k11_status.to_string()),
            (
                format!("{}.bss_transition", p),
                self.bss_transition.to_string(),
            ),
            (
                format!("{}.country_beacon", p),
                self.country_beacon.to_string(),
            ),
            (
                format!("{}.eapol_version", p),
                self.eapol_version.to_string(),
            ),
            (format!("{}.ft.status", p), self.ft_status.to_string()),
            (format!("{}.hide_ssid", p), self.hide_ssid.to_string()),
            (format!("{}.is_guest", p), self.is_guest.to_string()),
            (format!("{}.p2p", p), self.p2p.to_string()),
            (
                format!("{}.p2p_cross_connect", p),
                self.p2p_cross_connect.to_string(),
            ),
            (format!("{}.proxy_arp", p), self.proxy_arp.to_string()),
            (
                format!("{}.radius.macacl.status", p),
                self.radius_macacl_status.to_string(),
            ),
            (
                format!("{}.tdls_prohibit", p),
                self.tdls_prohibit.to_string(),
            ),
            (
                format!("{}.wpa.group_rekey", p),
                self.wpa_group_rekey.to_string(),
            ),
            (format!("{}.id", p), self.unique_id.clone()),
            (format!("{}.wpa", p), self.wpa.to_string()),
        ];

        if let Some(mgmt) = &self.wpa_key_mgmt {
            kvs.push((format!("{}.wpa.key.1.mgmt", p), mgmt.clone()));
        }
        if let Some(psk) = &self.wpa_psk {
            kvs.push((format!("{}.wpa.psk", p), psk.clone()));
        }
        if let Some(pairwise) = &self.wpa_pairwise {
            kvs.push((format!("{}.wpa.1.pairwise", p), pairwise.clone()));
        }

        kvs.push((format!("{}.pmf.cipher", p), self.pmf_cipher.clone()));
        kvs.push((format!("{}.pmf.mode", p), self.pmf_mode.to_string()));
        kvs.push((format!("{}.pmf.status", p), self.pmf_status.to_string()));

        if let Some(sup) = self.wpa3_support {
            kvs.push((format!("{}.wpa3.support", p), sup.to_string()));
        }
        if let Some(trans) = self.wpa3_transition {
            kvs.push((format!("{}.wpa3.transition", p), trans.to_string()));
        }
        if let Some(ft) = self.wpa3_ft_status {
            kvs.push((format!("{}.wpa3.ft.status", p), ft.to_string()));
        }
        if let Some(mac) = &self.sae_psk_mac {
            kvs.push((format!("{}.sae.psk.1.mac", p), mac.clone()));
        }
        if let Some(psk) = &self.sae_psk_psk {
            kvs.push((format!("{}.sae.psk.1.psk", p), psk.clone()));
        }
        if let Some(val) = self.sae_anti_clogging {
            kvs.push((format!("{}.sae.anti_clogging", p), val.to_string()));
        }
        if let Some(val) = self.sae_sync {
            kvs.push((format!("{}.sae.sync", p), val.to_string()));
        }

        kvs
    }
}

impl IniSection for WirelessSection {
    fn generate(
        &self,
        config: &ProvisionConfig,
        device_state: &DeviceState,
        mac: &str,
    ) -> HashMap<String, Vec<String>> {
        let mut sections = HashMap::new();
        let h = " wlans (radio)";

        add_line(&mut sections, h, "aaa.status=enabled".to_string());
        add_line(&mut sections, h, "wireless.status=enabled".to_string());

        let radios = get_phys_radios(config, device_state, mac);
        let networks = config.get_networks_for_device(mac);
        let mut sorted_networks: Vec<_> = networks.iter().collect();
        sorted_networks.sort_by(|(_k1, n1), (_k2, n2)| {
            n1.vlan.cmp(&n2.vlan).then_with(|| n1.ssid.cmp(&n2.ssid))
        });

        let normalized_mac = mac.replace(":", "").to_lowercase();

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
                let w_idx = dev_idx as u8 + 1; // Cast here

                let id_input = format!("{}-{}-{}", normalized_mac, w_idx, net_config.ssid);
                let unique_id = format!("{:x}", md5::compute(id_input));

                // Build Wireless Config
                let wireless_cfg = WirelessConfig {
                    idx: w_idx,
                    devname: devname.clone(),
                    status: UnifiBool::Enabled,
                    ssid: net_config.ssid.clone(),
                    mode: "master".to_string(),
                    security: "none".to_string(),
                    hide_ssid: net_config.hidden,
                    parent: radio.phyname.clone(),
                    usage: "user".to_string(),
                    l2_isolation: if net_config.client_device_isolation {
                        UnifiBool::Enabled
                    } else {
                        UnifiBool::Disabled
                    },
                    mac_acl_status: UnifiBool::Enabled,
                    mac_acl_policy: "deny".to_string(),
                    wmm: UnifiBool::Enabled,
                    uapsd: UnifiBool::Disabled,
                    authmode: 1,
                    autowds: UnifiBool::Disabled,
                    beacon_rate: if radio.band == Band::Band2g {
                        Some(1000)
                    } else {
                        None
                    },
                    dtim_period: if radio.band == Band::Band2g { 1 } else { 3 },
                    element_adopt: UnifiBool::Disabled,
                    is_guest: false,
                    mcast_enhance: 0,
                    mcastrate: "auto".to_string(),
                    mgmt_rate: if radio.band == Band::Band2g {
                        Some(1000)
                    } else {
                        None
                    },
                    minrate_cck_rates_status: if radio.band == Band::Band2g {
                        Some(true)
                    } else {
                        None
                    },
                    minrate_data: if radio.band == Band::Band2g {
                        Some(1000)
                    } else {
                        None
                    },
                    no2ghz_oui: if radio.band == Band::Band2g && !net_config.iot {
                        UnifiBool::Enabled
                    } else {
                        UnifiBool::Disabled
                    },
                    puren: 0,
                    pureg: if radio.band == Band::Band2g { 0 } else { 1 },
                    schedule_enabled: UnifiBool::Disabled,
                    vport: UnifiBool::Disabled,
                    vwire: UnifiBool::Disabled,
                    wds: UnifiBool::Disabled,
                    addmtikie: UnifiBool::Disabled,
                    iot: if net_config.iot {
                        Some(UnifiBool::Enabled)
                    } else {
                        None
                    },
                    qbssload: if net_config.iot {
                        Some(UnifiBool::Disabled)
                    } else {
                        None
                    },
                    unique_id: unique_id.clone(),
                };

                for (k, v) in wireless_cfg.to_config() {
                    add_line(&mut sections, h, format!("{}={}", k, v));
                }

                // Build AAA Config
                let mut aaa_cfg = AaaConfig {
                    idx: w_idx,
                    devname: devname.clone(),
                    status: UnifiBool::Enabled,
                    ssid: net_config.ssid.clone(),
                    br_devname: if net_config.vlan.is_some() {
                        format!("br0.{}", net_config.vlan.unwrap())
                    } else {
                        "br0".to_string()
                    },
                    driver: "madwifi".to_string(),
                    verbose: 2,
                    k11_status: UnifiBool::Disabled,
                    bss_transition: if net_config.iot {
                        UnifiBool::Disabled
                    } else {
                        UnifiBool::Enabled
                    },
                    country_beacon: UnifiBool::Disabled,
                    eapol_version: 2,
                    ft_status: UnifiBool::Disabled,
                    hide_ssid: net_config.hidden,
                    is_guest: false,
                    p2p: UnifiBool::Disabled,
                    p2p_cross_connect: UnifiBool::Disabled,
                    proxy_arp: UnifiBool::Disabled,
                    radius_macacl_status: UnifiBool::Disabled,
                    tdls_prohibit: UnifiBool::Disabled,
                    wpa_group_rekey: 0,
                    unique_id: unique_id.clone(),
                    wpa: 0,
                    wpa_key_mgmt: None,
                    wpa_psk: None,
                    wpa_pairwise: None,
                    wpa3_support: None,
                    wpa3_transition: None,
                    wpa3_ft_status: None,
                    pmf_status: UnifiBool::Disabled,
                    pmf_mode: 0,
                    pmf_cipher: "AES-128-CMAC".to_string(),
                    sae_psk_mac: None,
                    sae_psk_psk: None,
                    sae_anti_clogging: None,
                    sae_sync: None,
                };

                match net_config.security {
                    SecurityMode::Open => {
                        aaa_cfg.wpa = 0;
                    }
                    SecurityMode::Wpa2 => {
                        aaa_cfg.wpa = 2;
                        aaa_cfg.wpa_key_mgmt = Some("WPA-PSK".to_string());
                        aaa_cfg.wpa_psk = net_config.passphrase.clone();
                        aaa_cfg.wpa_pairwise = Some("CCMP".to_string());
                        aaa_cfg.pmf_cipher = "AES-128-CMAC".to_string();
                        aaa_cfg.pmf_mode = 0;
                        aaa_cfg.pmf_status = UnifiBool::Disabled;
                    }
                    SecurityMode::Wpa3 => {
                        aaa_cfg.wpa = 2;
                        aaa_cfg.wpa_key_mgmt = Some("SAE".to_string());
                        aaa_cfg.wpa_psk = net_config.passphrase.clone();
                        aaa_cfg.wpa_pairwise = Some("CCMP".to_string());
                        aaa_cfg.wpa3_support = Some(UnifiBool::Enabled);
                        aaa_cfg.wpa3_transition = Some(UnifiBool::Disabled);
                        aaa_cfg.wpa3_ft_status = Some(UnifiBool::Disabled);
                        aaa_cfg.pmf_status = UnifiBool::Enabled;
                        aaa_cfg.pmf_mode = 2;
                        aaa_cfg.pmf_cipher = "AES-128-CMAC".to_string();

                        if radio.band == Band::Band6g {
                            aaa_cfg.sae_psk_mac = Some("ff:ff:ff:ff:ff:ff".to_string());
                            aaa_cfg.sae_psk_psk = net_config.passphrase.clone();
                        }
                    }
                    SecurityMode::Wpa2Wpa3 => {
                        aaa_cfg.wpa = 2;
                        aaa_cfg.wpa_key_mgmt = Some("SAE".to_string());
                        aaa_cfg.wpa_psk = net_config.passphrase.clone();
                        aaa_cfg.wpa_pairwise = Some("CCMP".to_string());
                        aaa_cfg.wpa3_support = Some(UnifiBool::Enabled);

                        aaa_cfg.wpa3_transition = if radio.band == Band::Band6g {
                            Some(UnifiBool::Disabled)
                        } else {
                            Some(UnifiBool::Enabled)
                        };

                        aaa_cfg.wpa3_ft_status = Some(UnifiBool::Disabled);
                        aaa_cfg.pmf_status = UnifiBool::Enabled;
                        aaa_cfg.pmf_mode = if radio.band == Band::Band6g { 2 } else { 1 };
                        aaa_cfg.pmf_cipher = "AES-128-CMAC".to_string();

                        if radio.band == Band::Band6g {
                            aaa_cfg.sae_psk_mac = Some("ff:ff:ff:ff:ff:ff".to_string());
                            aaa_cfg.sae_psk_psk = net_config.passphrase.clone();
                        }

                        if net_config.client_device_isolation {
                            aaa_cfg.sae_anti_clogging = Some(5);
                            aaa_cfg.sae_sync = Some(5);
                        }
                    }
                }

                for (k, v) in aaa_cfg.to_config() {
                    add_line(&mut sections, h, format!("{}={}", k, v));
                }

                vap_idx_on_radio += 1;
            }
        }

        sections
    }
}
