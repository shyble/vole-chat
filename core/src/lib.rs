//! VoleNet protocol core for vole-chat.
//!
//! Everything a chat client needs to be a first-class member of a VoleNet hub, implemented to
//! interoperate byte-for-byte with OpenVole core (see PROTOCOL.md at the repo root — section
//! numbers in this crate refer to it). No UI, no storage: those are the app's.
//!
//! Layering, lowest first:
//! - [`der`]       SPKI DER wrapping of raw public keys (§1)
//! - [`keys`]      identity: the four keypairs, key strings, instance ids (§1)
//! - [`canonical`] the JSON canonicalization every signature covers (§2)
//! - [`message`]   signed messages: create and verify (§2)
//! - [`seal`]      hybrid X25519 + ML-KEM-768 sealed envelopes (§3)

pub mod canonical;
pub mod der;
pub mod keys;
pub mod message;
pub mod seal;

pub use keys::{Identity, PeerKeys};
pub use message::Message;
pub use seal::SealedBox;

/// The protocol version every message carries and every signature binds.
pub const VOLENET_VERSION: u32 = 2;
