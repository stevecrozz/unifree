use std::collections::HashMap;
use std::sync::Arc;

use prometheus::{register_gauge_vec, Encoder, GaugeVec, TextEncoder};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;
use unifree_types::Band;

#[derive(Debug, Clone, Serialize)]
pub struct RadioMetrics {
    pub name: String,
    pub band: Band,
    pub channel: Option<i64>,
    pub tx_power: Option<f64>,
    pub utilization: Option<f64>,
    pub noise: Option<f64>,
    pub clients: Option<u64>,
}

impl Default for RadioMetrics {
    fn default() -> Self {
        Self {
            name: String::new(),
            band: Band::Band2g,
            channel: None,
            tx_power: None,
            utilization: None,
            noise: None,
            clients: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceTelemetry {
    pub radio_metrics: Vec<RadioMetrics>,
    pub vap_clients: Vec<(String, u64)>,
    pub cpu_util: Option<f64>,
    pub mem_util: Option<f64>,
}

#[derive(Clone)]
pub struct TelemetryStore {
    inner: Arc<RwLock<HashMap<String, DeviceTelemetry>>>,
    radio_clients: GaugeVec,
    radio_noise: GaugeVec,
    radio_tx_power: GaugeVec,
    radio_utilization: GaugeVec,
    vap_clients: GaugeVec,
    device_cpu: GaugeVec,
    device_mem: GaugeVec,
}

impl TelemetryStore {
    pub fn new() -> Self {
        let radio_clients = register_gauge_vec!(
            "unifree_radio_clients",
            "Number of clients on a radio",
            &["device", "radio"]
        )
        .expect("register radio_clients");
        let radio_noise = register_gauge_vec!(
            "unifree_radio_noise_dbm",
            "Noise floor per radio",
            &["device", "radio"]
        )
        .expect("register radio_noise");
        let radio_tx_power = register_gauge_vec!(
            "unifree_radio_tx_power_dbm",
            "TX power per radio",
            &["device", "radio"]
        )
        .expect("register radio_tx_power");
        let radio_utilization = register_gauge_vec!(
            "unifree_radio_utilization_percent",
            "Channel utilization",
            &["device", "radio"]
        )
        .expect("register radio_utilization");
        let vap_clients = register_gauge_vec!(
            "unifree_vap_clients",
            "Clients per SSID",
            &["device", "ssid"]
        )
        .expect("register vap_clients");
        let device_cpu = register_gauge_vec!(
            "unifree_device_cpu_percent",
            "Device CPU usage",
            &["device"]
        )
        .expect("register device_cpu");
        let device_mem = register_gauge_vec!(
            "unifree_device_mem_percent",
            "Device memory usage",
            &["device"]
        )
        .expect("register device_mem");

        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            radio_clients,
            radio_noise,
            radio_tx_power,
            radio_utilization,
            vap_clients,
            device_cpu,
            device_mem,
        }
    }

    pub async fn update(&self, mac: &str, telemetry: DeviceTelemetry) {
        let key = mac.to_string();
        {
            let mut guard = self.inner.write().await;
            guard.insert(key.clone(), telemetry.clone());
        }

        // Update metrics straight from telemetry
        for radio in &telemetry.radio_metrics {
            if let Some(value) = radio.clients {
                self.radio_clients
                    .with_label_values(&[&key, &radio.name])
                    .set(value as f64);
            }
            if let Some(value) = radio.utilization {
                self.radio_utilization
                    .with_label_values(&[&key, &radio.name])
                    .set(value);
            }
            if let Some(value) = radio.tx_power {
                self.radio_tx_power
                    .with_label_values(&[&key, &radio.name])
                    .set(value);
            }
            if let Some(value) = radio.noise {
                self.radio_noise
                    .with_label_values(&[&key, &radio.name])
                    .set(value);
            }
        }

        for (ssid, count) in &telemetry.vap_clients {
            self.vap_clients
                .with_label_values(&[&key, ssid])
                .set(*count as f64);
        }

        if let Some(cpu) = telemetry.cpu_util {
            self.device_cpu.with_label_values(&[&key]).set(cpu);
        }
        if let Some(mem) = telemetry.mem_util {
            self.device_mem.with_label_values(&[&key]).set(mem);
        }
    }

    pub async fn export_json(&self) -> serde_json::Value {
        let guard = self.inner.read().await;
        serde_json::to_value(&*guard).unwrap_or(serde_json::Value::Null)
    }

    pub fn gather_prometheus() -> Vec<u8> {
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        buffer
    }
}

pub fn extract_radio_metrics(entry: &Value) -> Option<RadioMetrics> {
    let name = entry.get("name")?.as_str()?.to_string();
    let band = entry
        .get("radio")
        .and_then(|v| v.as_str())
        .map(|r| match r {
            "ng" => Band::Band2g,
            "na" => Band::Band5g,
            "6e" => Band::Band6g,
            _ => Band::Band2g,
        })
        .unwrap_or(Band::Band2g);
    Some(RadioMetrics {
        name,
        band,
        channel: entry.get("channel").and_then(|v| v.as_i64()),
        tx_power: entry.get("tx_power").and_then(|v| v.as_f64()).or_else(|| {
            entry
                .get("tx_power")
                .and_then(|v| v.as_i64())
                .map(|v| v as f64)
        }),
        utilization: entry.get("usage").and_then(|v| v.as_f64()),
        noise: entry.get("noise").and_then(|v| v.as_f64()),
        clients: entry.get("num_sta").and_then(|v| v.as_u64()),
    })
}
