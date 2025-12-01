//! Discovery packet encoding and decoding
//!
//! Discovery packets are broadcast over UDP port 10001 and contain device information
//! in a TLV (Type-Length-Value) format.

use std::collections::HashMap;
use std::net::IpAddr;

use crate::{Error, MacAddress, PayloadType, Result, DISCOVERY_TYPE_BROADCAST};

/// Parsed discovery packet
#[derive(Debug, Clone)]
pub struct DiscoveryPacket {
    /// Protocol version
    pub version: u8,
    /// Discovery type (usually 0x06 for device broadcasts)
    pub discovery_type: u8,
    /// TLV payloads
    pub payloads: HashMap<u8, Vec<u8>>,
}

impl DiscoveryPacket {
    /// Create a new empty discovery packet
    pub fn new() -> Self {
        Self {
            version: 1,
            discovery_type: DISCOVERY_TYPE_BROADCAST,
            payloads: HashMap::new(),
        }
    }

    /// Decode a discovery packet from bytes
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(Error::PacketTooShort {
                expected: 4,
                actual: data.len(),
            });
        }

        let version = data[0];
        let discovery_type = data[1];
        let total_length = u16::from_be_bytes([data[2], data[3]]) as usize + 4;

        if data.len() < total_length {
            return Err(Error::PacketTooShort {
                expected: total_length,
                actual: data.len(),
            });
        }

        let mut payloads = HashMap::new();
        let mut index = 4;

        while index < total_length {
            if index + 3 > data.len() {
                break;
            }

            let payload_type = data[index];
            let payload_length = u16::from_be_bytes([data[index + 1], data[index + 2]]) as usize;
            let payload_start = index + 3;
            let payload_end = payload_start + payload_length;

            if payload_end > data.len() {
                return Err(Error::InvalidPacket(format!(
                    "Payload extends beyond packet: end {} > len {}",
                    payload_end,
                    data.len()
                )));
            }

            let payload_data = data[payload_start..payload_end].to_vec();
            payloads.insert(payload_type, payload_data);

            index = payload_end;
        }

        Ok(Self {
            version,
            discovery_type,
            payloads,
        })
    }

    /// Encode the discovery packet to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut payload_bytes = Vec::new();

        for (&payload_type, payload_data) in &self.payloads {
            payload_bytes.push(payload_type);
            payload_bytes.extend_from_slice(&(payload_data.len() as u16).to_be_bytes());
            payload_bytes.extend_from_slice(payload_data);
        }

        let mut result = vec![self.version, self.discovery_type];
        result.extend_from_slice(&(payload_bytes.len() as u16).to_be_bytes());
        result.extend(payload_bytes);

        result
    }

    /// Get MAC address from the packet
    pub fn mac(&self) -> Option<MacAddress> {
        self.payloads
            .get(&(PayloadType::MacIp.into()))
            .and_then(|data| MacAddress::from_bytes(&data[..6]))
            .or_else(|| {
                self.payloads
                    .get(&(PayloadType::Mac.into()))
                    .and_then(|data| MacAddress::from_bytes(data))
            })
    }

    /// Get IP address from the packet
    pub fn ip(&self) -> Option<IpAddr> {
        self.payloads
            .get(&(PayloadType::MacIp.into()))
            .filter(|data| data.len() >= 10)
            .map(|data| IpAddr::from([data[6], data[7], data[8], data[9]]))
    }

    /// Get device model from the packet
    pub fn model(&self) -> Option<String> {
        self.payloads
            .get(&(PayloadType::ShortModel.into()))
            .or_else(|| self.payloads.get(&(PayloadType::ShortModel2.into())))
            .map(|data| String::from_utf8_lossy(data).to_string())
    }

    /// Get hostname from the packet
    pub fn hostname(&self) -> Option<String> {
        self.payloads
            .get(&(PayloadType::Hostname.into()))
            .map(|data| String::from_utf8_lossy(data).to_string())
    }

    /// Get firmware version from the packet
    pub fn firmware_version(&self) -> Option<String> {
        self.payloads
            .get(&(PayloadType::Version.into()))
            .or_else(|| self.payloads.get(&(PayloadType::Firmware.into())))
            .map(|data| String::from_utf8_lossy(data).to_string())
    }

    /// Get SSH port from the packet
    pub fn ssh_port(&self) -> Option<u16> {
        self.payloads
            .get(&(PayloadType::SshPort.into()))
            .filter(|data| data.len() >= 2)
            .map(|data| u16::from_be_bytes([data[0], data[1]]))
    }

    /// Check if device is in default (not adopted) state
    pub fn is_default(&self) -> bool {
        self.payloads
            .get(&(PayloadType::IsDefault.into()))
            .map(|data| !data.is_empty() && data[0] != 0)
            .unwrap_or(false)
    }

    /// Check if device is locating (LED flashing)
    pub fn is_locating(&self) -> bool {
        self.payloads
            .get(&(PayloadType::IsLocating.into()))
            .map(|data| !data.is_empty() && data[0] != 0)
            .unwrap_or(false)
    }

    /// Set MAC address payload
    pub fn set_mac(&mut self, mac: MacAddress) {
        self.payloads
            .insert(PayloadType::Mac.into(), mac.as_bytes().to_vec());
    }

    /// Set MAC + IP payload
    pub fn set_mac_ip(&mut self, mac: MacAddress, ip: IpAddr) {
        let mut data = mac.as_bytes().to_vec();
        if let IpAddr::V4(ipv4) = ip {
            data.extend_from_slice(&ipv4.octets());
        }
        self.payloads.insert(PayloadType::MacIp.into(), data);
    }

    /// Set SSH port payload
    pub fn set_ssh_port(&mut self, port: u16) {
        self.payloads
            .insert(PayloadType::SshPort.into(), port.to_be_bytes().to_vec());
    }

    /// Set serial number (as MAC address bytes)
    pub fn set_serial(&mut self, mac: MacAddress) {
        self.payloads
            .insert(PayloadType::Serial.into(), mac.as_bytes().to_vec());
    }
}

