//! Identity: the four keypairs, key strings, instance ids (PROTOCOL.md §1).
//!
//! Persistence is by **seeds**, never by PKCS8: every keypair here regenerates
//! deterministically from seed bytes, which is what a phone backs up as a QR. Only public keys
//! are ever exported, and only in the exact SPKI form the hub expects.

use crate::der::{decode_spki, encode_spki, KeyKind};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signer as _, SigningKey, Verifier as _, VerifyingKey};
use ml_dsa::{Keypair as _, MlDsa65};
use ml_kem::{Decapsulate as _, KeyExport as _, MlKem768};
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{PublicKey as XPublic, StaticSecret};

pub type MlDsaSigningKey = ml_dsa::SigningKey<MlDsa65>;
pub type MlDsaVerifyingKey = ml_dsa::VerifyingKey<MlDsa65>;
pub type MlKemDecapsulationKey = ml_kem::DecapsulationKey<MlKem768>;
pub type MlKemEncapsulationKey = ml_kem::EncapsulationKey<MlKem768>;

#[derive(Debug, Error)]
pub enum KeyError {
	#[error("not a vole key string")]
	Format,
	#[error("bad base64")]
	Base64,
	#[error("bad key: {0}")]
	Der(#[from] crate::der::DerError),
	#[error("invalid Ed25519 key")]
	Ed25519,
	#[error("invalid ML-DSA key")]
	MlDsa,
	#[error("invalid ML-KEM key or ciphertext")]
	MlKem,
	#[error("seed material has the wrong length")]
	Seeds,
}

/// The instance id of a node: 16 hex chars of `sha256(base64 SPKI text)`.
pub fn instance_id_from_ed25519_b64(spki_b64: &str) -> String {
	let digest = Sha256::digest(spki_b64.as_bytes());
	digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// A peer as learned from a key string (§1): what you need to verify its signatures.
#[derive(Clone)]
pub struct PeerKeys {
	pub instance_id: String,
	pub name: String,
	pub ed25519: VerifyingKey,
	pub mldsa: Option<MlDsaVerifyingKey>,
	/// The key string exactly as received, so it can be pinned and re-emitted unchanged.
	pub key_string: String,
}

impl std::fmt::Debug for PeerKeys {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("PeerKeys")
			.field("instance_id", &self.instance_id)
			.field("name", &self.name)
			.field("pq", &self.mldsa.is_some())
			.finish()
	}
}

/// Parse `vole-ed25519 <b64 spki> <name> [<b64 spki ml-dsa>]`.
pub fn parse_key_string(s: &str) -> Result<PeerKeys, KeyError> {
	let parts: Vec<&str> = s.trim().split(' ').collect();
	if parts.first() != Some(&"vole-ed25519") || parts.len() < 2 {
		return Err(KeyError::Format);
	}
	let ed_der = B64.decode(parts[1]).map_err(|_| KeyError::Base64)?;
	let (kind, raw) = decode_spki(&ed_der)?;
	if kind != KeyKind::Ed25519 {
		return Err(KeyError::Ed25519);
	}
	let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| KeyError::Ed25519)?;
	let ed25519 = VerifyingKey::from_bytes(&arr).map_err(|_| KeyError::Ed25519)?;
	let instance_id = instance_id_from_ed25519_b64(parts[1]);
	let name = parts.get(2).map(|n| n.to_string()).unwrap_or_else(|| instance_id[..8].to_string());
	// Optional 4th token: ML-DSA-65 public key. Ignored when unparsable, as core does.
	let mldsa = parts.get(3).and_then(|b64| parse_mldsa_public_b64(b64).ok());
	Ok(PeerKeys { instance_id, name, ed25519, mldsa, key_string: s.trim().to_string() })
}

pub fn parse_mldsa_public_b64(b64: &str) -> Result<MlDsaVerifyingKey, KeyError> {
	let der = B64.decode(b64).map_err(|_| KeyError::Base64)?;
	let (kind, raw) = decode_spki(&der)?;
	if kind != KeyKind::MlDsa65 {
		return Err(KeyError::MlDsa);
	}
	let enc = ml_dsa::EncodedVerifyingKey::<MlDsa65>::try_from(raw.as_slice()).map_err(|_| KeyError::MlDsa)?;
	Ok(MlDsaVerifyingKey::decode(&enc))
}

pub fn parse_x25519_public_b64(b64: &str) -> Result<XPublic, KeyError> {
	let der = B64.decode(b64).map_err(|_| KeyError::Base64)?;
	let (kind, raw) = decode_spki(&der)?;
	if kind != KeyKind::X25519 {
		return Err(KeyError::Format);
	}
	let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| KeyError::Format)?;
	Ok(XPublic::from(arr))
}

