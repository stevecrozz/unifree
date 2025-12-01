use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

const UPDATE_URL: &str = "https://fw-update.ubnt.com/api/firmware-latest?filter=eq~~product~~unifi-firmware&filter=eq~~channel~~release";

#[derive(Debug, Clone)]
pub struct FirmwareInfo {
    pub version: String,
    pub url: String,
    pub md5: Option<String>,
    pub platform: String,
}

#[derive(Debug, Deserialize)]
struct UbntResponse {
    _embedded: Embedded,
}

#[derive(Debug, Deserialize)]
struct Embedded {
    firmware: Vec<UbntFirmware>,
}

#[derive(Debug, Deserialize)]
struct UbntFirmware {
    version: String,
    platform: String,
    md5: Option<String>,
    _links: Links,
}

#[derive(Debug, Deserialize)]
struct Links {
    data: Href,
}

#[derive(Debug, Deserialize)]
struct Href {
    href: String,
}

#[derive(Clone)]
pub struct FirmwareManager {
    cache: Arc<RwLock<HashMap<String, FirmwareInfo>>>,
    client: reqwest::Client,
}

impl FirmwareManager {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_updates(&self) -> anyhow::Result<usize> {
        info!("Fetching firmware updates from Ubiquiti...");
        let resp = self
            .client
            .get(UPDATE_URL)
            .send()
            .await?
            .json::<UbntResponse>()
            .await?;

        let mut cache = self.cache.write().await;
        let count = resp._embedded.firmware.len();

        for fw in resp._embedded.firmware {
            // Store by platform (e.g. "BZ2", "U7PG2", etc)
            let info = FirmwareInfo {
                version: fw.version.clone(),
                url: fw._links.data.href,
                md5: fw.md5,
                platform: fw.platform.clone(),
            };

            cache.insert(fw.platform, info);
        }

        info!("Cached {} firmware updates", cache.len());
        Ok(count)
    }

    pub async fn get_update_for_device(
        &self,
        model: &str,
        current_version: &str,
    ) -> Option<FirmwareInfo> {
        let cache = self.cache.read().await;

        // Logic: Try exact model match first.
        // Some devices like UAP-AC-Pro might report "U7PG2" as model, which matches the platform key.
        // Others might report "UAP-AC-Pro" but the firmware is under "BZ2".
        // This mapping is tricky without a known DB.
        // For now, assume the device reports the "platform" code as its model (common in UniFi inform).

        if let Some(fw) = cache.get(model) {
            debug!(
                "Checking update for model {}: current='{}' available='{}'",
                model, current_version, fw.version
            );
            if is_newer(&fw.version, current_version) {
                return Some(fw.clone());
            }
        } else {
            debug!("No firmware info found for model {}", model);
        }

        None
    }
}

fn is_newer(remote: &str, local: &str) -> bool {
    // Basic heuristic: split by dots/pluses and compare numbers
    // Ubiquiti versions: "4.3.28.11361" or "6.0.14+13634"

    // Helper to normalize
    let normalize = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_numeric())
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };

    let r_parts = normalize(remote);
    let l_parts = normalize(local);

    // Lexicographical comparison of version components
    r_parts > l_parts
}
