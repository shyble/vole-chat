//! Signed messages: create and verify (PROTOCOL.md §2).

use crate::canonical::signed_string;
use crate::keys::{verify_ed25519, verify_mldsa, Identity, PeerKeys};
use crate::VOLENET_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Reject messages older than this (replay window) …
pub const MAX_MESSAGE_AGE_MS: i64 = 60_000;
/// … or further in the future than this (clock skew allowance).
pub const MAX_FUTURE_MS: i64 = 5_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
	pub version: u32,
	pub id: String,
	#[serde(rename = "type")]
	pub r#type: String,
	pub from: String,
	pub to: String,
	pub timestamp: i64,
	pub signature: String,
	#[serde(rename = "sigPq", skip_serializing_if = "Option::is_none")]
	pub sig_pq: Option<String>,
	#[serde(default)]
	pub payload: Value,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
	#[error("unsupported protocol version {0}")]
	Version(u32),
	#[error("message too old by {0} ms")]
	TooOld(i64),
	#[error("message from the future by {0} ms")]
	FromFuture(i64),
	#[error("invalid signature")]
	Signature,
	#[error("missing post-quantum signature")]
	MissingPq,
	#[error("invalid post-quantum signature")]
	PqSignature,
	#[error("message is not from that peer")]
	WrongSender,
}

impl Message {
	/// Sign a new message as `me`. Hybrid: Ed25519 plus ML-DSA-65 over the same string.
	pub fn create(me: &Identity, r#type: &str, to: &str, payload: Value) -> Message {
		let id = uuid::Uuid::new_v4().to_string();
		let timestamp = now_ms();
		let data = signed_string(&id, r#type, me.instance_id(), to, timestamp, &payload);
		Message {
			version: VOLENET_VERSION,
			id,
			r#type: r#type.to_string(),
			from: me.instance_id().to_string(),
			to: to.to_string(),
			timestamp,
			signature: me.sign_ed25519(data.as_bytes()),
			sig_pq: Some(me.sign_mldsa(data.as_bytes())),
			payload,
		}
	}

	/// The string this message's signatures cover.
	pub fn signed_string(&self) -> String {
		signed_string(&self.id, &self.r#type, &self.from, &self.to, self.timestamp, &self.payload)
	}

	/// Verify freshness and both signatures against a known peer, the way a hub does.
	/// When the peer is known to have an ML-DSA key, `sigPq` is required.
	pub fn verify(&self, peer: &PeerKeys, now: i64) -> Result<(), VerifyError> {
		if self.from != peer.instance_id {
			return Err(VerifyError::WrongSender);
		}
		self.verify_with_keys(peer, now)
	}

	/// Signature and freshness checks only — no sender/id binding. Use `verify` unless the
	/// binding is checked elsewhere.
	pub fn verify_with_keys(&self, peer: &PeerKeys, now: i64) -> Result<(), VerifyError> {
		if self.version != VOLENET_VERSION {
			return Err(VerifyError::Version(self.version));
		}
		let age = now - self.timestamp;
		if age > MAX_MESSAGE_AGE_MS {
			return Err(VerifyError::TooOld(age));
		}
		if age < -MAX_FUTURE_MS {
			return Err(VerifyError::FromFuture(-age));
		}
		let data = self.signed_string();
		if !verify_ed25519(&peer.ed25519, data.as_bytes(), &self.signature) {
			return Err(VerifyError::Signature);
		}
		if let Some(pq) = &peer.mldsa {
			match &self.sig_pq {
				None => return Err(VerifyError::MissingPq),
				Some(sig) if !verify_mldsa(pq, data.as_bytes(), sig) => return Err(VerifyError::PqSignature),
				_ => {}
			}
		}
		Ok(())
	}

	pub fn to_json(&self) -> String {
		serde_json::to_string(self).expect("message serializes")
	}

	pub fn from_json(s: &str) -> Option<Message> {
		let m: Message = serde_json::from_str(s).ok()?;
		if m.id.is_empty() || m.r#type.is_empty() || m.from.is_empty() {
			return None;
		}
		Some(m)
	}
}

pub fn now_ms() -> i64 {
	use std::time::{SystemTime, UNIX_EPOCH};
	SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn a_message_verifies_against_its_author_and_nobody_else() {
		let me = Identity::generate("me");
		let other = Identity::generate("other");
		let m = Message::create(&me, "ping", "peer", json!({"timestamp": 1}));
		assert_eq!(m.verify(&me.as_peer(), now_ms()), Ok(()));
		assert_eq!(m.verify(&other.as_peer(), now_ms()), Err(VerifyError::WrongSender));
		let mut forged = m.clone();
		forged.from = other.instance_id().to_string();
		assert_eq!(forged.verify(&other.as_peer(), now_ms()), Err(VerifyError::Signature));
	}

	#[test]
	fn freshness_is_enforced() {
		let me = Identity::generate("me");
		let m = Message::create(&me, "ping", "*", Value::Null);
		assert_eq!(m.verify(&me.as_peer(), m.timestamp + 61_000), Err(VerifyError::TooOld(61_000)));
		assert_eq!(m.verify(&me.as_peer(), m.timestamp - 6_000), Err(VerifyError::FromFuture(6_000)));
	}

	#[test]
	fn a_peer_known_to_be_pq_requires_the_pq_signature() {
		let me = Identity::generate("me");
		let mut m = Message::create(&me, "ping", "*", Value::Null);
		m.sig_pq = None;
		assert_eq!(m.verify(&me.as_peer(), now_ms()), Err(VerifyError::MissingPq));
		let mut legacy = me.as_peer();
		legacy.mldsa = None;
		assert_eq!(m.verify(&legacy, now_ms()), Ok(()));
	}

	#[test]
	fn json_round_trips_with_the_wire_field_names() {
		let me = Identity::generate("me");
		let m = Message::create(&me, "chat:message", "x", json!({"text": "hi"}));
		let s = m.to_json();
		assert!(s.contains("\"sigPq\":"));
		assert!(s.contains("\"type\":\"chat:message\""));
		assert_eq!(Message::from_json(&s).unwrap(), m);
	}
}
