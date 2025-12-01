use crate::config::models::ProvisionConfig;

impl ProvisionConfig {
    /// Generate mgmt_cfg INI for a device
    pub fn generate_mgmt_cfg(&self, mac: &str) -> String {
        self.generate_mgmt_cfg_with_auth(mac, None, None, "0000000000000000")
    }

    /// Generate mgmt_cfg INI for a device with optional auth key and inform URL
    pub fn generate_mgmt_cfg_with_auth(
        &self,
        mac: &str,
        _auth_key: Option<&str>,
        _inform_url: Option<&str>,
        cfgversion: &str,
    ) -> String {
        let led_enabled = self.get_led_for_device(mac);

        let mut lines = Vec::new();

        let normalized_mac = mac.replace(":", "").to_lowercase();
        let device = self.devices_info.get(&normalized_mac);

        // Required capability flags
        lines.push("capability=notif,notif-assoc-stat".to_string());

        // Prefer caller-provided cfgversion so provisioning can advance hashes
        let effective_cfgversion = if !cfgversion.is_empty() {
            cfgversion.to_string()
        } else if let Some(d) = device {
            d.cfgversion.clone()
        } else {
            cfgversion.to_string()
        };
        lines.push(format!("cfgversion={}", effective_cfgversion));

        lines.push(format!("led_enabled={}", led_enabled));
        lines.push("report_crash=true".to_string());
        lines.push("selfrun_guest_mode=pass".to_string());

        // mgmt_url and stun_url
        if let Some(d) = device {
            if let Some(inform_url) = &d.inform_url {
                lines.push(format!(
                    "mgmt_url={}",
                    inform_url.replace(":8081/inform", ":8443/manage/site/default")
                ));
            }
            if let Some(inform_ip) = &d.inform_ip {
                // Check for stun_url in settings, fallback to inform_ip
                let stun_url = self
                    .management
                    .stun_url
                    .clone()
                    .unwrap_or(format!("stun://{}:3478/", inform_ip));
                lines.push(format!("stun_url={}", stun_url));
            }
        }

        lines.push("use_aes_gcm=true".to_string());

        // Note: authkey and inform_url are NOT included in mgmt_cfg in official traces.
        // They are handled by the adoption process (set-adopt).
        // Including them might cause mcad to crash or malfunction.

        // SSH keys in mgmt_cfg
        let mut all_keys = self.ssh_keys.clone();
        if let Some(k) = &self.management.ssh_key {
            if let Some(parsed) = crate::config::models::parse_ssh_pubkey(k) {
                all_keys.push(parsed);
            }
        }

        for (i, key) in all_keys.iter().enumerate() {
            let idx = i + 1;
            lines.push(format!("mgmt.sshkeys.{}.type={}", idx, key.key_type));
            lines.push(format!("mgmt.sshkeys.{}.value={}", idx, key.value));
            if let Some(ref comment) = key.comment {
                lines.push(format!("mgmt.sshkeys.{}.comment={}", idx, comment));
            }
        }

        lines.join("\n")
    }
}
