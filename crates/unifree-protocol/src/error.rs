//! Error types for the protocol crate

use thiserror::Error;

/// Protocol error type
#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid MAC address: {0}")]
    InvalidMacAddress(String),

    #[error("Invalid packet: {0}")]
    InvalidPacket(String),

    #[error("Invalid magic bytes")]
    InvalidMagic,

    #[error("Packet too short: expected at least {expected} bytes, got {actual}")]
    PacketTooShort { expected: usize, actual: usize },

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Decryption error: {0}")]
    Decryption(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Decompression error: {0}")]
    Decompression(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),

    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for protocol operations
pub type Result<T> = std::result::Result<T, Error>;
