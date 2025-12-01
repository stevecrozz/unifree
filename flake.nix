{
  description = "unifree - Declarative UniFi AP Management for NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        unifree = pkgs.rustPlatform.buildRustPackage {
          pname = "unifree";
          version = "0.1.0";
          
          src = ./.;
          
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          
          nativeBuildInputs = with pkgs; [
            pkg-config
          ];
          
          buildInputs = with pkgs; [
            openssl
          ];
          
          meta = with pkgs.lib; {
            description = "Declarative UniFi AP Management for NixOS";
            homepage = "https://github.com/jonasgrosch/unifree";
            license = licenses.mit;
            platforms = platforms.linux;
          };
        };
      in
      {
        packages = {
          default = unifree;
          inherit unifree;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            cargo-watch
          ];
          
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      }
    ) // {
      # NixOS module
      nixosModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.services.unifree;
          inherit (lib) mkEnableOption mkOption types mkIf;
          networkType =
            types.submodule {
              options = {
                ssid = mkOption {
                  type = types.str;
                  description = "WiFi network name (SSID)";
                };

                passphrase = mkOption {
                  type = types.nullOr types.str;
                  default = null;
                  description = "WiFi password (use passphraseFile to avoid storing secrets in the Nix store).";
                };

                passphraseFile = mkOption {
                  type = types.nullOr types.path;
                  default = null;
                  description = "Path to a file containing the WiFi password (plain text).";
                };

                security = mkOption {
                  type = types.enum [ "open" "wpa2" "wpa3" "wpa2-wpa3" ];
                  default = "wpa2-wpa3";
                  description = "Security mode";
                };

                bands = mkOption {
                  type = types.listOf (types.enum [ "2g" "5g" "6g" ]);
                  default = [ "2g" "5g" ];
                  description = "Radio bands to broadcast on";
                };

                vlan = mkOption {
                  type = types.nullOr types.int;
                  default = null;
                  description = "VLAN ID for this network";
                };

                hidden = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Hide SSID from broadcasts";
                };

                guest = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Guest network with client isolation";
                };

                clientDeviceIsolation = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Enable client-device isolation on this network.";
                };

                iot = mkOption {
                  type = types.bool;
                  default = false;
                  description = "Mark the network as IoT for controller-specific behavior.";
                };

                pmf = mkOption {
                  type = types.int;
                  default = 0;
                  apply = value:
                    if value >= 0 && value <= 2 then value
                    else throw "services.unifree network pmf must be 0 (disabled), 1 (optional) or 2 (required)";
                  description = "Protected Management Frames mode (0 = disabled, 1 = optional, 2 = required).";
                };
              };
            };
        in
        {
          options.services.unifree = {
            enable = mkEnableOption "unifree UniFi AP management daemon";
            
            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.system}.unifree;
              description = "The unifree package to use";
            };
            
            httpAddress = mkOption {
              type = types.str;
              default = "0.0.0.0";
              description = "Address to listen on for HTTP (inform endpoint)";
            };
            
            httpPort = mkOption {
              type = types.port;
              default = 8080;
              description = "Port for HTTP inform endpoint";
            };
            
            discoveryPort = mkOption {
              type = types.port;
              default = 10001;
              description = "UDP port for device discovery";
            };
            
            stateDir = mkOption {
              type = types.path;
              default = "/var/lib/unifree";
              description = "Directory for storing device state";
            };
            
            logLevel = mkOption {
              type = types.enum [ "trace" "debug" "info" "warn" "error" ];
              default = "info";
              description = "Log level for the daemon";
            };
            
            autoAdopt = mkOption {
              type = types.bool;
              default = false;
              description = ''
                Automatically adopt new devices when discovered.
                Only devices in factory-default state will be adopted.
                Use with caution in shared network environments!
              '';
            };

            autoUpdate = mkOption {
              type = types.bool;
              default = false;
              description = ''
                Automatically upgrade adopted devices to the latest firmware when an update is available.
                Requires devices to be fully adopted and reporting model/firmware info.
              '';
            };
            
            informUrl = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = ''
                Inform URL for devices to connect to.
                If null, auto-detected from local IP and httpPort.
              '';
              example = "http://192.168.1.1:8080/inform";
            };
            
            openFirewall = mkOption {
              type = types.bool;
              default = false;
              description = "Open firewall ports for unifree";
            };
            
            # Global SSH management
            sshKeys = mkOption {
              type = types.listOf types.str;
              default = [];
              description = ''
                SSH public keys to provision on all managed devices.
                Format: "ssh-ed25519 AAAAC3... comment" or "ssh-rsa AAAAB3... comment"
                These keys allow SSH access for troubleshooting/management.
              '';
              example = [ "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... admin@router" ];
            };
            
            sshKeyFiles = mkOption {
              type = types.listOf types.path;
              default = [];
              description = ''
                Paths to files containing SSH public keys (one per line).
                These are read at service start time.
              '';
              example = [ "/etc/secrets/unifree-ssh-keys" ];
            };

            management = mkOption {
              type = types.submodule {
                options = {
                  username = mkOption {
                    type = types.nullOr types.str;
                    default = null;
                    description = "Username to provision on managed devices (falls back to vendor default when null).";
                  };

                  password = mkOption {
                    type = types.nullOr types.str;
                    default = null;
                    description = ''
                      Password to provision for the management user. Avoid storing secrets directly;
                      prefer `passwordFile` which is injected at runtime and never written to the Nix store.
                    '';
                  };

                  passwordFile = mkOption {
                    type = types.nullOr types.path;
                    default = null;
                    description = ''
                      Path to a file containing the management password (plain text or SHA-512 crypt hash).
                      If provided, the value is injected into the runtime config before the service starts.
                    '';
                  };

                  sshKey = mkOption {
                    type = types.nullOr types.str;
                    default = null;
                    description = "Optional SSH public key to include in mgmt_cfg for the management user.";
                  };

                  stunUrl = mkOption {
                    type = types.nullOr types.str;
                    default = null;
                    description = "Optional STUN URL advertised to devices.";
                  };
                };
              };
              default = {};
              description = "Management configuration stored in the JSON provision file.";
            };

            countryCode = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "ISO country code (e.g. \"US\", \"DE\") used for radio settings. Defaults to US when unset.";
            };

            timezone = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Controller timezone (IANA or POSIX). When null the daemon auto-detects.";
            };

            ntpServers = mkOption {
              type = types.listOf types.str;
              default = [];
              description = "Global NTP servers to write into system.cfg.";
            };
            
            # Default networks applied to all devices
            defaultNetworks = mkOption {
              type = types.attrsOf networkType;
              default = {};
              description = ''
                Default WiFi networks applied to all managed APs.
                Device-specific overrides take precedence.
              '';
              example = lib.literalExpression ''
                {
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
                    guest = true;
                    vlan = 100;
                  };
                }
              '';
            };
            
            # Device-specific configuration
            devices = mkOption {
              type = types.attrsOf (types.submodule {
                options = {
                  name = mkOption {
                    type = types.nullOr types.str;
                    default = null;
                    description = "Human-readable name for the device";
                  };
                  
                  networks = mkOption {
                    type = types.attrsOf networkType;
                    default = {};
                    description = "Device-specific network overrides";
                  };
                  
                  disabledNetworks = mkOption {
                    type = types.listOf types.str;
                    default = [];
                    description = "Names of default networks to disable on this device";
                  };
                  
                  led = mkOption {
                    type = types.nullOr types.bool;
                    default = null;
                    description = "LED override for this device (true = on, false = off).";
                  };
                };
              });
              default = {};
              description = "Device-specific configuration (key is MAC address without colons, lowercase)";
            };
          };
          
          config = mkIf cfg.enable {
            # Generate config file
            environment.etc."unifree/config.json".text = let
              # Convert network config to JSON-compatible format
              networkToJson = _: net: {
                ssid = net.ssid;
                passphrase = net.passphrase;
                security = net.security;
                bands = net.bands;
                vlan = net.vlan;
                hidden = net.hidden;
                guest = net.guest;
                client_device_isolation = net.clientDeviceIsolation;
                iot = net.iot;
                pmf = net.pmf;
              };
              
              # Convert SSH keys
              sshKeysJson = map (k: let
                parts = lib.splitString " " k;
              in {
                key_type = lib.elemAt parts 0;
                value = lib.elemAt parts 1;
                comment = if lib.length parts > 2 then lib.elemAt parts 2 else null;
              }) cfg.sshKeys;

              managementJson = {
                username = cfg.management.username;
                password = cfg.management.password;
                ssh_key = cfg.management.sshKey;
                stun_url = cfg.management.stunUrl;
              };
              
              # Build config object
              configJson = {
                management = managementJson;
                country_code = cfg.countryCode;
                timezone = cfg.timezone;
                ntp_servers = cfg.ntpServers;
                networks = lib.mapAttrs networkToJson cfg.defaultNetworks;
                ssh_keys = sshKeysJson;
                devices = lib.mapAttrs (mac: dev: {
                  name = dev.name;
                  networks = lib.mapAttrs networkToJson dev.networks;
                  disabled_networks = dev.disabledNetworks;
                  led = dev.led;
                }) cfg.devices;
              };
            in builtins.toJSON configJson;
            
            # Systemd service
            systemd.services.unifree = {
              description = "UniFi AP Management Daemon";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];
              
              # Script to read passwords from files and update config
              preStart = let
                readDefaultPassphrases = lib.concatStringsSep "\n" (
                  lib.mapAttrsToList (name: net:
                    lib.optionalString (net.passphraseFile != null) ''
                      if [ -f ${lib.escapeShellArg net.passphraseFile} ]; then
                        pass=$(cat ${lib.escapeShellArg net.passphraseFile})
                        ${pkgs.jq}/bin/jq \
                          --arg name ${lib.escapeShellArg name} \
                          --arg pass "$pass" \
                          '.networks[$name].passphrase = $pass' \
                          "$RUNTIME_DIRECTORY/config.json" > "$RUNTIME_DIRECTORY/config.json.tmp"
                        mv "$RUNTIME_DIRECTORY/config.json.tmp" "$RUNTIME_DIRECTORY/config.json"
                      fi
                    ''
                  ) cfg.defaultNetworks
                );

                readDevicePassphrases = lib.concatStringsSep "\n" (
                  lib.mapAttrsToList (mac: dev:
                    lib.concatStringsSep "\n" (
                      lib.mapAttrsToList (name: net:
                        lib.optionalString (net.passphraseFile != null) ''
                          if [ -f ${lib.escapeShellArg net.passphraseFile} ]; then
                            pass=$(cat ${lib.escapeShellArg net.passphraseFile})
                            ${pkgs.jq}/bin/jq \
                              --arg mac ${lib.escapeShellArg mac} \
                              --arg name ${lib.escapeShellArg name} \
                              --arg pass "$pass" \
                              '.devices[$mac].networks[$name].passphrase = $pass' \
                              "$RUNTIME_DIRECTORY/config.json" > "$RUNTIME_DIRECTORY/config.json.tmp"
                            mv "$RUNTIME_DIRECTORY/config.json.tmp" "$RUNTIME_DIRECTORY/config.json"
                          fi
                        ''
                      ) dev.networks
                    )
                  ) cfg.devices
                );

                readManagementPassword = lib.optionalString (cfg.management.passwordFile != null) ''
                  if [ -f ${lib.escapeShellArg cfg.management.passwordFile} ]; then
                    pass=$(cat ${lib.escapeShellArg cfg.management.passwordFile})
                    ${pkgs.jq}/bin/jq \
                      --arg pass "$pass" \
                      '.management.password = $pass' \
                      "$RUNTIME_DIRECTORY/config.json" > "$RUNTIME_DIRECTORY/config.json.tmp"
                    mv "$RUNTIME_DIRECTORY/config.json.tmp" "$RUNTIME_DIRECTORY/config.json"
                  fi
                '';
              in ''
                cp /etc/unifree/config.json $RUNTIME_DIRECTORY/config.json
                ${readDefaultPassphrases}
                ${readDevicePassphrases}
                ${readManagementPassword}
              '';
              
              serviceConfig = {
                Type = "simple";
                RuntimeDirectory = "unifree";
                ExecStart = let
                  sshKeyFileArgs = lib.concatMapStringsSep " " (f: "--ssh-key-file '${f}'") cfg.sshKeyFiles;
                  autoAdoptArg = lib.optionalString cfg.autoAdopt "--auto-adopt";
                  autoUpdateArg = lib.optionalString cfg.autoUpdate "--auto-update";
                  informUrlArg = lib.optionalString (cfg.informUrl != null) "--inform-url '${cfg.informUrl}'";
                in "${cfg.package}/bin/unifreed --http-addr ${cfg.httpAddress}:${toString cfg.httpPort} --discovery-port ${toString cfg.discoveryPort} --state-dir ${cfg.stateDir} --log-level ${cfg.logLevel} --config /run/unifree/config.json ${autoAdoptArg} ${autoUpdateArg} ${informUrlArg} ${sshKeyFileArgs}";
                Restart = "always";
                RestartSec = 5;
                
                # Hardening
                DynamicUser = true;
                StateDirectory = "unifree";
                AmbientCapabilities = [ "CAP_NET_BIND_SERVICE" ];
                CapabilityBoundingSet = [ "CAP_NET_BIND_SERVICE" ];
                NoNewPrivileges = true;
                ProtectSystem = "strict";
                ProtectHome = true;
                PrivateTmp = true;
                PrivateDevices = true;
                ProtectKernelTunables = true;
                ProtectKernelModules = true;
                ProtectControlGroups = true;
              };
            };
            
            # Firewall rules
            networking.firewall = mkIf cfg.openFirewall {
              allowedTCPPorts = [ cfg.httpPort ];
              allowedUDPPorts = [ cfg.discoveryPort ];
            };
          };
        };
        
      # Overlay
      overlays.default = final: prev: {
        unifree = self.packages.${prev.system}.unifree;
      };
    };
}
