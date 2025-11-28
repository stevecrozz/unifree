//! Inform packet encoding and decoding
//!
//! Inform packets are the main communication mechanism between UniFi devices
//! and the controller. They are sent as HTTP POST requests with encrypted JSON payloads.

use crate::crypto::{self, AesKey};
use crate::{Error, InformFlags, InformRequest, InformResponse, MacAddress, Result, INFORM_MAGIC};

/// Raw inform packet before decryption
#[derive(Debug, Clone)]
pub struct InformPacket {
    /// Packet version
    pub version: u32,
    /// MAC address of the device
    pub mac: MacAddress,
    /// Encryption/compression flags
    pub flags: InformFlags,
    /// Initialization vector for encryption
    pub iv: [u8; 16],
    /// Payload version
    pub payload_version: u32,
    /// Encrypted/compressed payload
    pub payload: Vec<u8>,
    /// Header bytes (used as AAD for GCM)
    header: [u8; 40],
}

impl InformPacket {
    /// Minimum packet size (header only)
    const HEADER_SIZE: usize = 40;

    /// Decode an inform packet from raw bytes
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < Self::HEADER_SIZE {
            return Err(Error::PacketTooShort {
                expected: Self::HEADER_SIZE,
                actual: data.len(),
            });
        }

        // Check magic bytes
        if &data[0..4] != INFORM_MAGIC {
            return Err(Error::InvalidMagic);
        }

        // Capture header for AAD (used in GCM mode)
        let mut header = [0u8; 40];
        header.copy_from_slice(&data[0..40]);

        // Parse header
        let version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        
        let mac = MacAddress::from_bytes(&data[8..14])
            .ok_or_else(|| Error::InvalidPacket("Invalid MAC in header".to_string()))?;
        
        let flags = InformFlags::from_raw(u16::from_be_bytes([data[14], data[15]]));
        
        let mut iv = [0u8; 16];
        iv.copy_from_slice(&data[16..32]);
        
        let payload_version = u32::from_be_bytes([data[32], data[33], data[34], data[35]]);
        let payload_length = u32::from_be_bytes([data[36], data[37], data[38], data[39]]) as usize;

        let payload_start = Self::HEADER_SIZE;
        let payload_end = payload_start + payload_length;

        if data.len() < payload_end {
            return Err(Error::PacketTooShort {
                expected: payload_end,
                actual: data.len(),
            });
        }

        let payload = data[payload_start..payload_end].to_vec();

        Ok(Self {
            version,
            mac,
            flags,
            iv,
            payload_version,
            payload,
            header,
        })
    }

    /// Encode the packet to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(Self::HEADER_SIZE + self.payload.len());

        // Magic
        result.extend_from_slice(INFORM_MAGIC);

        // Version
        result.extend_from_slice(&self.version.to_be_bytes());

        // MAC
        result.extend_from_slice(self.mac.as_bytes());

        // Flags
        result.extend_from_slice(&self.flags.to_raw().to_be_bytes());

        // IV
        result.extend_from_slice(&self.iv);

        // Payload version
        result.extend_from_slice(&self.payload_version.to_be_bytes());

        // Payload length
        result.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());

        // Payload
        result.extend_from_slice(&self.payload);

        result
    }

    /// Decrypt the payload using the given key
    pub fn decrypt(&self, key: &AesKey) -> Result<Vec<u8>> {
        let mut data = if self.flags.encrypted {
            if self.flags.aes_gcm {
                // GCM mode uses the 40-byte header as Additional Authenticated Data
                crypto::decrypt_gcm_aad(key, &self.iv, &self.payload, &self.header)?
            } else {
                crypto::decrypt_cbc(key, &self.iv, &self.payload)?
            }
        } else {
            self.payload.clone()
        };

        if self.flags.zlib_compressed {
            data = crypto::decompress_zlib(&data)?;
        }

        // Snappy compression is rare but might be used
        if self.flags.snappy_compressed {
            return Err(Error::Decompression("Snappy compression not implemented".to_string()));
        }

        Ok(data)
    }

    /// Decrypt and parse the payload as JSON
    pub fn decrypt_json(&self, key: &AesKey) -> Result<InformRequest> {
        let decrypted = self.decrypt(key)?;
        let json_str = String::from_utf8_lossy(&decrypted);
        tracing::trace!("Decrypted inform payload: {}", json_str);
        
        let request: InformRequest = serde_json::from_slice(&decrypted)?;
        Ok(request)
    }
}

