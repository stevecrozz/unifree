use super::{add_line, IniSection};
use crate::config::models::ProvisionConfig;
use crate::config::system::types::{ToUnifiConfig, UnifiBool};
use std::collections::HashMap;
use unifree_state::DeviceState;
use unifree_types::{Band, RadioTableEntry};

pub struct RadioSection;

// Domain Model for a Radio
struct RadioConfig {
    idx: u8,
    phyname: String,
    status: UnifiBool,
    countrycode: u16,
    antenna_gain: u8,
    antenna_id: i32,
    txpower_mode: String,
    txpower: String,   // "auto" or number
    freq: Option<u32>, // Some(freq) or None -> auto
    channel: String,   // "auto" or number
    ieee_mode: String,
    mode: String,
    ack_auto: UnifiBool,
    acktimeout: u32,
    ampdu_status: UnifiBool,
    clksel: u8,
    cwm_enable: u8,
    cwm_mode: u8,
    forbiasauto: u8,
    rate_auto: UnifiBool,
    rate_mcs: String,
    rfscan: UnifiBool,
    bcmc_l2_filter_status: UnifiBool,
    bgscan_status: UnifiBool,
    hard_noisefloor_status: UnifiBool,

    // VAPs associated with this radio
    vaps: Vec<RadioVapConfig>,
}

struct RadioVapConfig {
    idx: u8, // Virtual index (0, 1, 2...)
    devname: String,
    status: UnifiBool,
    mode: String,
}

impl ToUnifiConfig for RadioConfig {
    fn to_config(&self) -> Vec<(String, String)> {
        let p = format!("radio.{}", self.idx);
        let mut kvs = vec![
            (format!("{}.status", p), self.status.to_string()),
            (format!("{}.phyname", p), self.phyname.clone()),
            (format!("{}.countrycode", p), self.countrycode.to_string()),
            (format!("{}.antenna.gain", p), self.antenna_gain.to_string()),
            (format!("{}.antenna", p), self.antenna_id.to_string()),
            (format!("{}.txpower_mode", p), self.txpower_mode.clone()),
            (format!("{}.txpower", p), self.txpower.clone()),
            (format!("{}.ieee_mode", p), self.ieee_mode.clone()),
            (format!("{}.mode", p), self.mode.clone()),
            (format!("{}.ack.auto", p), self.ack_auto.to_string()),
            (format!("{}.acktimeout", p), self.acktimeout.to_string()),
            (format!("{}.ampdu.status", p), self.ampdu_status.to_string()),
            (format!("{}.clksel", p), self.clksel.to_string()),
            (format!("{}.cwm.enable", p), self.cwm_enable.to_string()),
            (format!("{}.cwm.mode", p), self.cwm_mode.to_string()),
            (format!("{}.forbiasauto", p), self.forbiasauto.to_string()),
            (format!("{}.rate.auto", p), self.rate_auto.to_string()),
            (format!("{}.rate.mcs", p), self.rate_mcs.clone()),
            (format!("{}.rfscan", p), self.rfscan.to_string()),
            (
                format!("{}.bcmc_l2_filter.status", p),
                self.bcmc_l2_filter_status.to_string(),
            ),
            (
                format!("{}.bgscan.status", p),
                self.bgscan_status.to_string(),
            ),
            (
                format!("{}.hard_noisefloor.status", p),
                self.hard_noisefloor_status.to_string(),
            ),
        ];

        if let Some(freq) = self.freq {
            kvs.push((format!("{}.freq", p), freq.to_string()));
        } else {
            kvs.push((format!("{}.channel", p), self.channel.clone()));
        }

        // Add VAPs
        if let Some(vap0) = self.vaps.first() {
            kvs.push((format!("{}.devname", p), vap0.devname.clone()));
        }

        for vap in self.vaps.iter().skip(1) {
            let vp = format!("{}.virtual.{}", p, vap.idx);
            kvs.push((format!("{}.status", vp), vap.status.to_string()));
            kvs.push((format!("{}.devname", vp), vap.devname.clone()));
            kvs.push((format!("{}.mode", vp), vap.mode.clone()));
        }

        kvs
    }
}

#[derive(Clone)]
pub struct PhysRadio {
    pub idx: u8,
    pub phyname: String,
    pub mode: String,
    pub band: Band,
    pub antenna_gain: u8,
    pub txpower_mode: String,
    pub freq: u32,
}

