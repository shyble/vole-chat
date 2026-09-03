//! Hybrid X25519 + ML-KEM-768 sealed envelopes (PROTOCOL.md §3).
//!
//! `key = HKDF-SHA256(ssX ‖ ssPq, salt ∅, info "<scheme>|<aad>")`, ChaCha20-Poly1305 with the
//! aad bound in. The scheme is decided by whether the box carries a KEM ciphertext, so an
//! envelope with the KEM stripped derives a different key and fails the tag rather than quietly
//! decrypting under the weaker one.

use crate::der::{encode_spki, KeyKind};
use crate::keys::{
	mlkem_decapsulate, mlkem_encapsulate, parse_mlkem_public_b64, parse_x25519_public_b64, Identity, KeyError,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::{aead::Aead, aead::Payload, ChaCha20Poly1305, KeyInit, Nonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use x25519_dalek::{EphemeralSecret, PublicKey as XPublic};

const SCHEME_X25519: &str = "volenet-seal-v1";
const SCHEME_HYBRID: &str = "volenet-seal-v2";
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SealedBox {
	/// Ephemeral X25519 public key, base64 SPKI DER.
	pub epk: String,
	/// ML-KEM-768 ciphertext, base64 — hybrid envelopes only.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub kem: Option<String>,
	/// 12-byte nonce, base64.
	pub n: String,
	/// Ciphertext ‖ 16-byte tag, base64.
	pub c: String,
}

#[derive(Debug, Error)]
pub enum SealError {
	#[error("bad recipient key: {0}")]
	Key(#[from] KeyError),
	#[error("bad envelope")]
	Envelope,
	#[error("this is a hybrid envelope and I hold no ML-KEM key")]
	NoMlKem,
	#[error("authentication failed")]
	Auth,
}

fn derive_key(ikm: &[u8], scheme: &str, aad: &str) -> [u8; 32] {
	let hk = Hkdf::<Sha256>::new(None, ikm); // empty salt ≡ HashLen zeros (RFC 5869), as Node does
	let mut key = [0u8; 32];
	hk.expand(format!("{scheme}|{aad}").as_bytes(), &mut key).expect("32 bytes is a valid HKDF length");
	key
}

/// Seal `plaintext` to a recipient. Hybrid when their ML-KEM key is given; X25519-only otherwise.
pub fn seal(
	recipient_x_b64: &str,
	recipient_mlkem_b64: Option<&str>,
	plaintext: &[u8],
	aad: &str,
) -> Result<SealedBox, SealError> {
	let recipient = parse_x25519_public_b64(recipient_x_b64)?;
	let eph = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
	let epk = XPublic::from(&eph);
	let ss_x = eph.diffie_hellman(&recipient);

	let mut ikm = ss_x.as_bytes().to_vec();
	let mut scheme = SCHEME_X25519;
	let mut kem = None;
	if let Some(mlkem_b64) = recipient_mlkem_b64 {
		let ek = parse_mlkem_public_b64(mlkem_b64)?;
		let (ct, ss_pq) = mlkem_encapsulate(&ek);
		ikm.extend_from_slice(&ss_pq);
		scheme = SCHEME_HYBRID;
		kem = Some(B64.encode(ct));
	}

	let key = derive_key(&ikm, scheme, aad);
	let mut nonce = [0u8; NONCE_BYTES];
	rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
	let cipher = ChaCha20Poly1305::new((&key).into());
	let ct = cipher
		.encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: aad.as_bytes() })
		.map_err(|_| SealError::Auth)?;
	Ok(SealedBox {
		epk: B64.encode(encode_spki(KeyKind::X25519, epk.as_bytes()).expect("32-byte key")),
		kem,
		n: B64.encode(nonce),
		c: B64.encode(ct),
	})
}

/// Open an envelope addressed to `me`. Fails closed on any mismatch or tampering.
pub fn unseal(me: &Identity, sealed: &SealedBox, aad: &str) -> Result<Vec<u8>, SealError> {
	let epk = parse_x25519_public_b64(&sealed.epk)?;
	let ss_x = me.x25519_secret().diffie_hellman(&epk);

	let mut ikm = ss_x.as_bytes().to_vec();
	let mut scheme = SCHEME_X25519;
	if let Some(kem_b64) = &sealed.kem {
		let ct = B64.decode(kem_b64).map_err(|_| SealError::Envelope)?;
		let ss_pq = mlkem_decapsulate(me.mlkem_decapsulation_key(), &ct)?;
		ikm.extend_from_slice(&ss_pq);
		scheme = SCHEME_HYBRID;
	}

	let key = derive_key(&ikm, scheme, aad);
	let nonce = B64.decode(&sealed.n).map_err(|_| SealError::Envelope)?;
	let ct = B64.decode(&sealed.c).map_err(|_| SealError::Envelope)?;
	if nonce.len() != NONCE_BYTES || ct.len() <= TAG_BYTES {
		return Err(SealError::Envelope);
	}
	let cipher = ChaCha20Poly1305::new((&key).into());
	cipher
		.decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: aad.as_bytes() })
		.map_err(|_| SealError::Auth)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn hybrid_round_trip_and_aad_binding() {
		let alice = Identity::generate("alice");
		let bob = Identity::generate("bob");
		let aad = format!("{}|{}", alice.instance_id(), bob.instance_id());
		let sealed = seal(&bob.x25519_public_b64(), Some(&bob.mlkem_public_b64()), b"hi bob", &aad).unwrap();
		assert!(sealed.kem.is_some());
		assert_eq!(unseal(&bob, &sealed, &aad).unwrap(), b"hi bob");
		// Re-addressed by a hostile relay: the recipient recomputes a different aad.
		let wrong = format!("{}|{}", "someone-else", bob.instance_id());
		assert!(matches!(unseal(&bob, &sealed, &wrong), Err(SealError::Auth)));
		// Stripping the KEM must not downgrade to a working X25519-only key.
		let mut stripped = sealed.clone();
		stripped.kem = None;
		assert!(matches!(unseal(&bob, &stripped, &aad), Err(SealError::Auth)));
		// Only bob can open it.
		assert!(unseal(&alice, &sealed, &aad).is_err());
	}

	#[test]
	fn x25519_only_round_trip() {
		let bob = Identity::generate("bob");
		let sealed = seal(&bob.x25519_public_b64(), None, b"legacy", "a|b").unwrap();
		assert!(sealed.kem.is_none());
		assert_eq!(unseal(&bob, &sealed, "a|b").unwrap(), b"legacy");
	}
}
