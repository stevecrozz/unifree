# unifree

**Declarative UniFi AP Management for NixOS**

A Rust-based daemon that replaces the UniFi Controller for managing UniFi Access Points, with deep NixOS integration enabling fully declarative AP configuration through Nix expressions.

## Status

🚧 **Work in Progress** - Not yet ready for production use.

## Features

- ✅ Discover UniFi APs via UDP broadcast
- ✅ Adopt devices via SSH (`set-adopt` command)
- ✅ Secure communication (AES-GCM encryption)
- ✅ SSH key provisioning for management access
- 🚧 Declarative configuration via NixOS module
- 📋 Device status monitoring
- 🚀 Lightweight (~5MB RAM footprint)

## Quick Start

```bash
# Build
cargo build --release

# Run the daemon
./target/release/unifreed --http-addr 0.0.0.0:8080

# List devices
./target/release/unifree list
```

## NixOS Module

```nix
{
  services.unifree = {
    enable = true;
    openFirewall = true;
    autoAdopt = true;  # Auto-adopt new factory-default APs
    
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
      ledEnabled = false;  # Disable LED on this AP
      
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
```

## License

MIT
