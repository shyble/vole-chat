//! A hub session (PROTOCOL.md §4–§7): join, announce, the WebSocket, ping discipline, roster,
//! relayed chat with the hub's verdict, consent, notices.
//!
//! The client talks to exactly one node — the hub — and trusts exactly one key for it, pinned
//! at join time. Everyone else is reached *through* the hub as a sealed envelope and verified
//! by the inner signature against the key the roster vouched for. The hub sees routing and
//! ciphertext, nothing else.

use crate::card::ContactCard;
use crate::keys::{parse_key_string, KeyError, PeerKeys};
use crate::message::{now_ms, Message};
use crate::seal::{seal, unseal, SealError, SealedBox};
use crate::Identity;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message as Ws;

/// A hub too old to give a verdict never answers; after this we assume the legacy meaning.
pub const VERDICT_TIMEOUT: Duration = Duration::from_secs(4);
/// Send our own ping this often (§5.3).
pub const PING_INTERVAL: Duration = Duration::from_secs(15);
/// The hub forgets us after 45 s of silence; we treat its silence the same way.
pub const SILENCE_LIMIT_MS: i64 = 45_000;
/// Replay window for inner (relayed) message ids.
const INNER_REPLAY_MS: i64 = 120_000;

#[derive(Debug, Error)]
pub enum ClientError {
	#[error("http: {0}")]
	Http(String),
	#[error("hub refused: {0}")]
	Refused(String),
	#[error("protocol: {0}")]
	Protocol(String),
	#[error("key: {0}")]
	Key(#[from] KeyError),
	#[error("seal: {0}")]
	Seal(#[from] SealError),
	#[error("not connected")]
	NotConnected,
	#[error("no such member on this hub: {0}")]
	UnknownPeer(String),
	#[error("member {0} announces no sealing key")]
	NoSealKey(String),
	#[error("the node has not accepted this identity yet")]
	NotTrusted,
}

/// What `POST /volenet/join` gave us. Pin `hub_key_string`; it is the hub from now on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JoinInfo {
	pub hub_key_string: String,
	pub hub_instance_id: String,
	pub hub_name: String,
	/// The hub queued the request for its operator; connecting will fail until approved.
	pub pending: bool,
}

/// A hub-rostered member (§6): where to seal to, and which key its inner signatures must match.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Member {
	pub instance_id: String,
	pub name: String,
	pub key_string: String,
	pub x_public: Option<String>,
	pub mlkem_public: Option<String>,
	pub connected: bool,
}

/// "Somebody tried to reach you while you were away" (§7).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Notice {
	pub from: String,
	pub from_name: String,
	pub count: u32,
	pub first: i64,
	pub last: i64,
}

/// The hub's answer about one envelope we sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
	/// Forwarded to the member.
	Delivered,
	/// The member is away; the hub kept a notice. Keep the message in the outbox.
	Held,
	/// Refused (`too-large`, `rate-limited`, …). Do not queue.
	Refused(String),
	/// No verdict within the timeout: an older hub. The write to the hub counts as delivery.
	Legacy,
}

/// What the session tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
	Connected { hub_name: String },
	Disconnected { reason: String },
	Roster(Vec<Member>),
	Chat { from: String, from_name: String, text: String, sent_at: i64, id: String },
	/// Someone wants to chat and needs your consent (or wrote to you before you gave it).
	ConnectRequest { from: String, from_name: String, note: Option<String> },
	ConnectAccepted { from: String, from_name: String },
	ConnectDenied { from: String, from_name: String },
	Pending(Vec<Notice>),
	/// An agent's brain answered. Only ever arrives over a direct link: a hub refuses to relay
	/// anything executable, so asking a brain through one is not possible by design.
	BrainReply { task_id: String, text: String, failed: bool },
}

