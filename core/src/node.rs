//! A direct session with one node — no hub (PROTOCOL.md §4–§5, applied to a peer).
//!
//! A phone cannot be dialed, but it can dial an agent exactly the way agents dial each other:
//! learn the node's key from `/volenet/info`, ask to be trusted with `/volenet/pair`, and once
//! the operator approves, announce and bind a socket to it. Chat then goes straight to the node,
//! sealed end to end with `sealed:direct`, which every agent unwraps transparently. Nothing in
//! between: no relay, no roster, no notices — and if the node is down, so is the link, so the
//! app's outbox holds what was written until it is back.

use crate::client::{ClientError, Event, Verdict, PING_INTERVAL, SILENCE_LIMIT_MS};
use crate::keys::{parse_key_string, PeerKeys};
use crate::message::{now_ms, Message};
use crate::seal::{seal, unseal, SealedBox};
use crate::Identity;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as Ws;

/// What `/volenet/info` says about a node before anything is trusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeInfo {
	pub key_string: String,
	pub instance_id: String,
	/// Only when the node publishes names.
	pub name: Option<String>,
	pub post_quantum: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairOutcome {
	/// The node already trusts this identity — connect straight away.
	pub already_trusted: bool,
	/// The request is queued for the node's operator.
	pub queued: bool,
}

fn http() -> Result<reqwest::Client, ClientError> {
	reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| ClientError::Http(e.to_string()))
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

/// Fingerprint a node before trusting it: its key string, as it will sign everything.
pub async fn node_info(url: &str) -> Result<NodeInfo, ClientError> {
	let base = url.trim_end_matches('/');
	let v: Value = http()?
		.get(format!("{base}/volenet/info"))
		.send()
		.await
		.map_err(|e| ClientError::Http(e.to_string()))?
		.json()
		.await
		.map_err(|e| ClientError::Protocol(format!("info: {e}")))?;
	let key = v["publicKey"].as_str().ok_or_else(|| ClientError::Protocol("the node publishes no key".into()))?;
	let p = parse_key_string(key)?;
	Ok(NodeInfo {
		key_string: key.to_string(),
		instance_id: p.instance_id,
		name: v["name"].as_str().map(str::to_string),
		post_quantum: p.mldsa.is_some(),
	})
}

/// Ask a node's operator to trust us. Nothing is trusted until they accept.
pub async fn node_pair(url: &str, me: &Identity, note: Option<&str>) -> Result<PairOutcome, ClientError> {
	let base = url.trim_end_matches('/');
	let mut body = json!({ "publicKey": me.key_string(), "name": me.name });
	if let Some(n) = note {
		body["note"] = json!(n);
	}
	let res = http()?
		.post(format!("{base}/volenet/pair"))
		.json(&body)
		.send()
		.await
		.map_err(|e| ClientError::Http(e.to_string()))?;
	let status = res.status();
	let v: Value = res.json().await.unwrap_or(Value::Null);
	if status.as_u16() == 404 {
		return Err(ClientError::Refused("that node does not accept pair requests".into()));
	}
	if !status.is_success() {
		return Err(ClientError::Refused(v["error"].as_str().unwrap_or("pair request refused").to_string()));
	}
	let already = v["alreadyTrusted"].as_bool() == Some(true);
	Ok(PairOutcome { already_trusted: already, queued: !already })
}

struct Inner {
	me: Arc<Identity>,
	node: PeerKeys,
	node_name: Mutex<String>,
	node_x: Option<String>,
	node_mlkem: Option<String>,
	/// The other end announced itself as a chat client, so it has no brain to ask.
	node_is_client: bool,
	tx: mpsc::UnboundedSender<String>,
	events: mpsc::UnboundedSender<Event>,
	seen: Mutex<HashMap<String, i64>>,
	last_rx: AtomicI64,
	closed: AtomicBool,
}

