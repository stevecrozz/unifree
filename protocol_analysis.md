# Protocol & Implementation Analysis: TNBU vs. unifree

This document provides a detailed comparison between the reference C# implementation (`TNBU`) and the current Rust implementation (`unifree`). The goal is to identify gaps in protocol handling, state management, and feature sets to guide the development of `unifree`.

## 1. Protocol & Data Structures

### 1.1 Inform Packet Header & Encryption (`TNBU.Core` vs `unifree-protocol`)

*   **Structure:** Both implementations correctly parse the 40-byte binary header (`TNBU` magic, version, MAC, flags, IV, payload version, payload length).
*   **Encryption:**
    *   **AES-CBC:** Both support AES-CBC with PKCS7 padding.
    *   **AES-GCM:** Both support AES-GCM.
    *   **Key Management:** Both default to the well-known key `ba86f2bbe107c7c57eb5f2690775c712` (md5("ubnt")) before adoption.
*   **Compression:**
    *   **ZLIB:** Both support ZLIB compression.
    *   **Snappy:** `TNBU` supports Snappy (`IronSnappy`). `unifree` **lacks Snappy support**. While most devices default to ZLIB or uncompressed, newer firmwares or specific models might prefer Snappy.
*   **Implementation Note:** `unifree` uses a builder pattern (`InformResponseBuilder`) which is cleaner than `TNBU`'s direct modification of properties.

### 1.2 Inform Payload (`BaseInformBody.cs` vs `types.rs`)

`TNBU` splits the payload into `BaseInformBody` and `ExtendedInformBody`. `unifree` uses a single `InformRequest` struct.

| Field | TNBU (`BaseInformBody.cs`) | unifree (`InformRequest`) | Notes |
| :--- | :--- | :--- | :--- |
| `cfgversion` | `string?` | `String` (default empty) | Matching. |
| `default` | `bool` | `bool` | Matching. |
| `inform_as_notif` | `bool` | `bool` | **Added recently** to `unifree`. |
| `notif_reason` | `string?` | `Option<String>` | **Added recently** to `unifree`. |
| `notif_payload` | `object?` | `Option<Value>` | **Added recently** to `unifree`. |
| `state` | `int` | `u8` | Matching. |
| `radio_table` | `Radio_Table[]` | `Vec<Value>` (parsed manually) | `unifree` parses this manually in `handle_inform`. `TNBU` has a structured model. |
| `scan_table` | In `Radio_Table` | **Missing** | `unifree` ignores scan results (used for wireless uplink/mesh). |
| `lldp_table` | `Lldp_Table[]` | `Vec<Value>` | `unifree` stores but doesn't strictly type this. |
| `port_table` | `Port_Table[]` | `Vec<Value>` | `unifree` stores but doesn't strictly type this. |
| `vport_table` | In `ExtendedInformBody` | `Vec<Value>` | `unifree` generic handling. |

**Missing/Weak Typed Fields in `unifree`:**
*   `scan_table`: Critical for wireless meshing/uplink detection.
*   `port_table`: Critical for switch configuration and topology mapping.
*   `lldp_table`: Useful for topology mapping.

### 1.3 Configuration Models (`TNBU.Core.Models.DeviceConfiguration` vs `unifree-common/src/config`)

`TNBU` has a very granular, object-oriented configuration model that serializes to the INI format via `ToString()` overrides. `unifree` uses a more functional/generative approach.

*   **Wireless/Radio:** `TNBU` has `CfgRadio`, `CfgWireless`, `CfgAaa`. `unifree` generates these sections dynamically in `generator.rs`. The logic is similar, but `TNBU` explicitly models things like `CfgAaaEntry` which maps to `aaa.X.status`, etc.
*   **Switches:** `TNBU` has extensive support for `CfgSwitch`, `CfgSwitchEntry`, `CfgSwitchVLAN`. `unifree` has basic switch/bridge support but lacks per-port VLAN tagging/untagging logic and LAG support found in `TNBU`.
*   **Mesh:** `TNBU` has `CfgMesh` for wireless uplinks. `unifree` has placeholders but no active mesh logic.

## 2. State Machine & Controller Logic

