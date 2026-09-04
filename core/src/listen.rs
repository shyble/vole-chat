//! Being reachable: the same protocol, served rather than dialled (PROTOCOL.md §4–§5 inverted).
//!
//! Two phones cannot dial each other across the internet — neither has an address — so they
//! talk through a hub. On one network they can: a foreground app may listen, and the other
//! phone dials it exactly as it dials an agent. That is what this is. The endpoints are the
//! ones every VoleNet node serves, so an **agent can dial a phone too** once the phone has
//! accepted it.
//!
//! Trust is the same contract as everywhere else: nothing is trusted because it connected.
//! An unknown caller can only fetch `/volenet/info` and leave a pair request; until the person
//! accepts it, its announcement is refused and its socket is dropped.

use crate::client::Event;
use crate::keys::{parse_key_string, PeerKeys};
use crate::message::{now_ms, Message};
use crate::seal::{seal, unseal, SealedBox};
use crate::Identity;
use axum::extract::ws::{Message as Ws, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Someone who has asked to be trusted, waiting for the person to decide.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairRequest {
	pub instance_id: String,
	pub name: String,
	pub key_string: String,
	pub note: Option<String>,
	pub at: i64,
}

/// One caller we have accepted, and its live socket if it is connected right now.
struct Caller {
	peer: PeerKeys,
	x_public: Option<String>,
	mlkem_public: Option<String>,
	tx: Option<mpsc::UnboundedSender<String>>,
}

struct Shared {
	me: Arc<Identity>,
	events: mpsc::UnboundedSender<Event>,
	/// Accepted callers, by instance id.
	callers: Mutex<HashMap<String, Caller>>,
	/// Asked, not yet decided.
	requests: Mutex<Vec<PairRequest>>,
	seen: Mutex<HashMap<String, i64>>,
	closed: AtomicBool,
}

/// A listening endpoint. Drop it, or call [`Listener::stop`], to stop being reachable.
pub struct Listener {
	shared: Arc<Shared>,
	port: u16,
	task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Listener {
	/// Start listening on `port` (0 picks a free one). `accepted` are callers trusted from a
	/// previous run, as key strings the app persisted.
	pub async fn start(
		me: Arc<Identity>,
		port: u16,
		accepted: impl IntoIterator<Item = String>,
	) -> std::io::Result<(Listener, mpsc::UnboundedReceiver<Event>)> {
		let (events, rx) = mpsc::unbounded_channel();
		let mut callers = HashMap::new();
		for key in accepted {
			if let Ok(peer) = parse_key_string(&key) {
				callers.insert(
					peer.instance_id.clone(),
					Caller { peer, x_public: None, mlkem_public: None, tx: None },
				);
			}
		}
		let shared = Arc::new(Shared {
			me,
			events,
			callers: Mutex::new(callers),
			requests: Mutex::new(Vec::new()),
			seen: Mutex::new(HashMap::new()),
			closed: AtomicBool::new(false),
		});
		let app = Router::new()
			.route("/health", get(|| async { Json(json!({"status":"ok","protocol":"volenet","version":1})) }))
			.route("/volenet/info", get(info))
			.route("/volenet/pair", post(pair))
			.route("/volenet/message", post(message))
			.route("/", any(upgrade))
			.fallback(any(upgrade))
			.with_state(shared.clone());
		let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
		let bound = listener.local_addr()?.port();
		let task = tokio::spawn(async move {
			let _ = axum::serve(listener, app).await;
		});
		Ok((Listener { shared, port: bound, task: Mutex::new(Some(task)) }, rx))
	}

	/// The port actually bound.
	pub fn port(&self) -> u16 {
		self.port
	}

	/// The address to hand to the other phone, as a URL they can paste.
	pub fn url(&self) -> Option<String> {
		lan_ip().map(|ip| format!("http://{ip}:{}", self.port))
	}

	pub fn is_listening(&self) -> bool {
		!self.shared.closed.load(Ordering::SeqCst)
	}

	/// Callers waiting to be trusted.
	pub fn requests(&self) -> Vec<PairRequest> {
		self.shared.requests.lock().unwrap().clone()
	}

	/// Trust a caller from now on. Returns its key string, for the app to persist.
	pub fn accept(&self, instance_id: &str) -> Option<String> {
		let mut reqs = self.shared.requests.lock().unwrap();
		let i = reqs.iter().position(|r| r.instance_id == instance_id)?;
		let req = reqs.remove(i);
		drop(reqs);
		let peer = parse_key_string(&req.key_string).ok()?;
		self.shared
			.callers
			.lock()
			.unwrap()
			.entry(peer.instance_id.clone())
			.or_insert(Caller { peer, x_public: None, mlkem_public: None, tx: None });
		Some(req.key_string)
	}

	pub fn deny(&self, instance_id: &str) {
		self.shared.requests.lock().unwrap().retain(|r| r.instance_id != instance_id);
	}

	/// Stop trusting a caller: drop their socket and any pending request. Reaching us again
	/// means asking again.
	pub fn forget(&self, instance_id: &str) {
		self.shared.callers.lock().unwrap().remove(instance_id);
		self.deny(instance_id);
	}

	/// Everyone accepted, and whether their socket is up right now.
	pub fn callers(&self) -> Vec<(String, String, bool)> {
		self.shared
			.callers
			.lock()
			.unwrap()
			.values()
			.map(|c| (c.peer.instance_id.clone(), c.peer.name.clone(), c.tx.is_some()))
			.collect()
	}

	/// Chat back down a caller's socket, sealed when it announced sealing keys.
	pub fn send_chat(&self, to: &str, text: &str, sent_at: Option<i64>) -> bool {
		self.shared.send_chat(to, text, sent_at)
	}

	pub fn stop(&self) {
		if !self.shared.closed.swap(true, Ordering::SeqCst) {
			if let Some(t) = self.task.lock().unwrap().take() {
				t.abort();
			}
			self.shared.callers.lock().unwrap().values_mut().for_each(|c| c.tx = None);
		}
	}
}

impl Drop for Listener {
	fn drop(&mut self) {
		self.stop();
	}
}

impl Shared {
	fn send_chat(&self, to: &str, text: &str, sent_at: Option<i64>) -> bool {
		let msg = Message::create(
			&self.me,
			"chat:message",
			to,
			json!({ "text": text, "fromName": self.me.name, "sentAt": sent_at.unwrap_or_else(now_ms) }),
		);
		self.send_wrapped(to, msg)
	}

	/// Send to a caller, sealed when it announced sealing keys, plain when it did not.
	fn send_wrapped(&self, to: &str, msg: Message) -> bool {
		let callers = self.callers.lock().unwrap();
		let Some(caller) = callers.get(to) else { return false };
		let Some(tx) = &caller.tx else { return false };
		let wire = match &caller.x_public {
			Some(x) => {
				let aad = format!("{}|{}", self.me.instance_id(), to);
				match seal(x, caller.mlkem_public.as_deref(), msg.to_json().as_bytes(), &aad) {
					Ok(sealed) => Message::create(&self.me, "sealed:direct", to, json!({ "box": sealed })),
					Err(_) => return false,
				}
			}
			None => msg,
		};
		tx.send(wire.to_json()).is_ok()
	}

	fn trusted(&self, id: &str) -> Option<PeerKeys> {
		self.callers.lock().unwrap().get(id).map(|c| c.peer.clone())
	}

	/// Verify against the caller's pinned key, with freshness and replay checks.
	fn verify(&self, msg: &Message) -> bool {
		let Some(peer) = self.trusted(&msg.from) else { return false };
		if msg.verify(&peer, now_ms()).is_err() {
			return false;
		}
		let now = now_ms();
		let mut seen = self.seen.lock().unwrap();
		seen.retain(|_, t| now - *t < 120_000);
		let key = format!("{}:{}", msg.from, msg.id);
		if seen.contains_key(&key) {
			return false;
		}
		seen.insert(key, now);
		true
	}

	/// What we say about ourselves in a `discover:response`.
	fn identity_payload(&self) -> Value {
		json!({
			"name": self.me.name,
			"publicKey": self.me.key_string(),
			"endpoint": "",
			"capabilities": [],
			"role": "peer",
			"kind": "client",
			"version": "3.0.0",
			"xPublicKey": self.me.x25519_public_b64(),
			"mlkemPublicKey": self.me.mlkem_public_b64(),
		})
	}

	/// Remember a caller's sealing keys from its announcement, so replies can be sealed.
	fn note_keys(&self, id: &str, payload: &Value) {
		if let Some(c) = self.callers.lock().unwrap().get_mut(id) {
			if let Some(x) = payload["xPublicKey"].as_str() {
				c.x_public = Some(x.to_string());
			}
			if let Some(k) = payload["mlkemPublicKey"].as_str() {
				c.mlkem_public = Some(k.to_string());
			}
		}
	}

	fn deliver(&self, msg: Message) {
		match msg.r#type.as_str() {
			"ping" => {
				let pong = Message::create(&self.me, "pong", &msg.from, json!({ "timestamp": now_ms() }));
				if let Some(tx) = self.callers.lock().unwrap().get(&msg.from).and_then(|c| c.tx.clone()) {
					let _ = tx.send(pong.to_json());
				}
			}
			"pong" | "tool:list:response" => {}
			"tool:list" => {
				let reply = Message::create(&self.me, "tool:list:response", &msg.from, json!([]));
				if let Some(tx) = self.callers.lock().unwrap().get(&msg.from).and_then(|c| c.tx.clone()) {
					let _ = tx.send(reply.to_json());
				}
			}
			"discover" => self.note_keys(&msg.from, &msg.payload),
			// An agent may delegate to a peer it trusts. This is a phone: there is nobody here to
			// think about it, and silence would leave the asker waiting forever, so refuse plainly.
			"task:delegate" => {
				let Some(task_id) = msg.payload["taskId"].as_str() else { return };
				let reply = Message::create(
					&self.me,
					"task:result",
					&msg.from,
					json!({
						"taskId": task_id,
						"status": "failed",
						"error": format!("{} is a vole-chat app, not an agent — there is no brain here to ask.", self.me.name),
					}),
				);
				self.send_wrapped(&msg.from, reply);
			}
			"chat:message" => {
				let Some(text) = msg.payload["text"].as_str() else { return };
				let name = self
					.trusted(&msg.from)
					.map(|p| p.name)
					.unwrap_or_else(|| msg.from[..8.min(msg.from.len())].to_string());
				let from_name = msg.payload["fromName"].as_str().unwrap_or(&name).to_string();
				let sent_at = match msg.payload["sentAt"].as_i64() {
					Some(t) if t > 0 && t <= msg.timestamp + 60_000 => t,
					_ => msg.timestamp,
				};
				let _ = self.events.send(Event::Chat {
					from: msg.from.clone(),
					from_name,
					text: text.to_string(),
					sent_at,
					id: msg.id,
				});
			}
			"sealed:direct" => {
				let Ok(sealed) = serde_json::from_value::<SealedBox>(msg.payload["box"].clone()) else { return };
				let aad = format!("{}|{}", msg.from, self.me.instance_id());
				let Ok(plain) = unseal(&self.me, &sealed, &aad) else { return };
				let Ok(text) = String::from_utf8(plain) else { return };
				let Some(inner) = Message::from_json(&text) else { return };
				if inner.from == msg.from && self.verify(&inner) {
					self.deliver(inner);
				}
			}
			_ => {}
		}
	}
}

/// Find this device's address on the local network, for the URL we hand out.
pub fn lan_ip() -> Option<String> {
	// The routing table knows which interface reaches the outside; no packet is actually sent.
	let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
	sock.connect("192.168.1.1:9").ok().or_else(|| sock.connect("8.8.8.8:53").ok())?;
	match sock.local_addr().ok()?.ip() {
		std::net::IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_unspecified() => Some(v4.to_string()),
		_ => None,
	}
}

// ── endpoints ─────────────────────────────────────────────────────────────

async fn info(State(s): State<Arc<Shared>>) -> impl IntoResponse {
	Json(json!({
		"ok": true,
		"protocol": "volenet",
		"version": 1,
		"publicKey": s.me.key_string(),
		"name": s.me.name,
		"kind": "client",
		"peers": [],
		"peerCount": 0,
	}))
}

async fn pair(State(s): State<Arc<Shared>>, Json(body): Json<Value>) -> impl IntoResponse {
	let Some(key) = body["publicKey"].as_str() else {
		return (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error":"invalid public key"})));
	};
	let Ok(peer) = parse_key_string(key) else {
		return (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error":"invalid public key"})));
	};
	if s.callers.lock().unwrap().contains_key(&peer.instance_id) {
		return (axum::http::StatusCode::OK, Json(json!({"ok":true,"alreadyTrusted":true})));
	}
	let name = body["name"].as_str().unwrap_or(&peer.name).chars().take(64).collect::<String>();
	let note = body["note"].as_str().map(|n| n.chars().take(200).collect::<String>());
	let mut reqs = s.requests.lock().unwrap();
	if reqs.len() >= 32 && !reqs.iter().any(|r| r.instance_id == peer.instance_id) {
		return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"too many pending requests"})));
	}
	reqs.retain(|r| r.instance_id != peer.instance_id);
	let req = PairRequest {
		instance_id: peer.instance_id.clone(),
		name: name.clone(),
		key_string: key.to_string(),
		note: note.clone(),
		at: now_ms(),
	};
	reqs.push(req);
	drop(reqs);
	let _ = s.events.send(Event::ConnectRequest { from: peer.instance_id, from_name: name, note });
	(axum::http::StatusCode::OK, Json(json!({"ok":true,"pending":true})))
}

