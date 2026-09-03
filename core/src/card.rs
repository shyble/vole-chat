//! An identity you can hand someone, small enough to scan.
//!
//! A key string (PROTOCOL.md §1) proves signatures but says nothing about **sealing**: the
//! X25519 and ML-KEM keys travel in discovery and rosters instead. So a member reached through
//! a hub is sealed to whatever keys that hub vouched for, and a hostile hub could substitute
//! its own and read everything it forwards. Signatures would still be unforgeable, but
//! confidentiality would be gone.
//!
//! A card closes that without carrying the keys. Post-quantum keys are big — ML-DSA-65 alone is
//! 1952 bytes, and all four together come to about 4.4 KB of base64, well past the 2953 bytes a
//! QR code can hold at all. So the card carries the Ed25519 key, which is small and fixes the
//! identity, plus a **fingerprint over the whole key set**. The bulk arrives over the wire as
//! usual and is checked against that fingerprint: the hub still supplies the keys, but can no
//! longer change them. About 160 characters, which is a comfortable QR to scan off a screen.

use crate::der::{decode_spki, KeyKind};
use crate::keys::{instance_id_from_ed25519_b64, parse_key_string, PeerKeys};
use crate::Identity;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Bound into the fingerprint so a card can never be read under different rules.
const FINGERPRINT_DOMAIN: &str = "volechat-card-v1";

/// Everything needed to reach someone securely, short enough for a QR code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactCard {
	/// Ed25519 public key, base64 SPKI. Fixes the identity and the instance id.
	#[serde(rename = "i")]
	pub ed25519: String,
	/// Display name, for showing before anything is verified.
	#[serde(rename = "n", default, skip_serializing_if = "String::is_empty")]
	pub name: String,
	/// SHA-256 over the full key set, base64. What the hub's copy has to match.
	#[serde(rename = "f")]
	pub fingerprint: String,
	/// A hub where this identity can be found, so a card alone is enough to start.
	#[serde(rename = "h", skip_serializing_if = "Option::is_none")]
	pub hub: Option<String>,
}

#[derive(Debug, Error)]
pub enum CardError {
	#[error("not a vole-chat code")]
	Format,
	#[error("bad identity: {0}")]
	Key(#[from] crate::keys::KeyError),
	#[error("that code is for a different identity")]
	WrongIdentity,
	#[error("the keys offered do not match the code — somebody is being passed off as them")]
	Mismatch,
}

/// The fingerprint a card commits to: every public key this identity uses, in a fixed order.
pub fn fingerprint(ed25519_b64: &str, mldsa_b64: &str, x_b64: &str, mlkem_b64: &str) -> String {
	let mut h = Sha256::new();
	h.update(FINGERPRINT_DOMAIN.as_bytes());
	for part in [ed25519_b64, mldsa_b64, x_b64, mlkem_b64] {
		h.update(b"|");
		h.update(part.as_bytes());
	}
	B64.encode(h.finalize())
}

impl ContactCard {
	/// This identity's own card.
	pub fn mine(me: &Identity, hub: Option<&str>) -> ContactCard {
		let peer = me.as_peer();
		let ed25519 = ed25519_b64_of(&peer.key_string).unwrap_or_default();
		ContactCard {
			fingerprint: fingerprint(
				&ed25519,
				&me.mldsa_public_b64(),
				&me.x25519_public_b64(),
				&me.mlkem_public_b64(),
			),
			ed25519,
			name: me.name.clone(),
			hub: hub.map(str::to_string),
		}
	}

	/// What goes in the QR: compact JSON, around 160 characters.
	pub fn encode(&self) -> String {
		serde_json::to_string(self).expect("card serializes")
	}

	/// Read a scanned card. A bare key string is accepted as an identity-only code — that is
	/// what earlier builds displayed — but it pins nothing beyond the identity.
	pub fn decode(text: &str) -> Result<ContactCard, CardError> {
		let text = text.trim();
		if text.starts_with('{') {
			let card: ContactCard = serde_json::from_str(text).map_err(|_| CardError::Format)?;
			if card.ed25519.is_empty() || card.fingerprint.is_empty() {
				return Err(CardError::Format);
			}
			// The identity key must be a real one, or the id it yields means nothing.
			let der = B64.decode(&card.ed25519).map_err(|_| CardError::Format)?;
			match decode_spki(&der) {
				Ok((KeyKind::Ed25519, _)) => {}
				_ => return Err(CardError::Format),
			}
			return Ok(card);
		}
		if text.starts_with("vole-ed25519 ") {
			let peer = parse_key_string(text)?;
			return Ok(ContactCard {
				ed25519: ed25519_b64_of(text).ok_or(CardError::Format)?,
				name: peer.name,
				fingerprint: String::new(),
				hub: None,
			});
		}
		Err(CardError::Format)
	}