struct Inner {
	me: Arc<Identity>,
	hub: PeerKeys,
	hub_name: Mutex<String>,
	tx: mpsc::UnboundedSender<String>,
	events: mpsc::UnboundedSender<Event>,
	roster: Mutex<Vec<Member>>,
	inflight: Mutex<HashMap<String, (String, oneshot::Sender<Verdict>)>>,
	consents: Mutex<HashSet<String>>,
	/// Cards the person scanned: their keys, pinned by us rather than vouched by the hub.
	pinned: Mutex<HashMap<String, ContactCard>>,
	/// Everyone we have ever been told about, with the keys they were verified as having.
	///
	/// A hub drops a member from its roster 45 s after they go quiet, and a phone goes quiet
	/// the moment its screen locks. Without this, writing to someone whose screen is off fails
	/// with "no such member" — which is precisely when a message most needs to be held for
	/// them. Remembering their keys means we can still seal it, the hub can still say it is
	/// holding it, and it goes out when they are back.
	known: Mutex<HashMap<String, Member>>,
	seen: Mutex<HashMap<String, i64>>,
	pending: Mutex<Vec<Notice>>,
	last_chat_from: Mutex<HashMap<String, i64>>,
	last_rx: AtomicI64,
	closed: AtomicBool,
}

pub struct HubClient {
	inner: Arc<Inner>,
	tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

fn http() -> Result<reqwest::Client, ClientError> {
	reqwest::Client::builder()
		.timeout(Duration::from_secs(10))
		.build()
		.map_err(|e| ClientError::Http(e.to_string()))
}

fn ws_url(http_url: &str) -> String {
	let u = http_url.trim_end_matches('/');
	if let Some(rest) = u.strip_prefix("https://") {
		format!("wss://{rest}")
	} else if let Some(rest) = u.strip_prefix("http://") {
		format!("ws://{rest}")
	} else {
		u.to_string()
	}
}

impl HubClient {
	/// §4: register with a public hub and learn its key. Idempotent.
	pub async fn join(hub_url: &str, me: &Identity) -> Result<JoinInfo, ClientError> {
		let base = hub_url.trim_end_matches('/');
		let res = http()?
			.post(format!("{base}/volenet/join"))
			.json(&json!({ "publicKey": me.key_string(), "name": me.name }))
			.send()
			.await
			.map_err(|e| ClientError::Http(e.to_string()))?;
		let status = res.status();
		let text = res.text().await.map_err(|e| ClientError::Http(e.to_string()))?;
		if status.as_u16() == 404 || text.trim().is_empty() {
			return Err(ClientError::Refused("not a public hub (publicJoin is not enabled there)".into()));
		}
		let v: Value = serde_json::from_str(&text).map_err(|_| ClientError::Protocol(format!("join: non-JSON reply: {text}")))?;
		if !status.is_success() {
			return Err(ClientError::Refused(v["error"].as_str().unwrap_or("join failed").to_string()));
		}
		if v["pending"].as_bool() == Some(true) {
			return Ok(JoinInfo { hub_key_string: String::new(), hub_instance_id: String::new(), hub_name: String::new(), pending: true });
		}
		let key = v["hubPublicKey"].as_str().ok_or_else(|| ClientError::Protocol("join: hub returned no key".into()))?;
		let hub = parse_key_string(key)?;
		Ok(JoinInfo {
			hub_key_string: key.to_string(),
			hub_instance_id: hub.instance_id,
			hub_name: v["instanceName"].as_str().unwrap_or("hub").to_string(),
			pending: false,
		})
	}