impl Default for DiscoveryPacket {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a discovered device
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub mac: MacAddress,
    pub ip: IpAddr,
    pub model: Option<String>,
    pub hostname: Option<String>,
    pub firmware_version: Option<String>,
    pub ssh_port: u16,
    pub is_default: bool,
    pub is_locating: bool,
}

impl TryFrom<DiscoveryPacket> for DiscoveredDevice {
    type Error = Error;

    fn try_from(packet: DiscoveryPacket) -> Result<Self> {
        let mac = packet
            .mac()
            .ok_or_else(|| Error::InvalidPacket("Missing MAC address".to_string()))?;
        let ip = packet
            .ip()
            .ok_or_else(|| Error::InvalidPacket("Missing IP address".to_string()))?;

        Ok(Self {
            mac,
            ip,
            model: packet.model(),
            hostname: packet.hostname(),
            firmware_version: packet.firmware_version(),
            ssh_port: packet.ssh_port().unwrap_or(22),
            is_default: packet.is_default(),
            is_locating: packet.is_locating(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_packet_roundtrip() {
        let mut packet = DiscoveryPacket::new();
        let mac: MacAddress = "1c:0b:8b:8e:17:7f".parse().unwrap();
        let ip: IpAddr = "192.168.1.100".parse().unwrap();

        packet.set_mac(mac);
        packet.set_mac_ip(mac, ip);
        packet.set_ssh_port(22);

        let encoded = packet.encode();
        let decoded = DiscoveryPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.mac(), Some(mac));
        assert_eq!(decoded.ip(), Some(ip));
        assert_eq!(decoded.ssh_port(), Some(22));
    }

    #[test]
    fn test_discovered_device_conversion() {
        let mut packet = DiscoveryPacket::new();
        let mac: MacAddress = "1c:0b:8b:8e:17:7f".parse().unwrap();
        let ip: IpAddr = "192.168.1.100".parse().unwrap();

        packet.set_mac_ip(mac, ip);
        packet.set_ssh_port(22);

        let device: DiscoveredDevice = packet.try_into().unwrap();
        assert_eq!(device.mac, mac);
        assert_eq!(device.ip, ip);
        assert_eq!(device.ssh_port, 22);
    }
}