pub fn parse_mlkem_public_b64(b64: &str) -> Result<MlKemEncapsulationKey, KeyError> {
	let der = B64.decode(b64).map_err(|_| KeyError::Base64)?;
	let (kind, raw) = decode_spki(&der)?;
	if kind != KeyKind::MlKem768 {
		return Err(KeyError::MlKem);
	}
	let key = ml_kem::Key::<MlKemEncapsulationKey>::try_from(raw.as_slice()).map_err(|_| KeyError::MlKem)?;
	MlKemEncapsulationKey::new(&key).map_err(|_| KeyError::MlKem)
}

/// Everything that regenerates an identity. Back this up; nothing else is needed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Seeds {
	pub ed25519: [u8; 32],
	pub x25519: [u8; 32],
	pub mldsa: [u8; 32],
	/// ML-KEM-768 seed `d ‖ z`.
	pub mlkem: [u8; 64],
}

impl Seeds {
	pub fn random() -> Seeds {
		use rand::RngCore;
		let mut rng = rand::rngs::OsRng;
		let mut s = Seeds { ed25519: [0; 32], x25519: [0; 32], mldsa: [0; 32], mlkem: [0; 64] };
		rng.fill_bytes(&mut s.ed25519);
		rng.fill_bytes(&mut s.x25519);
		rng.fill_bytes(&mut s.mldsa);
		rng.fill_bytes(&mut s.mlkem);
		s
	}

	/// 160 bytes: ed25519 ‖ x25519 ‖ mldsa ‖ mlkem(d‖z).
	pub fn to_bytes(&self) -> [u8; 160] {
		let mut out = [0u8; 160];
		out[0..32].copy_from_slice(&self.ed25519);
		out[32..64].copy_from_slice(&self.x25519);
		out[64..96].copy_from_slice(&self.mldsa);
		out[96..160].copy_from_slice(&self.mlkem);
		out
	}

	pub fn from_bytes(b: &[u8]) -> Result<Seeds, KeyError> {
		if b.len() != 160 {
			return Err(KeyError::Seeds);
		}
		Ok(Seeds {
			ed25519: b[0..32].try_into().unwrap(),
			x25519: b[32..64].try_into().unwrap(),
			mldsa: b[64..96].try_into().unwrap(),
			mlkem: b[96..160].try_into().unwrap(),
		})
	}
}

/// A node's own identity: signs, agrees, and knows how to present itself.
pub struct Identity {
	pub name: String,
	seeds: Seeds,
	ed: SigningKey,
	mldsa: MlDsaSigningKey,
	x: StaticSecret,
	mlkem_dk: MlKemDecapsulationKey,
	// cached wire forms
	ed_spki_b64: String,
	instance_id: String,
}

impl Identity {
	pub fn generate(name: &str) -> Identity {
		Identity::from_seeds(name, Seeds::random())
	}

	pub fn from_seeds(name: &str, seeds: Seeds) -> Identity {
		let ed = SigningKey::from_bytes(&seeds.ed25519);
		let mldsa = MlDsaSigningKey::from_seed(&ml_dsa::Seed::try_from(&seeds.mldsa[..]).expect("32 bytes"));
		let x = StaticSecret::from(seeds.x25519);
		let mlkem_dk = MlKemDecapsulationKey::from_seed(ml_kem::Seed::try_from(&seeds.mlkem[..]).expect("64 bytes"));
		let ed_spki = encode_spki(KeyKind::Ed25519, ed.verifying_key().as_bytes()).expect("32-byte key");
		let ed_spki_b64 = B64.encode(ed_spki);
		let instance_id = instance_id_from_ed25519_b64(&ed_spki_b64);
		Identity { name: name.to_string(), seeds, ed, mldsa, x, mlkem_dk, ed_spki_b64, instance_id }
	}

	pub fn seeds(&self) -> &Seeds {
		&self.seeds
	}

	pub fn instance_id(&self) -> &str {
		&self.instance_id
	}

	/// `vole-ed25519 <b64> <name> <b64 ml-dsa>` — always hybrid.
	pub fn key_string(&self) -> String {
		format!("vole-ed25519 {} {} {}", self.ed_spki_b64, self.name, self.mldsa_public_b64())
	}

	pub fn mldsa_public_b64(&self) -> String {
		let raw = self.mldsa.verifying_key().encode();
		B64.encode(encode_spki(KeyKind::MlDsa65, raw.as_slice()).expect("1952-byte key"))
	}

	pub fn x25519_public_b64(&self) -> String {
		let pk = XPublic::from(&self.x);
		B64.encode(encode_spki(KeyKind::X25519, pk.as_bytes()).expect("32-byte key"))
	}

	pub fn mlkem_public_b64(&self) -> String {
		let raw = self.mlkem_dk.encapsulation_key().to_bytes();
		B64.encode(encode_spki(KeyKind::MlKem768, raw.as_slice()).expect("1184-byte key"))
	}