	/// §5: announce, open the socket, bind it, and keep it alive. `consents` are the members
	/// whose relayed chat is accepted without a fresh request (persisted by the app).
	pub async fn connect(
		hub_url: &str,
		hub_key_string: &str,
		me: Arc<Identity>,
		consents: impl IntoIterator<Item = String>,
		pinned: impl IntoIterator<Item = ContactCard>,
		known: impl IntoIterator<Item = Member>,
	) -> Result<(HubClient, mpsc::UnboundedReceiver<Event>), ClientError> {
		let hub = parse_key_string(hub_key_string)?;
		let base = hub_url.trim_end_matches('/').to_string();

		// 1. Announce over HTTP; the hub answers inline with its own record.
		let discover = Message::create(
			&me,
			"discover",
			"*",
			json!({
				"name": me.name,
				"publicKey": me.key_string(),
				"endpoint": "",
				"capabilities": [],
				"role": "peer",
				"version": "3.0.0",
				"xPublicKey": me.x25519_public_b64(),
				"mlkemPublicKey": me.mlkem_public_b64(),
			}),
		);
		let res = http()?
			.post(format!("{base}/volenet/message"))
			.header("content-type", "application/json")
			.body(discover.to_json())
			.send()
			.await
			.map_err(|e| ClientError::Http(e.to_string()))?;
		if res.status().as_u16() == 401 {
			return Err(ClientError::Refused("the hub does not know this identity — join it first".into()));
		}
		if !res.status().is_success() {
			return Err(ClientError::Http(format!("discover: HTTP {}", res.status())));
		}
		let body: Value = res.json().await.map_err(|e| ClientError::Protocol(format!("discover: {e}")))?;
		let reply = body.get("response").cloned().ok_or_else(|| {
			ClientError::Refused("the hub did not answer our announcement — is this identity trusted there?".into())
		})?;
		let reply: Message = serde_json::from_value(reply).map_err(|e| ClientError::Protocol(format!("discover:response: {e}")))?;
		reply.verify(&hub, now_ms()).map_err(|e| ClientError::Protocol(format!("hub's discover:response failed to verify: {e}")))?;
		let hub_name = reply.payload["name"].as_str().unwrap_or(&hub.name).to_string();
		let advertised = reply.payload["endpoint"].as_str().unwrap_or("").to_string();
		let socket_base = if advertised.starts_with("http") { advertised } else { base.clone() };

		// 2. The socket.
		let (ws, _) = tokio_tungstenite::connect_async(ws_url(&socket_base))
			.await
			.map_err(|e| ClientError::Http(format!("websocket: {e}")))?;
		let (mut sink, mut stream) = ws.split();
		let (tx, mut rx) = mpsc::unbounded_channel::<String>();
		let (events, event_rx) = mpsc::unbounded_channel::<Event>();
		let inner = Arc::new(Inner {
			me,
			hub,
			hub_name: Mutex::new(hub_name.clone()),
			tx,
			events,
			roster: Mutex::new(Vec::new()),
			inflight: Mutex::new(HashMap::new()),
			consents: Mutex::new(consents.into_iter().collect()),
			pinned: Mutex::new(
				pinned
					.into_iter()
					.map(|c| (c.instance_id(), c))
					.collect(),
			),
			known: Mutex::new(known.into_iter().map(|m| (m.instance_id.clone(), m)).collect()),
			seen: Mutex::new(HashMap::new()),
			pending: Mutex::new(Vec::new()),
			last_chat_from: Mutex::new(HashMap::new()),
			last_rx: AtomicI64::new(now_ms()),
			closed: AtomicBool::new(false),
		});

		// 3. Bind: the first verified message names us to the hub. Then keep pinging.
		inner.send(&Message::create(&inner.me, "ping", inner.hub.instance_id.as_str(), json!({ "timestamp": now_ms() })));

		let writer = {
			let inner = inner.clone();
			tokio::spawn(async move {
				while let Some(text) = rx.recv().await {
					if sink.send(Ws::Text(text.into())).await.is_err() {
						inner.disconnect("socket write failed");
						break;
					}
				}
			})
		};
		let reader = {
			let inner = inner.clone();
			tokio::spawn(async move {
				while let Some(frame) = stream.next().await {
					match frame {
						Ok(Ws::Text(t)) => inner.on_text(t.as_str()),
						Ok(Ws::Close(_)) | Err(_) => {
							inner.disconnect("socket closed");
							break;
						}
						_ => {}
					}
				}
				inner.disconnect("socket ended");
			})
		};
		let keepalive = {
			let inner = inner.clone();
			tokio::spawn(async move {
				let mut tick = tokio::time::interval(PING_INTERVAL);
				tick.tick().await;
				loop {
					tick.tick().await;
					if inner.closed.load(Ordering::SeqCst) {
						break;
					}
					if now_ms() - inner.last_rx.load(Ordering::SeqCst) > SILENCE_LIMIT_MS {
						inner.disconnect("hub silent for 45 s");
						break;
					}
					inner.send(&Message::create(&inner.me, "ping", inner.hub.instance_id.as_str(), json!({ "timestamp": now_ms() })));
				}
			})
		};

		let _ = inner.events.send(Event::Connected { hub_name });
		Ok((HubClient { inner, tasks: Mutex::new(vec![writer, reader, keepalive]) }, event_rx))
	}

	pub fn instance_id(&self) -> &str {
		self.inner.me.instance_id()
	}

	pub fn hub_instance_id(&self) -> &str {
		&self.inner.hub.instance_id
	}

	pub fn hub_name(&self) -> String {
		self.inner.hub_name.lock().unwrap().clone()
	}