	pub fn instance_id(&self) -> String {
		instance_id_from_ed25519_b64(&self.ed25519)
	}

	/// Whether this card commits to sealing keys, or only names an identity.
	pub fn seals(&self) -> bool {
		!self.fingerprint.is_empty()
	}

	/// Check what a hub offered for this identity against what the card committed to.
	///
	/// The key string carries the Ed25519 and ML-DSA keys; the sealing keys come separately.
	/// All four have to match, or somebody is being passed off as them.
	pub fn verify(&self, key_string: &str, x_b64: Option<&str>, mlkem_b64: Option<&str>) -> Result<PeerKeys, CardError> {
		let peer = parse_key_string(key_string)?;
		let ed = ed25519_b64_of(key_string).ok_or(CardError::Format)?;
		if ed != self.ed25519 {
			return Err(CardError::WrongIdentity);
		}
		if !self.seals() {
			return Ok(peer); // identity-only code: nothing more was promised
		}
		let mldsa = key_string.split(' ').nth(3).unwrap_or("");
		let expected = fingerprint(&ed, mldsa, x_b64.unwrap_or(""), mlkem_b64.unwrap_or(""));
		if expected != self.fingerprint {
			return Err(CardError::Mismatch);
		}
		Ok(peer)
	}
}

/// The Ed25519 token out of a key string.
fn ed25519_b64_of(key_string: &str) -> Option<String> {
	let mut parts = key_string.trim().split(' ');
	if parts.next()? != "vole-ed25519" {
		return None;
	}
	Some(parts.next()?.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn keys_of(me: &Identity) -> (String, String, String) {
		(me.key_string(), me.x25519_public_b64(), me.mlkem_public_b64())
	}

	#[test]
	fn a_card_is_small_enough_to_scan_and_names_its_identity() {
		let me = Identity::generate("alice");
		let card = ContactCard::mine(&me, Some("https://club.openvole.com/mesh"));
		let text = card.encode();
		assert!(text.len() < 400, "a QR has to hold this: {} chars", text.len());
		let back = ContactCard::decode(&text).unwrap();
		assert_eq!(back, card);
		assert_eq!(back.instance_id(), me.instance_id());
		assert_eq!(back.name, "alice");
		assert_eq!(back.hub.as_deref(), Some("https://club.openvole.com/mesh"));
		assert!(back.seals());
	}

	#[test]
	fn the_real_keys_verify_and_substituted_ones_do_not() {
		let me = Identity::generate("alice");
		let impostor = Identity::generate("impostor");
		let card = ContactCard::mine(&me, None);
		let (key_string, x, mlkem) = keys_of(&me);

		let peer = card.verify(&key_string, Some(&x), Some(&mlkem)).expect("her own keys verify");
		assert_eq!(peer.instance_id, me.instance_id());

		// A hub swapping the sealing keys is exactly what the fingerprint is for.
		assert!(matches!(
			card.verify(&key_string, Some(&impostor.x25519_public_b64()), Some(&mlkem)),
			Err(CardError::Mismatch)
		));
		assert!(matches!(
			card.verify(&key_string, Some(&x), Some(&impostor.mlkem_public_b64())),
			Err(CardError::Mismatch)
		));
		assert!(matches!(card.verify(&key_string, None, None), Err(CardError::Mismatch)));
		// And a different identity entirely is refused before any of that.
		assert!(matches!(
			card.verify(&impostor.key_string(), Some(&impostor.x25519_public_b64()), None),
			Err(CardError::WrongIdentity)
		));
	}

	#[test]
	fn an_identity_only_code_still_reads_but_promises_less() {
		let me = Identity::generate("bob");
		let card = ContactCard::decode(&me.key_string()).unwrap();
		assert_eq!(card.instance_id(), me.instance_id());
		assert!(!card.seals());
		// It pins the identity, so a different one is still refused …
		let other = Identity::generate("other");
		assert!(matches!(card.verify(&other.key_string(), None, None), Err(CardError::WrongIdentity)));
		// … but it made no promise about sealing keys, so any of hers pass.
		assert!(card.verify(&me.key_string(), Some(&other.x25519_public_b64()), None).is_ok());
	}

	#[test]
	fn junk_is_refused() {
		assert!(ContactCard::decode("hello").is_err());
		assert!(ContactCard::decode("{}").is_err());
		assert!(ContactCard::decode(r#"{"i":"not-base64","f":"x"}"#).is_err());
	}
}