pub struct DirectClient {
	inner: Arc<Inner>,
	tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl DirectClient {
	/// Announce ourselves to the node and bind a socket. Fails with `NotTrusted` until the
	/// operator has accepted our pair request.
	pub async fn connect(
		url: &str,
		node_key_string: &str,
		me: Arc<Identity>,
	) -> Result<(DirectClient, mpsc::UnboundedReceiver<Event>), ClientError> {
		let node = parse_key_string(node_key_string)?;
		let base = url.trim_end_matches('/').to_string();
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
			return Err(ClientError::NotTrusted);
		}
		if !res.status().is_success() {
			return Err(ClientError::Http(format!("discover: HTTP {}", res.status())));
		}
		let body: Value = res.json().await.map_err(|e| ClientError::Protocol(format!("discover: {e}")))?;
		let reply = body.get("response").cloned().ok_or(ClientError::NotTrusted)?;
		let reply: Message = serde_json::from_value(reply).map_err(|e| ClientError::Protocol(format!("discover:response: {e}")))?;
		reply.verify(&node, now_ms()).map_err(|e| ClientError::Protocol(format!("the node's discover:response failed to verify: {e}")))?;
		let node_name = reply.payload["name"].as_str().unwrap_or(&node.name).to_string();
		let advertised = reply.payload["endpoint"].as_str().unwrap_or("").to_string();
		let socket_base = if advertised.starts_with("http") { advertised } else { base.clone() };
		let node_x = reply.payload["xPublicKey"].as_str().map(str::to_string);
		let node_mlkem = reply.payload["mlkemPublicKey"].as_str().map(str::to_string);
		let node_is_client = reply.payload["kind"].as_str() == Some("client");

		let (ws, _) = tokio_tungstenite::connect_async(ws_url(&socket_base))
			.await
			.map_err(|e| ClientError::Http(format!("websocket: {e}")))?;
		let (mut sink, mut stream) = ws.split();
		let (tx, mut rx) = mpsc::unbounded_channel::<String>();
		let (events, event_rx) = mpsc::unbounded_channel::<Event>();
		let inner = Arc::new(Inner {
			me,
			node,
			node_name: Mutex::new(node_name.clone()),
			node_x,
			node_mlkem,
			node_is_client,
			tx,
			events,
			seen: Mutex::new(HashMap::new()),
			last_rx: AtomicI64::new(now_ms()),
			closed: AtomicBool::new(false),
		});
		inner.send(&Message::create(&inner.me, "ping", inner.node.instance_id.as_str(), json!({ "timestamp": now_ms() })));

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
						inner.disconnect("node silent for 45 s");
						break;
					}
					inner.send(&Message::create(&inner.me, "ping", inner.node.instance_id.as_str(), json!({ "timestamp": now_ms() })));
				}
			})
		};
		let _ = inner.events.send(Event::Connected { hub_name: node_name });
		Ok((DirectClient { inner, tasks: Mutex::new(vec![writer, reader, keepalive]) }, event_rx))
	}

	pub fn instance_id(&self) -> &str {
		self.inner.me.instance_id()
	}

	pub fn node_instance_id(&self) -> &str {
		&self.inner.node.instance_id
	}

	pub fn node_name(&self) -> String {
		self.inner.node_name.lock().unwrap().clone()
	}

	pub fn is_connected(&self) -> bool {
		!self.inner.closed.load(Ordering::SeqCst)
	}

	/// Whether chat to this node is sealed end to end (it announced sealing keys) or, for an
	/// older node, signed but readable on the wire.
	pub fn seals(&self) -> bool {
		self.inner.node_x.is_some()
	}

	/// Whether the other end is an agent at all. Another phone announces itself as a client;
	/// asking it to think would go unanswered but for the refusal it sends back.
	pub fn has_brain(&self) -> bool {
		!self.inner.node_is_client
	}

	/// Ask the node's **brain**, rather than leaving a message for whoever runs it.
	///
	/// This is `task:delegate` with a `fromName`, which is what one agent sends another: the
	/// node frames it as a peer message, runs it through its own brain in a session kept per
	/// peer — so the conversation has continuity — and answers with `task:result`.
	///
	/// The node decides whether to allow it. Until its operator sets `allowBrain: true` for this
	/// identity, the answer is a refusal saying so. An answer can take as long as a model takes,
	/// and arrives as [`Event::BrainReply`] carrying the id returned here.
	pub fn ask_brain(&self, text: &str) -> Result<String, ClientError> {
		let inner = &self.inner;
		if inner.closed.load(Ordering::SeqCst) {
			return Err(ClientError::NotConnected);
		}
		if inner.node_is_client {
			return Err(ClientError::Refused("that is a chat app, not an agent — it has no brain".into()));
		}
		let task_id = uuid::Uuid::new_v4().to_string();
		let msg = Message::create(
			&inner.me,
			"task:delegate",
			&inner.node.instance_id,
			json!({ "taskId": task_id, "input": text, "fromName": inner.me.name }),
		);
		inner.send_wrapped(&msg);
		Ok(task_id)
	}

	/// Chat straight to the node. There is no verdict for direct delivery: a successful write to
	/// a live socket is the delivery, as it is between agents.
	pub fn send_chat(&self, text: &str, sent_at: Option<i64>) -> Verdict {
		let inner = &self.inner;
		if inner.closed.load(Ordering::SeqCst) {
			return Verdict::Refused("not connected".into());
		}
		let msg = Message::create(
			&inner.me,
			"chat:message",
			&inner.node.instance_id,
			json!({ "text": text, "fromName": inner.me.name, "sentAt": sent_at.unwrap_or_else(now_ms) }),
		);
		if !inner.send_wrapped(&msg) {
			return Verdict::Refused("not connected".into());
		}
		Verdict::Delivered
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

	/// Send sealed when the node announced sealing keys, plain when it is older.
	fn send_wrapped(&self, m: &Message) -> bool {
		let wire = match &self.node_x {
			Some(x) => {
				let aad = format!("{}|{}", self.me.instance_id(), self.node.instance_id);
				match seal(x, self.node_mlkem.as_deref(), m.to_json().as_bytes(), &aad) {
					Ok(sealed) => {
						Message::create(&self.me, "sealed:direct", &self.node.instance_id, json!({ "box": sealed }))
					}
					Err(_) => return false,
				}
			}
			None => m.clone(),
		};
		self.tx.send(wire.to_json()).is_ok()
	}

	fn disconnect(&self, reason: &str) {
		if !self.closed.swap(true, Ordering::SeqCst) {
			let _ = self.events.send(Event::Disconnected { reason: reason.to_string() });
		}
	}

	fn on_text(&self, text: &str) {
		let Some(msg) = Message::from_json(text) else { return };
		self.on_message(msg);
	}

	/// Only the node may speak on this socket, and only with a valid, fresh, unseen signature.
	fn on_message(&self, msg: Message) {
		if msg.from != self.node.instance_id || msg.verify(&self.node, now_ms()).is_err() || !self.first_time(&msg.id) {
			return;
		}
		self.last_rx.store(now_ms(), Ordering::SeqCst);
		match msg.r#type.as_str() {
			"ping" => self.send(&Message::create(&self.me, "pong", &self.node.instance_id, json!({ "timestamp": now_ms() }))),
			"tool:list" => self.send(&Message::create(&self.me, "tool:list:response", &self.node.instance_id, json!([]))),
			"chat:message" => {
				let Some(text) = msg.payload["text"].as_str() else { return };
				let from_name = msg.payload["fromName"].as_str().map(str::to_string).unwrap_or_else(|| self.node_name.lock().unwrap().clone());
				let sent_at = match msg.payload["sentAt"].as_i64() {
					Some(t) if t > 0 && t <= msg.timestamp + 60_000 => t,
					_ => msg.timestamp,
				};
				let _ = self.events.send(Event::Chat { from: msg.from.clone(), from_name, text: text.to_string(), sent_at, id: msg.id });
			}
			"task:result" => {
				let Some(task_id) = msg.payload["taskId"].as_str() else { return };
				let failed = msg.payload["status"].as_str().unwrap_or("failed") != "completed";
				let text = msg.payload["result"]
					.as_str()
					.or_else(|| msg.payload["error"].as_str())
					.unwrap_or("(no answer)")
					.to_string();
				let _ = self.events.send(Event::BrainReply { task_id: task_id.to_string(), text, failed });
			}
			"sealed:direct" => {
				let Ok(sealed) = serde_json::from_value::<SealedBox>(msg.payload["box"].clone()) else { return };
				let aad = format!("{}|{}", self.node.instance_id, self.me.instance_id());
				let Ok(plain) = unseal(&self.me, &sealed, &aad) else { return };
				let Ok(text) = String::from_utf8(plain) else { return };
				if let Some(inner) = Message::from_json(&text) {
					self.on_message(inner);
				}
			}
			_ => {}
		}
	}

	fn first_time(&self, id: &str) -> bool {
		let now = now_ms();
		let mut seen = self.seen.lock().unwrap();
		seen.retain(|_, t| now - *t < 120_000);
		if seen.contains_key(id) {
			return false;
		}
		seen.insert(id.to_string(), now);
		true
	}
}

impl Drop for DirectClient {
	fn drop(&mut self) {
		self.inner.closed.store(true, Ordering::SeqCst);
		for t in self.tasks.lock().unwrap().drain(..) {
			t.abort();
		}
	}
}