/// Builder for creating inform response packets
pub struct InformResponseBuilder {
    mac: MacAddress,
    key: AesKey,
    use_gcm: bool,
    use_compression: bool,
}

impl InformResponseBuilder {
    /// Create a new response builder
    pub fn new(mac: MacAddress, key: AesKey) -> Self {
        Self {
            mac,
            key,
            use_gcm: true,
            use_compression: false, // Responses are typically small
        }
    }

    /// Enable or disable GCM mode
    pub fn use_gcm(mut self, use_gcm: bool) -> Self {
        self.use_gcm = use_gcm;
        self
    }

    /// Enable or disable compression
    pub fn use_compression(mut self, compress: bool) -> Self {
        self.use_compression = compress;
        self
    }

    /// Build an encrypted response packet
    pub fn build(&self, response: &InformResponse) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(response)?;
        
        let mut payload = json;
        
        // Compress if enabled
        if self.use_compression {
            payload = crypto::compress_zlib(&payload)?;
        }

        // Generate IV
        let iv = crypto::generate_iv();

        // Encrypt
        let encrypted = if self.use_gcm {
            crypto::encrypt_gcm(&self.key, &iv, &payload)?
        } else {
            crypto::encrypt_cbc(&self.key, &iv, &payload)?
        };

        // Build flags
        let flags = InformFlags {
            encrypted: true,
            zlib_compressed: self.use_compression,
            snappy_compressed: false,
            aes_gcm: self.use_gcm,
        };

        // Build packet (header will be generated during encode)
        let packet = InformPacket {
            version: 0,
            mac: self.mac,
            flags,
            iv,
            payload_version: 1,
            payload: encrypted,
            header: [0u8; 40], // Will be overwritten by encode()
        };

        Ok(packet.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inform_packet_decode_header() {
        // Minimal packet with just header and empty payload
        let mut data = vec![0u8; 40];
        data[0..4].copy_from_slice(INFORM_MAGIC);
        // Version
        data[4..8].copy_from_slice(&1u32.to_be_bytes());
        // MAC
        data[8..14].copy_from_slice(&[0x1c, 0x0b, 0x8b, 0x8e, 0x17, 0x7f]);
        // Flags (encrypted + gcm)
        data[14..16].copy_from_slice(&0x09u16.to_be_bytes());
        // IV (zeros for test)
        // Payload version
        data[32..36].copy_from_slice(&1u32.to_be_bytes());
        // Payload length (0)
        data[36..40].copy_from_slice(&0u32.to_be_bytes());

        let packet = InformPacket::decode(&data).unwrap();
        assert_eq!(packet.version, 1);
        assert_eq!(packet.mac.to_colon_string(), "1c:0b:8b:8e:17:7f");
        assert!(packet.flags.encrypted);
        assert!(packet.flags.aes_gcm);
        assert!(!packet.flags.zlib_compressed);
    }

    #[test]
    fn test_inform_packet_roundtrip() {
        let mac: MacAddress = "1c:0b:8b:8e:17:7f".parse().unwrap();
        let key = AesKey::default_key();
        
        let response = InformResponse::noop(10);
        let builder = InformResponseBuilder::new(mac, key.clone());
        
        let encoded = builder.build(&response).unwrap();
        let decoded = InformPacket::decode(&encoded).unwrap();
        
        assert_eq!(decoded.mac, mac);
        assert!(decoded.flags.encrypted);
        assert!(decoded.flags.aes_gcm);
        
        // Decrypt and verify
        let decrypted = decoded.decrypt(&key).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(json["_type"], "noop");
        assert_eq!(json["interval"], 10);
    }
}
