//! An identity you can hand someone, small enough to scan.
//!
//! A key string (PROTOCOL.md §1) proves signatures but says nothing about **sealing**: the
//! X25519 and ML-KEM keys travel in discovery and rosters instead. So a member reached through
//! a hub is sealed to whatever keys that hub vouched for, and a hostile hub could substitute
//! its own and read everything it forwards. Signatures would still be unforgeable, but
//! confidentiality would be gone.
//!
//! A card closes that. It cannot carry everything — all four keys come to about 4.4 KB of
//! base64, past the 2953 bytes a QR holds at all, and ML-DSA-65 alone is 1952 bytes. So it
//! carries what is needed to *reach* someone: the Ed25519 key that fixes their identity, and
//! the two **sealing** keys, about 1.7 KB together. The large ML-DSA signing key stays on the
//! wire, bound by a **fingerprint over the whole set** so the hub cannot alter it either.
//!
//! Carrying the sealing keys is what lets you write to someone you have scanned but never seen
//! online. A hub forgets a member 45 s after their screen locks, and with only a fingerprint
//! there would be nothing to seal to — which is exactly when a message most needs to be held.

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
	/// X25519 public key, base64 SPKI. Sealing goes here, with no hub involved.
	#[serde(rename = "x", default, skip_serializing_if = "String::is_empty")]
	pub x_public: String,
	/// ML-KEM-768 public key, base64 SPKI — the post-quantum half of the seal.
	#[serde(rename = "m", default, skip_serializing_if = "String::is_empty")]
	pub mlkem_public: String,
	/// SHA-256 over the full key set, base64. What the hub's copy of the rest has to match.
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
		let x_public = me.x25519_public_b64();
		let mlkem_public = me.mlkem_public_b64();
		ContactCard {
			fingerprint: fingerprint(&ed25519, &me.mldsa_public_b64(), &x_public, &mlkem_public),
			ed25519,
			name: me.name.clone(),
			x_public,
			mlkem_public,
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
				x_public: String::new(),
				mlkem_public: String::new(),
				fingerprint: String::new(),
				hub: None,
			});
		}
		Err(CardError::Format)
	}

	pub fn instance_id(&self) -> String {
		instance_id_from_ed25519_b64(&self.ed25519)
	}

	/// Whether this card carries sealing keys, or only names an identity.
	pub fn seals(&self) -> bool {
		!self.x_public.is_empty() && !self.fingerprint.is_empty()
	}

	/// The identity alone, for verifying a first message from someone no hub has described yet.
	/// Ed25519 only: the ML-DSA key is too big for a card, so hybrid verification starts once a
	/// roster supplies it and the fingerprint has checked out.
	pub fn identity_peer(&self) -> Result<PeerKeys, CardError> {
		let der = B64.decode(&self.ed25519).map_err(|_| CardError::Format)?;
		let (kind, raw) = decode_spki(&der).map_err(|_| CardError::Format)?;
		if kind != KeyKind::Ed25519 {
			return Err(CardError::Format);
		}
		let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| CardError::Format)?;
		let ed = ed25519_dalek::VerifyingKey::from_bytes(&arr).map_err(|_| CardError::Format)?;
		Ok(PeerKeys {
			instance_id: self.instance_id(),
			name: self.name.clone(),
			ed25519: ed,
			mldsa: None,
			key_string: String::new(),
		})
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

	fn back_seals(c: &ContactCard) -> bool {
		c.seals()
	}

	#[test]
	fn a_card_is_small_enough_to_scan_and_names_its_identity() {
		let me = Identity::generate("alice");
		let card = ContactCard::mine(&me, Some("https://club.openvole.com/mesh"));
		let text = card.encode();
		assert!(text.len() < 2953, "a QR has to hold this: {} chars", text.len());
		assert!(back_seals(&card));
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