	pub fn is_connected(&self) -> bool {
		!self.inner.closed.load(Ordering::SeqCst)
	}

	/// The hub's directory as last broadcast.
	pub fn roster(&self) -> Vec<Member> {
		self.inner.roster.lock().unwrap().clone()
	}

	pub fn member(&self, id_or_name: &str) -> Option<Member> {
		self.inner.find_member(id_or_name)
	}

	/// Everyone we have been told about and verified, so the app can keep them across restarts.
	pub fn known_members(&self) -> Vec<Member> {
		self.inner.known.lock().unwrap().values().cloned().collect()
	}

	/// Pin a card on a live session, so scanning one needs no reconnect.
	pub fn pin(&self, card: ContactCard) {
		self.inner.pinned.lock().unwrap().insert(card.instance_id(), card);
	}

	/// Who tried to reach me while I was away, until their messages arrive.
	pub fn pending(&self) -> Vec<Notice> {
		self.inner.pending.lock().unwrap().clone()
	}

	/// Members whose relayed chat is accepted.
	pub fn consents(&self) -> Vec<String> {
		self.inner.consents.lock().unwrap().iter().cloned().collect()
	}

	/// Accept a member's relayed chat from now on (the app persists this).
	pub fn accept(&self, instance_id: &str) {
		self.inner.consents.lock().unwrap().insert(instance_id.to_string());
	}

	/// Stop accepting them, and forget the card that pinned them. Their next message is held
	/// as a fresh request, which is what removing someone should mean.
	pub fn revoke(&self, instance_id: &str) {
		self.inner.consents.lock().unwrap().remove(instance_id);
		self.inner.pinned.lock().unwrap().remove(instance_id);
	}

	/// §7: seal a chat message to a member and send it via the hub. `sent_at` is when it was
	/// written — pass the original time when re-sending from the outbox.
	pub async fn send_chat(&self, to: &str, text: &str, sent_at: Option<i64>) -> Verdict {
		let payload = json!({
			"text": text,
			"fromName": self.inner.me.name,
			"sentAt": sent_at.unwrap_or_else(now_ms),
		});
		self.send_inner(to, "chat:message", payload).await
	}

	/// Ask a member to accept our chat. Counts as accepting theirs.
	pub async fn request_connect(&self, to: &str, note: Option<&str>) -> Verdict {
		if let Some(m) = self.inner.find_member(to) {
			self.accept(&m.instance_id);
		}
		let mut payload = json!({ "fromName": self.inner.me.name });
		if let Some(n) = note {
			payload["note"] = json!(n);
		}
		self.send_inner(to, "relay:connect-request", payload).await
	}

	/// Approve an inbound request: consent, and tell them.
	pub async fn approve(&self, to: &str) -> Verdict {
		if let Some(m) = self.inner.find_member(to) {
			self.accept(&m.instance_id);
		}
		self.send_inner(to, "relay:connect-accept", json!({ "fromName": self.inner.me.name })).await
	}

	pub async fn deny(&self, to: &str) -> Verdict {
		self.send_inner(to, "relay:connect-deny", json!({ "fromName": self.inner.me.name })).await
	}

