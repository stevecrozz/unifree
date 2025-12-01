# unifree

**Declarative UniFi AP Management for NixOS**

A Rust-based daemon that replaces the UniFi Controller for managing UniFi Access Points, with deep NixOS integration enabling fully declarative AP configuration through Nix expressions.

## Status

🚧 **Active development** — feature set is still evolving and breaking changes are expected.

- Controller replacement basics (discovery, adoption, config push) are implemented and used on test hardware.
- Telemetry and metrics are shipping but dashboards / long‑term storage are not bundled yet.
- Declarative NixOS integration is functional for single-site setups; multi-site support and secrets tooling are still TODO.

## Features

- ✅ Discover UniFi APs via UDP broadcast
- ✅ Adopt devices via SSH (`set-adopt` command)
- ✅ Secure communication (AES-GCM encryption)
- ✅ SSH key provisioning for management access
- ✅ Auto-updating of device firmware
- ✅ Full CLI management (upgrade, reset, forget)
- ✅ `/metrics` and `/telemetry` endpoints for Prometheus/Grafana
- ✅ Declarative configuration via NixOS module (per-network, per-device overrides)
- 📋 Device status & telemetry cache exported as JSON
- 🚀 Lightweight (~5MB RAM footprint)

## Quick Start

```bash
# Build
cargo build --release

# Run the daemon (with auto-update enabled)
./target/release/unifreed --http-addr 0.0.0.0:8080 --auto-update --auto-adopt

# List devices
./target/release/unifree list

# Adopt a device (triggers SSH adoption via daemon)
./target/release/unifree adopt <MAC>

# Upgrade a specific device
./target/release/unifree upgrade <MAC>

# Abandon a device (optionally factory reset it)
./target/release/unifree abandon <MAC> --factory-reset
```

## NixOS Module

```nix
{
  services.unifree = {
    enable = true;
    openFirewall = true;
    autoAdopt = true;  # Auto-adopt new factory-default APs

    management = {
      username = "jonas";
      passwordFile = "/run/secrets/unifree-admin-pass";
      sshKey = "ssh-ed25519 AAAAC3Nza...";
    };

    countryCode = "DE";
    timezone = "Europe/Berlin";
    ntpServers = [ "pool.ntp.org" ];
    
    # SSH keys for management access to all APs
    sshKeys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... admin@router"
    ];
    
    # Default networks - applied to ALL adopted APs
    defaultNetworks = {
      home = {
        ssid = "MyNetwork";
        passphraseFile = "/run/secrets/wifi-password";
        security = "wpa2-wpa3";
        bands = [ "2g" "5g" "6g" ];
      };
      
      guest = {
        ssid = "Guest";
        passphraseFile = "/run/secrets/guest-password";
        security = "wpa2";
        bands = [ "2g" "5g" ];
        guest = true;  # Client isolation
        vlan = 100;
      };
      
      iot = {
        ssid = "IoT-Devices";
        passphraseFile = "/run/secrets/iot-password";
        security = "wpa2";
        bands = [ "2g" ];
        vlan = 200;
        hidden = true;
      };
    };
    
    # Device-specific overrides (optional)
    devices."1c0b8b8e177f" = {
      name = "AP-Living-Room";
      led = false;  # Disable LED on this AP
      
      # Override the home network SSID for this AP
      networks.home = {
        ssid = "MyNetwork-LivingRoom";
        passphraseFile = "/run/secrets/wifi-password";
        security = "wpa2-wpa3";
        bands = [ "2g" "5g" "6g" ];
      };
      
      # Disable IoT network on this specific AP
      disabledNetworks = [ "iot" ];
    };
  };
}
```

## Architecture

```
┌─────────────────┐     ┌─────────────────┐
│  unifreed       │     │  unifree (CLI)  │
│  (daemon)       │◄────│                 │
└────────┬────────┘     └─────────────────┘
         │
         │ HTTP POST /inform
         │ UDP 10001 (discovery)
         ▼
┌─────────────────┐
│  UniFi AP       │
└─────────────────┘

### Components

- **unifree-protocol** – inform/discovery packet encode/decode, crypto helpers.
- **unifree-config** – provisioning models and INI generators used by both daemon and CLI.
- **unifree-state** – device state structs shared between CLI/daemon.
- **unifree-adopt** – libssh2-powered adoption helpers (`set-adopt`, `restore-default`).
- **unifree-daemon** – Axum/Tokio service orchestrating discovery, inform handling, adoption, config pushes, and telemetry.
- **unifree-cli** – operator tool for manual actions and troubleshooting.

All crates live in this workspace and can be built/tested individually via `cargo check -p <crate>`.

### Telemetry & Monitoring

- `GET /metrics` exposes Prometheus-format gauges/counters for radio client counts, TX power, noise floor, per-SSID client counts, and device CPU/memory utilization.
- `GET /telemetry` returns the latest per-device RF snapshot as JSON for dashboards that need richer detail (client RSSI, per-radio stats, etc.).
- Downstream systems (Prometheus, Grafana, Loki, etc.) are expected to scrape/persist data; the daemon only keeps the latest snapshot per device.

## Roadmap

- [ ] Grafana dashboards and recording rules based on the new metrics.
- [ ] Multi-site / multi-controller coordination (federated configs).
- [ ] Switch/bridge support (LLDP, port/VLAN telemetry).
- [ ] Pluggable secrets management (Vault/age for SSH credentials).
- [ ] Web UI for light-touch operations (device overview, adoption queue).
```

## License

MIT