/// A caller announces itself over HTTP; we answer inline, as core does, because it may be
/// behind NAT and unreachable to us.
async fn message(State(s): State<Arc<Shared>>, body: String) -> impl IntoResponse {
	let Some(msg) = Message::from_json(&body) else {
		return (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error":"Invalid message"})));
	};
	if !s.verify(&msg) {
		return (axum::http::StatusCode::UNAUTHORIZED, Json(json!({"error":"unverified"})));
	}
	if msg.r#type == "discover" {
		s.note_keys(&msg.from, &msg.payload);
		let reply = Message::create(&s.me, "discover:response", &msg.from, s.identity_payload());
		return (
			axum::http::StatusCode::OK,
			Json(json!({"ok": true, "response": serde_json::from_str::<Value>(&reply.to_json()).unwrap()})),
		);
	}
	s.deliver(msg);
	(axum::http::StatusCode::OK, Json(json!({"ok":true})))
}

async fn upgrade(State(s): State<Arc<Shared>>, ws: WebSocketUpgrade) -> impl IntoResponse {
	ws.on_upgrade(move |socket| socket_loop(socket, s))
}

/// One socket. The first verified message from an accepted caller binds it; anything else is
/// dropped, so an unknown or unaccepted caller gets nowhere.
async fn socket_loop(socket: WebSocket, s: Arc<Shared>) {
	let (mut sink, mut stream) = socket.split();
	let (tx, mut rx) = mpsc::unbounded_channel::<String>();
	let writer = tokio::spawn(async move {
		while let Some(text) = rx.recv().await {
			if sink.send(Ws::Text(text.into())).await.is_err() {
				break;
			}
		}
	});
	let mut bound: Option<String> = None;
	while let Some(Ok(frame)) = stream.next().await {
		let Ws::Text(text) = frame else { continue };
		let Some(msg) = Message::from_json(&text) else { continue };
		if !s.verify(&msg) {
			continue;
		}
		if bound.as_deref() != Some(msg.from.as_str()) {
			if let Some(c) = s.callers.lock().unwrap().get_mut(&msg.from) {
				c.tx = Some(tx.clone());
			}
			bound = Some(msg.from.clone());
		}
		s.deliver(msg);
	}
	if let Some(id) = bound {
		if let Some(c) = s.callers.lock().unwrap().get_mut(&id) {
			c.tx = None;
		}
	}
	writer.abort();
}