	/// How this identity looks to a peer that pinned its key string.
	pub fn as_peer(&self) -> PeerKeys {
		parse_key_string(&self.key_string()).expect("own key string parses")
	}

	/// Ed25519 signature, base64.
	pub fn sign_ed25519(&self, data: &[u8]) -> String {
		B64.encode(self.ed.sign(data).to_bytes())
	}

	/// ML-DSA-65 signature (deterministic, pure mode, empty context — what OpenSSL verifies by
	/// default), base64.
	pub fn sign_mldsa(&self, data: &[u8]) -> String {
		use ml_dsa::Signer as _;
		let sig: ml_dsa::Signature<MlDsa65> = self.mldsa.sign(data);
		B64.encode(sig.encode().as_slice())
	}

	pub fn x25519_secret(&self) -> &StaticSecret {
		&self.x
	}

	pub fn mlkem_decapsulation_key(&self) -> &MlKemDecapsulationKey {
		&self.mlkem_dk
	}
}

impl std::fmt::Debug for Identity {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Identity").field("name", &self.name).field("instance_id", &self.instance_id).finish()
	}
}

/// Verify an Ed25519 signature (base64) over `data`.
pub fn verify_ed25519(key: &VerifyingKey, data: &[u8], sig_b64: &str) -> bool {
	let Ok(bytes) = B64.decode(sig_b64) else { return false };
	let Ok(sig) = ed25519_dalek::Signature::from_slice(&bytes) else { return false };
	key.verify(data, &sig).is_ok()
}

/// Verify an ML-DSA-65 signature (base64) over `data`.
pub fn verify_mldsa(key: &MlDsaVerifyingKey, data: &[u8], sig_b64: &str) -> bool {
	use ml_dsa::Verifier as _;
	let Ok(bytes) = B64.decode(sig_b64) else { return false };
	let Ok(sig) = ml_dsa::Signature::<MlDsa65>::try_from(bytes.as_slice()) else { return false };
	key.verify(data, &sig).is_ok()
}

/// (ciphertext, shared secret) from an ML-KEM-768 encapsulation (used by `seal`). The
/// randomness comes from the OS here, so the KEM never needs an RNG of its own generation.
pub fn mlkem_encapsulate(ek: &MlKemEncapsulationKey) -> (Vec<u8>, Vec<u8>) {
	use rand::RngCore;
	let mut m = [0u8; 32];
	rand::rngs::OsRng.fill_bytes(&mut m);
	let (ct, ss) = ek.encapsulate_deterministic(&ml_kem::B32::try_from(&m[..]).expect("32 bytes"));
	(ct.as_slice().to_vec(), ss.as_slice().to_vec())
}

/// Shared secret from an ML-KEM-768 decapsulation (used by `seal`).
pub fn mlkem_decapsulate(dk: &MlKemDecapsulationKey, ct: &[u8]) -> Result<Vec<u8>, KeyError> {
	let ss = dk.decapsulate_slice(ct).map_err(|_| KeyError::MlKem)?;
	Ok(ss.as_slice().to_vec())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn identity_regenerates_from_seeds_and_parses_its_own_key_string() {
		let a = Identity::generate("phone");
		let b = Identity::from_seeds("phone", a.seeds().clone());
		assert_eq!(a.key_string(), b.key_string());
		assert_eq!(a.instance_id(), b.instance_id());
		assert_eq!(a.mlkem_public_b64(), b.mlkem_public_b64());
		let peer = parse_key_string(&a.key_string()).unwrap();
		assert_eq!(peer.instance_id, a.instance_id());
		assert_eq!(peer.name, "phone");
		assert!(peer.mldsa.is_some());
	}

	#[test]
	fn signatures_verify_and_tampering_fails() {
		let id = Identity::generate("n");
		let peer = id.as_peer();
		let sig = id.sign_ed25519(b"hello");
		let pq = id.sign_mldsa(b"hello");
		assert!(verify_ed25519(&peer.ed25519, b"hello", &sig));
		assert!(verify_mldsa(peer.mldsa.as_ref().unwrap(), b"hello", &pq));
		assert!(!verify_ed25519(&peer.ed25519, b"hellp", &sig));
		assert!(!verify_mldsa(peer.mldsa.as_ref().unwrap(), b"hellp", &pq));
	}

	#[test]
	fn kem_round_trip() {
		let id = Identity::generate("k");
		let ek = parse_mlkem_public_b64(&id.mlkem_public_b64()).unwrap();
		let (ct, ss) = mlkem_encapsulate(&ek);
		assert_eq!(ct.len(), 1088);
		assert_eq!(mlkem_decapsulate(id.mlkem_decapsulation_key(), &ct).unwrap(), ss);
	}

	#[test]
	fn seeds_round_trip_bytes() {
		let s = Seeds::random();
		assert_eq!(Seeds::from_bytes(&s.to_bytes()).unwrap(), s);
	}
}