### 2.1 State Handling (`DeviceManagerService.cs` vs `unifree-daemon`)

`TNBU` maintains explicit state flags on the `Device` object:
*   `IsAdopted`
*   `IsAdopting`
*   `IsConfiguring`
*   `IsUpdating`
*   `IsResetting`
*   `IsOnline` / `IsInformValid` (via timestamps)

`unifree` uses a single `DeviceStatus` enum:
*   `Discovered`
*   `Adopting`
*   `Adopted`
*   `Provisioning`
*   `Offline`

**Key Logic Differences:**

1.  **"Magic" Config Version (`ADOPT_CFG`):**
    *   **TNBU:** When sending the initial `SetAdopt` command (which sends the auth key), it sets the `cfgversion` in the payload to a constant `ADOPT_CFG` ("0123456789abcdef").
    *   **TNBU:** When the device reports back with `cfgversion == ADOPT_CFG`, it interprets this as "Adoption Successful" and transitions to `IsConfiguring`, immediately pushing the full config.
    *   **Unifree:** Sends a random `cfgversion` during adoption. It waits for the device to report back, checks if the config version matches the target hash, and if not, pushes the system config. This is functionally similar but `TNBU`'s explicit handshake with a known magic string is more deterministic.

2.  **Extended Parsing (Recursive Adoption):**
    *   **TNBU:** In `GotInform`, it recursively checks `extendedInformBody.radio_table` -> `scan_table`. If it finds a device in the scan table with `is_vport = true` (wireless uplink), it **auto-registers** that device as known! This allows for zero-touch adoption of wireless mesh points.
    *   **Unifree:** Lacks this logic. Wireless meshing requires manual adoption/pre-configuration or isn't supported automatically.

3.  **Firmware Updates:**
    *   **TNBU:** `DeviceManagerService` checks `device.FirmwareUpdate`. If present, it sends `InformResponse.Upgrade`. It uses a dedicated `FirmwareManager` to fetch versions from Ubiquiti's API.
    *   **Unifree:** No firmware update logic.

### 2.2 Configuration Building (`ConfigurationBuilderService.cs`)

*   **TNBU:** Rebuilds the *entire* `SystemConfig` object on every inform where configuration is needed. It calculates the MD5 hash of the generated config to check against the device's reported version.
*   **Unifree:** Does essentially the same: generates the INI string, hashes it, and compares.
*   **Hardcoded Tweaks:** `TNBU` has some hardcoded MAC-address specific hacks (e.g., `if(d.Mac.ToString() == "70A741C418A8")`) for VLANs on specific switches. This highlights the complexity of switch configuration.

## 3. Missing Features in `unifree`

1.  **Firmware Updates:** The ability to detect available updates and command the device to upgrade (`_type: "upgrade"`).
2.  **Wireless Uplink / Mesh:** Logic to parse `scan_table`, detect isolated APs, and provision them via wireless backhaul (`vwire`, `vport`). `TNBU` handles this explicitly with `set-meshv3-payload` commands.
3.  **Switch Port Profiles:** Granular control over switch ports (PoE, VLAN tagging/untagging, isolation, storm control). `unifree` is mostly AP-focused.
4.  **Snappy Compression:** Handling `IsSnappy` flag in packets.
5.  **Advanced Commands:** `TNBU` supports `reboot`, `power-cycle` (for PoE ports), `set-locate`, etc. `unifree` protocol library supports them, but daemon doesn't expose them fully.

## 4. Recommendations for `unifree`

1.  **Adopt "Magic" Adoption Config:** Consider using a constant `cfgversion` (like `0123456789abcdef`) for the initial `mgmt_cfg` push. This provides a clear signal that the device has accepted the auth key and is ready for the full system config.
2.  **Implement Snappy:** Add `snap` or `rust-snappy` dependency to support Snappy compression/decompression, future-proofing the controller.
3.  **Expand Data Models:** Create strictly typed structs for `ScanTable`, `LldpTable`, and `PortTable` in `unifree-common` to pave the way for switch and mesh support.
4.  **Firmware Update Manager:** Create a `FirmwareManager` struct/task that periodically checks for updates and exposes an API to trigger upgrades.
