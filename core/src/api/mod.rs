//! What the Flutter app calls. Thin: every function here maps onto one thing in the crate, with
//! plain data going across the bridge. Identity secrets cross as a base64 seed bundle the app
//! keeps in the platform keystore; nothing else about a key ever leaves Rust.

use crate::frb_generated::StreamSink;
use crate::keys::Seeds;
use crate::{Event, HubClient, Identity, Outbox, Verdict};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use flutter_rust_bridge::frb;
use std::sync::Arc;

#[frb(init)]
pub fn init_app() {
	flutter_rust_bridge::setup_default_user_utils();
}

// ── Identity ──────────────────────────────────────────────────────────────

pub struct IdentityInfo {
	pub name: String,
	pub instance_id: String,
	pub key_string: String,
	/// 160 seed bytes, base64. This *is* the identity; back it up, never send it anywhere.
	pub seeds_b64: String,
}

fn identity_info(id: &Identity) -> IdentityInfo {
	IdentityInfo {
		name: id.name.clone(),
		instance_id: id.instance_id().to_string(),
		key_string: id.key_string(),
		seeds_b64: B64.encode(id.seeds().to_bytes()),
	}
}

fn identity_from(name: &str, seeds_b64: &str) -> anyhow::Result<Identity> {
	let bytes = B64.decode(seeds_b64).map_err(|_| anyhow::anyhow!("seeds are not base64"))?;
	let seeds = Seeds::from_bytes(&bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
	Ok(Identity::from_seeds(name, seeds))
}

#[frb(sync)]
pub fn identity_create(name: String) -> IdentityInfo {
	identity_info(&Identity::generate(&name))
}

#[frb(sync)]
pub fn identity_restore(name: String, seeds_b64: String) -> anyhow::Result<IdentityInfo> {
	Ok(identity_info(&identity_from(&name, &seeds_b64)?))
}

/// Is this a key string another node could be pinned by? (For pasting a hub or contact key.)
#[frb(sync)]
pub fn key_string_parse(key_string: String) -> anyhow::Result<PeerInfo> {
	let p = crate::keys::parse_key_string(&key_string).map_err(|e| anyhow::anyhow!("{e}"))?;
	Ok(PeerInfo { instance_id: p.instance_id, name: p.name, post_quantum: p.mldsa.is_some() })
}

pub struct PeerInfo {
	pub instance_id: String,
	pub name: String,
	pub post_quantum: bool,
}

// ── Hubs ──────────────────────────────────────────────────────────────────

pub struct JoinResult {
	pub hub_key_string: String,
	pub hub_instance_id: String,
	pub hub_name: String,
	pub pending: bool,
}

/// Register with a public hub and learn its key. Pin `hub_key_string`.
pub async fn hub_join(url: String, name: String, seeds_b64: String) -> anyhow::Result<JoinResult> {
	let me = identity_from(&name, &seeds_b64)?;
	let j = HubClient::join(&url, &me).await.map_err(|e| anyhow::anyhow!("{e}"))?;
	Ok(JoinResult { hub_key_string: j.hub_key_string, hub_instance_id: j.hub_instance_id, hub_name: j.hub_name, pending: j.pending })
}

// ── Session ───────────────────────────────────────────────────────────────

pub struct MemberInfo {
	pub instance_id: String,
	pub name: String,
	pub key_string: String,
	pub connected: bool,
	/// Announces a sealing key, so chat can be sent to it.
	pub reachable: bool,
}

pub struct NoticeInfo {
	pub from: String,
	pub from_name: String,
	pub count: u32,
	pub first: i64,
	pub last: i64,
}

pub enum SessionEvent {
	Connected { hub_name: String },
	Disconnected { reason: String },
	Roster { members: Vec<MemberInfo> },
	Chat { from: String, from_name: String, text: String, sent_at: i64, id: String },
	ConnectRequest { from: String, from_name: String, note: Option<String> },
	ConnectAccepted { from: String, from_name: String },
	ConnectDenied { from: String, from_name: String },
	Pending { notices: Vec<NoticeInfo> },
}

pub enum SendVerdict {
	Delivered,
	Held,
	Refused { reason: String },
	Legacy,
}

fn member_info(m: &crate::Member) -> MemberInfo {
	MemberInfo {
		instance_id: m.instance_id.clone(),
		name: m.name.clone(),
		key_string: m.key_string.clone(),
		connected: m.connected,
		reachable: m.x_public.is_some(),
	}
}

fn notice_info(n: &crate::Notice) -> NoticeInfo {
	NoticeInfo { from: n.from.clone(), from_name: n.from_name.clone(), count: n.count, first: n.first, last: n.last }
}

fn verdict(v: Verdict) -> SendVerdict {
	match v {
		Verdict::Delivered => SendVerdict::Delivered,
		Verdict::Held => SendVerdict::Held,
		Verdict::Refused(reason) => SendVerdict::Refused { reason },
		Verdict::Legacy => SendVerdict::Legacy,
	}
}

fn session_event(e: Event) -> SessionEvent {
	match e {
		Event::Connected { hub_name } => SessionEvent::Connected { hub_name },
		Event::Disconnected { reason } => SessionEvent::Disconnected { reason },
		Event::Roster(m) => SessionEvent::Roster { members: m.iter().map(member_info).collect() },
		Event::Chat { from, from_name, text, sent_at, id } => SessionEvent::Chat { from, from_name, text, sent_at, id },
		Event::ConnectRequest { from, from_name, note } => SessionEvent::ConnectRequest { from, from_name, note },
		Event::ConnectAccepted { from, from_name } => SessionEvent::ConnectAccepted { from, from_name },
		Event::ConnectDenied { from, from_name } => SessionEvent::ConnectDenied { from, from_name },
		Event::Pending(n) => SessionEvent::Pending { notices: n.iter().map(notice_info).collect() },
	}
}

/// A live connection to one hub. Call `events` once, right after connecting, to receive
/// everything the session reports.
#[frb(opaque)]
pub struct Session {
	client: HubClient,
	rx: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Event>>>,
}

/// Announce, bind the socket, and start the session. `consents` are members whose chat is
/// accepted without a fresh request.
pub async fn session_connect(
	url: String,
	hub_key_string: String,
	name: String,
	seeds_b64: String,
	consents: Vec<String>,
) -> anyhow::Result<Session> {
	let me = Arc::new(identity_from(&name, &seeds_b64)?);
	let (client, rx) = HubClient::connect(&url, &hub_key_string, me, consents).await.map_err(|e| anyhow::anyhow!("{e}"))?;
	Ok(Session { client, rx: std::sync::Mutex::new(Some(rx)) })
}

impl Session {
	/// The session's events, as a stream. Only the first call receives them.
	pub fn events(&self, sink: StreamSink<SessionEvent>) {
		let Some(mut rx) = self.rx.lock().unwrap().take() else { return };
		tokio::spawn(async move {
			while let Some(e) = rx.recv().await {
				if sink.add(session_event(e)).is_err() {
					break;
				}
			}
		});
	}

	#[frb(sync)]
	pub fn instance_id(&self) -> String {
		self.client.instance_id().to_string()
	}

	#[frb(sync)]
	pub fn hub_instance_id(&self) -> String {
		self.client.hub_instance_id().to_string()
	}

	#[frb(sync)]
	pub fn is_connected(&self) -> bool {
		self.client.is_connected()
	}

	#[frb(sync)]
	pub fn roster(&self) -> Vec<MemberInfo> {
		self.client.roster().iter().map(member_info).collect()
	}

	#[frb(sync)]
	pub fn pending(&self) -> Vec<NoticeInfo> {
		self.client.pending().iter().map(notice_info).collect()
	}

	#[frb(sync)]
	pub fn consents(&self) -> Vec<String> {
		self.client.consents()
	}

	/// Accept a member's chat from now on. The app persists the consent list.
	#[frb(sync)]
	pub fn accept(&self, instance_id: String) {
		self.client.accept(&instance_id)
	}

	pub async fn send_chat(&self, to: String, text: String, sent_at: Option<i64>) -> SendVerdict {
		verdict(self.client.send_chat(&to, &text, sent_at).await)
	}

	pub async fn request_connect(&self, to: String, note: Option<String>) -> SendVerdict {
		verdict(self.client.request_connect(&to, note.as_deref()).await)
	}

	pub async fn approve(&self, to: String) -> SendVerdict {
		verdict(self.client.approve(&to).await)
	}

	pub async fn deny(&self, to: String) -> SendVerdict {
		verdict(self.client.deny(&to).await)
	}

	/// Deliver what the outbox holds for members who are connected. Takes and returns the
	/// outbox as JSON, which is how the app persists it.
	pub async fn flush_outbox(&self, outbox_json: String, ttl_ms: i64) -> FlushOutcome {
		let mut outbox = Outbox::from_json(&outbox_json);
		let now = crate::message::now_ms();
		let results = outbox.flush(&self.client, now, ttl_ms).await;
		let mut delivered = Vec::new();
		let mut expired = Vec::new();
		let mut refused = Vec::new();
		for r in results {
			match r {
				crate::outbox::Flushed::Delivered { r#ref, .. } => delivered.push(r#ref),
				crate::outbox::Flushed::Expired { r#ref, .. } => expired.push(r#ref),
				crate::outbox::Flushed::Refused { r#ref, reason, .. } => refused.push(format!("{ref}: {reason}")),
				crate::outbox::Flushed::StillAway { .. } => {}
			}
		}
		FlushOutcome { outbox_json: outbox.to_json(), delivered, expired, refused }
	}

	pub async fn close(&self) {
		self.client.close().await
	}
}

pub struct FlushOutcome {
	pub outbox_json: String,
	/// refs delivered (remove them from the transcript's "held" state)
	pub delivered: Vec<String>,
	pub expired: Vec<String>,
	pub refused: Vec<String>,
}

// ── Outbox helpers (JSON in, JSON out) ─────────────────────────────────────

pub struct OutboxAdd {
	pub outbox_json: String,
	pub r#ref: String,
}

#[frb(sync)]
pub fn outbox_add(outbox_json: String, to: String, to_name: String, text: String, sent_at: i64) -> OutboxAdd {
	let mut outbox = Outbox::from_json(&outbox_json);
	let e = outbox.add(&to, &to_name, &text, sent_at);
	OutboxAdd { outbox_json: outbox.to_json(), r#ref: e.r#ref }
}

pub struct OutboxItem {
	pub r#ref: String,
	pub to: String,
	pub to_name: String,
	pub text: String,
	pub sent_at: i64,
	pub attempts: u32,
}

#[frb(sync)]
pub fn outbox_list(outbox_json: String) -> Vec<OutboxItem> {
	Outbox::from_json(&outbox_json)
		.list()
		.iter()
		.map(|e| OutboxItem {
			r#ref: e.r#ref.clone(),
			to: e.to.clone(),
			to_name: e.to_name.clone(),
			text: e.text.clone(),
			sent_at: e.sent_at,
			attempts: e.attempts,
		})
		.collect()
}

#[frb(sync)]
pub fn outbox_remove(outbox_json: String, r#ref: String) -> String {
	let mut outbox = Outbox::from_json(&outbox_json);
	outbox.remove(&r#ref);
	outbox.to_json()
}
