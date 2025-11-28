//! UniFi Protocol Implementation
//!
//! This crate implements the UniFi device communication protocol including:
//! - Discovery packets (UDP broadcast)
//! - Inform packets (HTTP POST with encryption)
//! - Configuration generation (INI format)

pub mod crypto;
pub mod discovery;
pub mod error;
pub mod inform;
pub mod types;

pub use error::{Error, Result};
pub use types::*;

/// Default encryption key used before device adoption
pub const DEFAULT_KEY: &str = "ba86f2bbe107c7c57eb5f2690775c712";

/// Protocol magic bytes for inform packets
pub const INFORM_MAGIC: &[u8; 4] = b"TNBU";

/// Discovery packet type for device announcements
pub const DISCOVERY_TYPE_BROADCAST: u8 = 0x06;
