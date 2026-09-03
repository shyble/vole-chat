//! What waits for someone who is away, and when it goes (PROTOCOL.md §7, §8).
//!
//! A message the hub could not forward stays here, on this device, and leaves when its
//! recipient shows as connected in a roster. Nothing about it is ever handed to a hub. The
//! app persists this (`to_json` / `from_json`) and calls [`Outbox::flush`] whenever a roster
//! arrives; one message at a time and in order, so a conversation lands the way it was written.

use crate::client::{HubClient, Verdict};
use serde::{Deserialize, Serialize};

pub const DEFAULT_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboxEntry {
	pub r#ref: String,
	pub to: String,
	pub to_name: String,
	pub text: String,
	/// When it was written — what the recipient's transcript should show.
	pub sent_at: i64,
	pub attempts: u32,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub last_error: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Outbox {
	entries: Vec<OutboxEntry>,
}

/// What a flush did to one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flushed {
	Delivered { r#ref: String, to: String },
	StillAway { r#ref: String, to: String },
	Refused { r#ref: String, to: String, reason: String },
	Expired { r#ref: String, to: String },
}

impl Outbox {
	pub fn new() -> Outbox {
		Outbox::default()
	}

	pub fn add(&mut self, to: &str, to_name: &str, text: &str, sent_at: i64) -> OutboxEntry {
		let e = OutboxEntry {
			r#ref: uuid::Uuid::new_v4().to_string(),
			to: to.to_string(),
			to_name: to_name.to_string(),
			text: text.to_string(),
			sent_at,
			attempts: 0,
			last_error: None,
		};
		self.entries.push(e.clone());
		e
	}

	pub fn remove(&mut self, r#ref: &str) -> bool {
		let before = self.entries.len();
		self.entries.retain(|e| e.r#ref != r#ref);
		self.entries.len() != before
	}

	pub fn list(&self) -> &[OutboxEntry] {
		&self.entries
	}

	pub fn for_peer(&self, to: &str) -> Vec<&OutboxEntry> {
		self.entries.iter().filter(|e| e.to == to).collect()
	}

	pub fn len(&self) -> usize {
		self.entries.len()
	}

	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}

	pub fn to_json(&self) -> String {
		serde_json::to_string(self).expect("outbox serializes")
	}

	pub fn from_json(s: &str) -> Outbox {
		serde_json::from_str(s).unwrap_or_default()
	}

	/// Drop entries older than `ttl_ms`; returns them.
	pub fn sweep(&mut self, now: i64, ttl_ms: i64) -> Vec<OutboxEntry> {
		let (expired, keep): (Vec<_>, Vec<_>) = self.entries.drain(..).partition(|e| now - e.sent_at > ttl_ms);
		self.entries = keep;
		expired
	}

	/// Try to deliver everything whose recipient is currently connected. Sequential and in
	/// order; when a recipient turns out to still be away, the rest of their messages wait too.
	pub async fn flush(&mut self, client: &HubClient, now: i64, ttl_ms: i64) -> Vec<Flushed> {
		let mut results: Vec<Flushed> = self
			.sweep(now, ttl_ms)
			.into_iter()
			.map(|e| Flushed::Expired { r#ref: e.r#ref, to: e.to })
			.collect();
		let connected: std::collections::HashSet<String> =
			client.roster().into_iter().filter(|m| m.connected).map(|m| m.instance_id).collect();
		let mut still_away = std::collections::HashSet::new();
		let refs: Vec<String> = self.entries.iter().map(|e| e.r#ref.clone()).collect();
		for r in refs {
			let Some(entry) = self.entries.iter().find(|e| e.r#ref == r).cloned() else { continue };
			if still_away.contains(&entry.to) || !connected.contains(&entry.to) {
				continue;
			}
			match client.send_chat(&entry.to, &entry.text, Some(entry.sent_at)).await {
				Verdict::Delivered | Verdict::Legacy => {
					self.remove(&r);
					results.push(Flushed::Delivered { r#ref: r, to: entry.to });
				}
				Verdict::Held => {
					self.note(&r, "held");
					still_away.insert(entry.to.clone());
					results.push(Flushed::StillAway { r#ref: r, to: entry.to });
				}
				Verdict::Refused(reason) => {
					self.note(&r, &reason);
					results.push(Flushed::Refused { r#ref: r, to: entry.to, reason });
				}
			}
		}
		results
	}

	fn note(&mut self, r#ref: &str, error: &str) {
		if let Some(e) = self.entries.iter_mut().find(|e| e.r#ref == r#ref) {
			e.attempts += 1;
			e.last_error = Some(error.to_string());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn json_round_trip_and_sweep() {
		let mut o = Outbox::new();
		o.add("b", "bob", "old", 1_000);
		o.add("b", "bob", "new", 900_000);
		let back = Outbox::from_json(&o.to_json());
		assert_eq!(back.len(), 2);
		let mut back = back;
		let expired = back.sweep(1_000_000, 500_000);
		assert_eq!(expired.len(), 1);
		assert_eq!(expired[0].text, "old");
		assert_eq!(back.list()[0].text, "new");
	}
}