	async fn send_inner(&self, to: &str, r#type: &str, payload: Value) -> Verdict {
		let inner = &self.inner;
		if inner.closed.load(Ordering::SeqCst) {
			return Verdict::Refused("not connected".into());
		}
		let member = match inner.find_member(to) {
			Some(m) => {
				// The hub supplies the sealing keys, so when a card was scanned they have to
				// match what it committed to. Without that a hostile hub could hand out its own
				// and read everything it forwards.
				if let Some(card) = inner.pinned.lock().unwrap().get(&m.instance_id) {
					if let Err(e) =
						card.verify(&m.key_string, m.x_public.as_deref(), m.mlkem_public.as_deref())
					{
						return Verdict::Refused(format!("{}: {e}", m.name));
					}
				}
				m
			}
			// No hub has described them — they have been away long enough to be forgotten, or
			// were never online while we were. A scanned card carries their sealing keys, so we
			// can still write, and the hub will hold it for them.
			None => match inner.pinned.lock().unwrap().get(to).cloned() {
				Some(c) if c.seals() => Member {
					instance_id: c.instance_id(),
					name: c.name.clone(),
					key_string: String::new(),
					x_public: Some(c.x_public.clone()),
					mlkem_public: (!c.mlkem_public.is_empty()).then(|| c.mlkem_public.clone()),
					connected: false,
				},
				_ => return Verdict::Refused(format!("no such member: {to}")),
			},
		};
		let (x_pub, mlkem_pub) = (member.x_public.clone(), member.mlkem_public.clone());
		let Some(x_pub) = x_pub else {
			return Verdict::Refused(format!("{} announces no sealing key", member.name));
		};
		let msg = Message::create(&inner.me, r#type, &member.instance_id, payload);
		let aad = format!("{}|{}", inner.me.instance_id(), member.instance_id);
		let sealed = match seal(&x_pub, mlkem_pub.as_deref(), msg.to_json().as_bytes(), &aad) {
			Ok(s) => s,
			Err(e) => return Verdict::Refused(format!("seal: {e}")),
		};
		let r#ref = uuid::Uuid::new_v4().to_string();
		let (vtx, vrx) = oneshot::channel();
		inner.inflight.lock().unwrap().insert(r#ref.clone(), (member.instance_id.clone(), vtx));
		let outer = Message::create(
			&inner.me,
			"sealed",
			&inner.hub.instance_id,
			json!({ "to": member.instance_id, "box": sealed, "ref": r#ref }),
		);
		inner.send(&outer);
		match tokio::time::timeout(VERDICT_TIMEOUT, vrx).await {
			Ok(Ok(v)) => v,
			_ => {
				inner.inflight.lock().unwrap().remove(&r#ref);
				Verdict::Legacy
			}
		}
	}

	pub async fn close(&self) {
		self.inner.disconnect("closed by the app");
		let tasks: Vec<_> = self.tasks.lock().unwrap().drain(..).collect();
		for t in tasks {
			t.abort();
		}
	}
}

impl Inner {
	fn send(&self, m: &Message) {
		let _ = self.tx.send(m.to_json());
	}

	fn disconnect(&self, reason: &str) {
		if !self.closed.swap(true, Ordering::SeqCst) {
			let _ = self.events.send(Event::Disconnected { reason: reason.to_string() });
		}
	}

	fn find_member(&self, id_or_name: &str) -> Option<Member> {
		let matches = |m: &Member| {
			m.instance_id == id_or_name || m.name == id_or_name || m.instance_id.starts_with(id_or_name)
		};
		if let Some(m) = self.roster.lock().unwrap().iter().find(|m| matches(m)) {
			return Some(m.clone());
		}
		// Not on the roster now — away, or gone long enough for the hub to forget them. What we
		// were told before is still true, and is what lets the message be held for them.
		self.known.lock().unwrap().values().find(|m| matches(m)).map(|m| {
			let mut away = m.clone();
			away.connected = false;
			away
		})
	}

	/// A text frame from the hub. Only the hub may speak on this socket, and only with a valid,
	/// fresh, unseen signature.
	fn on_text(&self, text: &str) {
		let Some(msg) = Message::from_json(text) else { return };
		if msg.from != self.hub.instance_id {
			return;
		}
		if msg.verify(&self.hub, now_ms()).is_err() {
			return;
		}
		if !self.first_time(&msg.from, &msg.id) {
			return;
		}
		self.last_rx.store(now_ms(), Ordering::SeqCst);
		self.on_message(msg);
	}

	fn on_message(&self, msg: Message) {
		match msg.r#type.as_str() {
			"ping" => self.send(&Message::create(&self.me, "pong", &self.hub.instance_id, json!({ "timestamp": now_ms() }))),
			"pong" => {}
			"tool:list" => self.send(&Message::create(&self.me, "tool:list:response", &self.hub.instance_id, json!([]))),
			"roster" => self.on_roster(&msg.payload),
			"relay:ack" => {
				if let Some(r) = msg.payload["ref"].as_str() {
					self.settle_ref(r, Verdict::Delivered);
				}
			}
			"relay:error" => {
				let reason = msg.payload["reason"].as_str().unwrap_or("unknown").to_string();
				let held = msg.payload["held"].as_bool() == Some(true) || reason == "peer-unreachable";
				let verdict = if held { Verdict::Held } else { Verdict::Refused(reason) };
				match msg.payload["ref"].as_str() {
					Some(r) => self.settle_ref(r, verdict),
					None => {
						if let Some(to) = msg.payload["to"].as_str() {
							self.settle_to(to, verdict);
						}
					}
				}
			}
			"relay:deliver" => self.on_deliver(&msg.payload),
			"sealed:direct" => {
				// The hub wrapped one of its own messages for us (net.encrypt on its side).
				let Ok(sealed) = serde_json::from_value::<SealedBox>(msg.payload["box"].clone()) else { return };
				let aad = format!("{}|{}", self.hub.instance_id, self.me.instance_id());
				let Ok(plain) = unseal(&self.me, &sealed, &aad) else { return };
				let Ok(text) = String::from_utf8(plain) else { return };
				self.on_text(&text);
			}
			"relay:pending" => self.on_pending(&msg.payload),
			_ => {}
		}
	}

	fn on_roster(&self, payload: &Value) {
		let Some(list) = payload["members"].as_array() else { return };
		let mut members = Vec::new();
		for m in list {
			let Some(id) = m["instanceId"].as_str() else { continue };
			if id == self.me.instance_id() {
				continue;
			}
			members.push(Member {
				instance_id: id.to_string(),
				name: m["name"].as_str().unwrap_or(&id[..8.min(id.len())]).to_string(),
				key_string: m["publicKey"].as_str().unwrap_or("").to_string(),
				x_public: m["xPublicKey"].as_str().map(str::to_string),
				mlkem_public: m["mlkemPublicKey"].as_str().map(str::to_string),
				connected: m["connected"].as_bool().unwrap_or(false),
			});
		}
		{
			let pinned = self.pinned.lock().unwrap();
			let mut known = self.known.lock().unwrap();
			for m in &members {
				// Only remember keys that satisfy a card we hold, so a hostile roster cannot
				// poison what we fall back to later.
				let ok = match pinned.get(&m.instance_id) {
					Some(c) => c.verify(&m.key_string, m.x_public.as_deref(), m.mlkem_public.as_deref()).is_ok(),
					None => !m.key_string.is_empty(),
				};
				if ok {
					known.insert(m.instance_id.clone(), m.clone());
				}
			}
		}
		*self.roster.lock().unwrap() = members.clone();
		let _ = self.events.send(Event::Roster(members));
	}

	fn on_deliver(&self, payload: &Value) {
		let Some(from) = payload["from"].as_str() else { return };
		let Ok(sealed) = serde_json::from_value::<SealedBox>(payload["box"].clone()) else { return };
		let aad = format!("{}|{}", from, self.me.instance_id());
		let Ok(plain) = unseal(&self.me, &sealed, &aad) else { return };
		let Ok(text) = String::from_utf8(plain) else { return };
		let Some(inner) = Message::from_json(&text) else { return };
		if inner.from != from || inner.to != self.me.instance_id() {
			return;
		}
		// The inner signature must match the key we pinned for them, or — when we pinned none —
		// the one the roster vouches for. A roster that contradicts a pinned card is somebody
		// being passed off as them, so nothing from it is delivered.
		let card = self.pinned.lock().unwrap().get(from).cloned();
		let Some(member) = self.find_member(from).or_else(|| {
			// A first message from someone no roster has described yet: their card is enough to
			// know who they are, and their name.
			card.as_ref().filter(|c| c.seals()).map(|c| Member {
				instance_id: c.instance_id(),
				name: c.name.clone(),
				key_string: String::new(),
				x_public: Some(c.x_public.clone()),
				mlkem_public: (!c.mlkem_public.is_empty()).then(|| c.mlkem_public.clone()),
				connected: false,
			})
		}) else {
			return;
		};
		let peer = match &card {
			// A scanned card decides: the keys the hub offers must be the ones it committed to,
			// or this is somebody being passed off as them and nothing is delivered.
			Some(c) if !member.key_string.is_empty() => {
				let Ok(p) = c.verify(&member.key_string, member.x_public.as_deref(), member.mlkem_public.as_deref())
				else {
					return;
				};
				p
			}
			// Nothing to check it against yet, so the card's own identity key decides.
			Some(c) => {
				let Ok(p) = c.identity_peer() else { return };
				p
			}
			None => {
				let Ok(p) = parse_key_string(&member.key_string) else { return };
				p
			}
		};
		let member_name = member.name.clone();
		if inner.verify(&peer, now_ms()).is_err() {
			return;
		}
		if !self.first_time(&inner.from, &inner.id) {
			return;
		}
		let from_name = inner.payload["fromName"].as_str().unwrap_or(&member_name).to_string();
		match inner.r#type.as_str() {
			"chat:message" => {
				let Some(text) = inner.payload["text"].as_str() else { return };
				if !self.consents.lock().unwrap().contains(from) {
					// Held until the user decides — surfaced the way core does, as a request.
					let _ = self.events.send(Event::ConnectRequest { from: from.to_string(), from_name, note: None });
					return;
				}
				let sent_at = match inner.payload["sentAt"].as_i64() {
					Some(t) if t > 0 && t <= inner.timestamp + 60_000 => t,
					_ => inner.timestamp,
				};
				self.last_chat_from.lock().unwrap().insert(from.to_string(), inner.timestamp);
				self.pending.lock().unwrap().retain(|n| n.from != from);
				let _ = self.events.send(Event::Chat { from: from.to_string(), from_name, text: text.to_string(), sent_at, id: inner.id });
			}
			"relay:connect-request" => {
				let note = inner.payload["note"].as_str().map(str::to_string);
				let _ = self.events.send(Event::ConnectRequest { from: from.to_string(), from_name, note });
			}
			"relay:connect-accept" => {
				let _ = self.events.send(Event::ConnectAccepted { from: from.to_string(), from_name });
			}
			"relay:connect-deny" => {
				let _ = self.events.send(Event::ConnectDenied { from: from.to_string(), from_name });
			}
			_ => {}
		}
	}

	fn on_pending(&self, payload: &Value) {
		let Some(senders) = payload["senders"].as_array() else { return };
		let heard = self.last_chat_from.lock().unwrap();
		let now = now_ms();
		let mut notices = Vec::new();
		for s in senders {
			let Some(from) = s["from"].as_str() else { continue };
			let last = s["last"].as_i64().unwrap_or(now);
			// Their message may already be here; a notice about earlier attempts announces nothing.
			if heard.get(from).is_some_and(|&h| h >= last) {
				continue;
			}
			notices.push(Notice {
				from: from.to_string(),
				from_name: s["fromName"].as_str().filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| {
					self.find_member(from).map(|m| m.name).unwrap_or_else(|| from[..8.min(from.len())].to_string())
				}),
				count: s["count"].as_u64().unwrap_or(1).max(1) as u32,
				first: s["first"].as_i64().unwrap_or(now),
				last,
			});
		}
		drop(heard);
		if notices.is_empty() {
			return;
		}
		*self.pending.lock().unwrap() = notices.clone();
		let _ = self.events.send(Event::Pending(notices));
	}

	fn settle_ref(&self, r#ref: &str, verdict: Verdict) {
		if let Some((_, tx)) = self.inflight.lock().unwrap().remove(r#ref) {
			let _ = tx.send(verdict);
		}
	}

	/// An older hub echoes no ref: settle everything in flight to that member.
	fn settle_to(&self, to: &str, verdict: Verdict) {
		let mut inflight = self.inflight.lock().unwrap();
		let refs: Vec<String> = inflight.iter().filter(|(_, (t, _))| t == to).map(|(r, _)| r.clone()).collect();
		for r in refs {
			if let Some((_, tx)) = inflight.remove(&r) {
				let _ = tx.send(verdict.clone());
			}
		}
	}

	/// Replay guard keyed on (from, id); prunes as it goes.
	fn first_time(&self, from: &str, id: &str) -> bool {
		let now = now_ms();
		let mut seen = self.seen.lock().unwrap();
		seen.retain(|_, t| now - *t < INNER_REPLAY_MS);
		let key = format!("{from}:{id}");
		if seen.contains_key(&key) {
			return false;
		}
		seen.insert(key, now);
		true
	}
}

impl Drop for HubClient {
	fn drop(&mut self) {
		self.inner.closed.store(true, Ordering::SeqCst);
		for t in self.tasks.lock().unwrap().drain(..) {
			t.abort();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ws_url_swaps_scheme_only() {
		assert_eq!(ws_url("http://h:9700/"), "ws://h:9700");
		assert_eq!(ws_url("https://club.example.com/mesh"), "wss://club.example.com/mesh");
	}
}