impl IniSection for RadioSection {
    fn generate(
        &self,
        config: &ProvisionConfig,
        device_state: &DeviceState,
        mac: &str,
    ) -> HashMap<String, Vec<String>> {
        let mut sections = HashMap::new();
        let h = " wlans (radio)";

        let country_code = config.get_effective_country_code_numeric();

        add_line(&mut sections, h, "radio.status=enabled".to_string());
        add_line(
            &mut sections,
            h,
            format!("radio.countrycode={}", country_code),
        );
        add_line(&mut sections, h, "radio.outdoor=disabled".to_string());

        let radios = get_phys_radios(config, device_state, mac);
        let networks = config.get_networks_for_device(mac);
        let mut sorted_networks: Vec<_> = networks.iter().collect();
        sorted_networks.sort_by(|(_k1, n1), (_k2, n2)| {
            n1.vlan.cmp(&n2.vlan).then_with(|| n1.ssid.cmp(&n2.ssid))
        });

        for radio in &radios {
            // Calculate VAPs
            let mut radio_networks = Vec::new();
            for (name, net_config) in &sorted_networks {
                if net_config.bands.contains(&radio.band) {
                    radio_networks.push((name, net_config));
                }
            }

            let mut vaps = Vec::new();
            let mut vap_idx_on_radio = 0;
            for _ in radio_networks {
                let dev_idx = (radio.idx as usize - 1) * 3 + vap_idx_on_radio;
                let devname = format!("ath{}", dev_idx);

                vaps.push(RadioVapConfig {
                    idx: vap_idx_on_radio as u8,
                    devname,
                    status: UnifiBool::Enabled,
                    mode: "master".to_string(),
                });
                vap_idx_on_radio += 1;
            }

            // Build Typed Config
            let radio_cfg = RadioConfig {
                idx: radio.idx,
                phyname: radio.phyname.clone(),
                status: UnifiBool::Enabled,
                countrycode: country_code,
                antenna_gain: radio.antenna_gain,
                antenna_id: -1,
                txpower_mode: radio.txpower_mode.clone(),
                txpower: "auto".to_string(),
                freq: if radio.freq > 0 {
                    Some(radio.freq)
                } else {
                    None
                },
                channel: "auto".to_string(),
                ieee_mode: radio.mode.clone(),
                mode: "master".to_string(),
                ack_auto: UnifiBool::Disabled,
                acktimeout: 64,
                ampdu_status: UnifiBool::Enabled,
                clksel: 1,
                cwm_enable: 0,
                cwm_mode: 0,
                forbiasauto: 0,
                rate_auto: UnifiBool::Enabled,
                rate_mcs: "auto".to_string(),
                rfscan: UnifiBool::Disabled,
                bcmc_l2_filter_status: UnifiBool::Enabled,
                bgscan_status: UnifiBool::Disabled,
                hard_noisefloor_status: UnifiBool::Disabled,
                vaps,
            };

            for (k, v) in radio_cfg.to_config() {
                add_line(&mut sections, h, format!("{}={}", k, v));
            }
        }

        sections
    }
}

// Helper to get radios
pub fn get_phys_radios(
    config: &ProvisionConfig,
    device_state: &DeviceState,
    mac: &str,
) -> Vec<PhysRadio> {
    let mut radios = Vec::new();
    let normalized_mac = mac.replace(":", "").to_lowercase();

    // 1. Try static config
    if let Some(device) = config.devices_info.get(&normalized_mac) {
        for (i, entry) in device.radio_table.iter().enumerate() {
            radios.push(entry_to_phys_radio(i, entry));
        }
    }

    // 2. Try dynamic state
    if radios.is_empty() && !device_state.radio_table.is_empty() {
        for (i, entry) in device_state.radio_table.iter().enumerate() {
            radios.push(entry_to_phys_radio(i, entry));
        }
    }

    // 3. Fallback
    if radios.is_empty() {
        radios = vec![
            PhysRadio {
                idx: 1,
                phyname: "wifi0".to_string(),
                mode: "11nght20".to_string(),
                band: Band::Band2g,
                antenna_gain: 4,
                txpower_mode: "high".to_string(),
                freq: 0,
            },
            PhysRadio {
                idx: 2,
                phyname: "wifi1".to_string(),
                mode: "11naht40".to_string(),
                band: Band::Band5g,
                antenna_gain: 6,
                txpower_mode: "high".to_string(),
                freq: 0,
            },
        ];
    }
    radios
}

fn entry_to_phys_radio(i: usize, entry: &RadioTableEntry) -> PhysRadio {
    let idx = (i + 1) as u8;
    let phyname = entry.name.clone();

    let (band, mode, default_freq, default_tx) = match entry.radio.as_str() {
        "ng" => (Band::Band2g, "11nght20", 0, "high"),
        "na" => (Band::Band5g, "11naht40", 0, "high"),
        "6e" => (Band::Band6g, "11naht40", 6135, "auto"),
        _ => (Band::Band2g, "11nght20", 0, "auto"),
    };

    let antenna_gain = entry
        .antenna_gain
        .as_ref()
        .and_then(|v| v.as_u64())
        .map(|v| v as u8)
        .unwrap_or(0);

    PhysRadio {
        idx,
        phyname,
        mode: mode.to_string(),
        band,
        antenna_gain,
        txpower_mode: default_tx.to_string(),
        freq: default_freq,
    }
}
