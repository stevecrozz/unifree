//! Cryptographic operations for UniFi protocol
//!
//! Supports AES-128-GCM (preferred) and AES-128-CBC encryption,
//! along with ZLIB compression.

use aes_gcm::{
    aead::{Aead, KeyInit, consts::U12},
    aes::Aes128,
    AesGcm, Nonce,
};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use std::io::{Read, Write};

use crate::{Error, Result, DEFAULT_KEY};

type Aes128Gcm = AesGcm<Aes128, U12>;

/// AES key (16 bytes / 128 bits)
#[derive(Clone)]
pub struct AesKey([u8; 16]);

impl AesKey {
    /// Create key from hex string
    pub fn from_hex(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str)?;
        Self::from_bytes(&bytes)
    }

    /// Create key from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 16 {
            return Err(Error::InvalidKeyLength {
                expected: 16,
                actual: bytes.len(),
            });
        }
        let mut key = [0u8; 16];
        key.copy_from_slice(bytes);
        Ok(Self(key))
    }

    /// Get the default key used before adoption
    pub fn default_key() -> Self {
        Self::from_hex(DEFAULT_KEY).expect("Default key is valid")
    }

    /// Get key bytes
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl std::fmt::Debug for AesKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AesKey([REDACTED])")
    }
}

/// Encrypt data using AES-128-GCM
pub fn encrypt_gcm(key: &AesKey, iv: &[u8; 16], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes128Gcm::new(key.as_bytes().into());
    // GCM uses 12-byte nonce, but UniFi protocol uses 16-byte IV
    // We truncate to 12 bytes as per the protocol behavior
    let nonce = Nonce::from_slice(&iv[..12]);
    
    cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| Error::Encryption(e.to_string()))
}

/// Decrypt data using AES-128-GCM (without AAD - simple mode)
pub fn decrypt_gcm(key: &AesKey, iv: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>> {
    decrypt_gcm_aad(key, iv, ciphertext, &[])
}

/// Decrypt data using AES-128-GCM with Additional Authenticated Data
/// 
/// UniFi devices use 16-byte IVs. OpenSSL handles this correctly by
/// using GHASH preprocessing for non-96-bit IVs.
pub fn decrypt_gcm_aad(key: &AesKey, iv: &[u8; 16], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    use openssl::symm::{Cipher, Crypter, Mode};
    
    // GCM tag is last 16 bytes of ciphertext
    if ciphertext.len() < 16 {
        return Err(Error::Decryption("Ciphertext too short for GCM tag".to_string()));
    }
    
    let (ct, tag) = ciphertext.split_at(ciphertext.len() - 16);
    
    let cipher = Cipher::aes_128_gcm();
    let mut crypter = Crypter::new(cipher, Mode::Decrypt, key.as_bytes(), Some(iv))
        .map_err(|e| Error::Decryption(e.to_string()))?;
    
    // Set AAD before decryption
    if !aad.is_empty() {
        crypter.aad_update(aad)
            .map_err(|e| Error::Decryption(e.to_string()))?;
    }
    
    // Set the expected tag
    crypter.set_tag(tag)
        .map_err(|e| Error::Decryption(e.to_string()))?;
    
    // Decrypt
    let mut plaintext = vec![0u8; ct.len() + cipher.block_size()];
    let mut count = crypter.update(ct, &mut plaintext)
        .map_err(|e| Error::Decryption(e.to_string()))?;
    count += crypter.finalize(&mut plaintext[count..])
        .map_err(|e| Error::Decryption(e.to_string()))?;
    
    plaintext.truncate(count);
    Ok(plaintext)
}

/// Encrypt data using AES-128-CBC (legacy, for older firmware)
pub fn encrypt_cbc(key: &AesKey, iv: &[u8; 16], plaintext: &[u8]) -> Result<Vec<u8>> {
    use aes::Aes128;
    use cbc::{cipher::BlockEncryptMut, cipher::KeyIvInit, Encryptor};

    type Aes128CbcEnc = Encryptor<Aes128>;

    // PKCS7 padding
    let block_size = 16;
    let padding_len = block_size - (plaintext.len() % block_size);
    let mut buffer = plaintext.to_vec();
    buffer.extend(std::iter::repeat(padding_len as u8).take(padding_len));

    let buf_len = buffer.len();
    let encryptor = Aes128CbcEnc::new(key.as_bytes().into(), iv.into());
    encryptor
        .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buffer, buf_len)
        .map_err(|e| Error::Encryption(e.to_string()))?;
    
    Ok(buffer)
}

/// Decrypt data using AES-128-CBC (legacy, for older firmware)
pub fn decrypt_cbc(key: &AesKey, iv: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use aes::Aes128;
    use cbc::{cipher::BlockDecryptMut, cipher::KeyIvInit, Decryptor};

    type Aes128CbcDec = Decryptor<Aes128>;

    let mut buffer = ciphertext.to_vec();
    let decryptor = Aes128CbcDec::new(key.as_bytes().into(), iv.into());
    
    let decrypted = decryptor
        .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buffer)
        .map_err(|e| Error::Decryption(e.to_string()))?;
    
    Ok(decrypted.to_vec())
}

/// Compress data using ZLIB
pub fn compress_zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| Error::Compression(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| Error::Compression(e.to_string()))
}

/// Decompress data using ZLIB
pub fn decompress_zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| Error::Decompression(e.to_string()))?;
    Ok(decompressed)
}

/// Generate a random IV (16 bytes)
pub fn generate_iv() -> [u8; 16] {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    // Simple IV generation - in production you might want to use a proper RNG
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    let mut iv = [0u8; 16];
    iv[..8].copy_from_slice(&timestamp.to_le_bytes()[..8]);
    
    // Add some pseudo-randomness
    let random_part = std::process::id() as u64 ^ timestamp as u64;
    iv[8..16].copy_from_slice(&random_part.to_le_bytes());
    
    iv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_from_hex() {
        let key = AesKey::from_hex(DEFAULT_KEY).unwrap();
        assert_eq!(key.as_bytes().len(), 16);
    }

    #[test]
    fn test_gcm_roundtrip() {
        let key = AesKey::default_key();
        let iv = generate_iv();
        let plaintext = b"Hello, UniFi!";
        
        let ciphertext = encrypt_gcm(&key, &iv, plaintext).unwrap();
        let decrypted = decrypt_gcm(&key, &iv, &ciphertext).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_cbc_roundtrip() {
        let key = AesKey::default_key();
        let iv = generate_iv();
        let plaintext = b"Hello, UniFi!";
        
        let ciphertext = encrypt_cbc(&key, &iv, plaintext).unwrap();
        let decrypted = decrypt_cbc(&key, &iv, &ciphertext).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_zlib_roundtrip() {
        let data = b"This is some test data that should compress well well well well";
        
        let compressed = compress_zlib(data).unwrap();
        let decompressed = decompress_zlib(&compressed).unwrap();
        
        assert_eq!(decompressed, data);
        assert!(compressed.len() < data.len()); // Should actually compress
    }
}
