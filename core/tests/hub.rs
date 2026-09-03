//! The whole client against a real OpenVole hub and a real member, both from core's own code.
//!
//! One sequence, because each phase builds on the last: join and pin the hub key; announce and
//! bind the socket; read the roster; chat both ways through the relay; write to a member who
//! has gone away and watch the hub hold it; deliver from the outbox when they return; go away
//! ourselves and come back to a notice of who tried.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;
use rust_lib_vole_chat::message::now_ms;
use rust_lib_vole_chat::{Event, HubClient, Identity, Outbox, Verdict};

// Tests in a file run in parallel, so each takes its own pair of ports.
const PORTS_MAIN: (u16, u16) = (19995, 19996);
const PORTS_REQUEST: (u16, u16) = (19993, 19994);

struct Harness {
	child: Child,
	stdin: std::process::ChildStdin,
	lines: std_mpsc::Receiver<Value>,
	pub hub_url: String,
	pub hub_key: String,
	pub member_id: String,
}

impl Harness {
	fn spawn(dir: &Path, ports: (u16, u16)) -> Harness {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
		let tsx: PathBuf = root.join("harness/node_modules/.bin/tsx");
		assert!(tsx.exists(), "run `pnpm install` in harness/ first");
		let mut child = Command::new(tsx)
			.arg(root.join("harness/hub.ts"))
			.arg(ports.0.to_string())
			.arg(ports.1.to_string())
			.arg(dir)
			.current_dir(root.join("harness"))
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit())
			.spawn()
			.expect("spawn harness hub");
		let stdin = child.stdin.take().unwrap();
		let stdout = child.stdout.take().unwrap();
		let (tx, lines) = std_mpsc::channel();
		std::thread::spawn(move || {
			for line in BufReader::new(stdout).lines().map_while(Result::ok) {
				if let Ok(v) = serde_json::from_str::<Value>(&line) {
					let _ = tx.send(v);
				}
			}
		});
		let mut h = Harness { child, stdin, lines, hub_url: String::new(), hub_key: String::new(), member_id: String::new() };
		let ready = h.wait_ev("ready", 40);
		h.hub_url = ready["hubUrl"].as_str().unwrap().to_string();
		h.hub_key = ready["hubKey"].as_str().unwrap().to_string();
		h.member_id = ready["memberId"].as_str().unwrap().to_string();
		h
	}

	fn cmd(&mut self, c: Value) {
		writeln!(self.stdin, "{c}").unwrap();
		self.stdin.flush().unwrap();
	}

	/// Next harness line with `ev == name` (others are discarded), within `secs`.
	fn wait_ev(&mut self, name: &str, secs: u64) -> Value {
		self.wait_where(secs, |v| v["ev"] == name)
	}

	fn wait_where(&mut self, secs: u64, pred: impl Fn(&Value) -> bool) -> Value {
		let deadline = Instant::now() + Duration::from_secs(secs);
		loop {
			let left = deadline.saturating_duration_since(Instant::now());
			match self.lines.recv_timeout(left) {
				Ok(v) if pred(&v) => return v,
				Ok(_) => continue,
				Err(_) => panic!("harness: nothing matching within {secs}s"),
			}
		}
	}
}

impl Drop for Harness {
	fn drop(&mut self) {
		let _ = writeln!(self.stdin, r#"{{"cmd":"exit"}}"#);
		let _ = self.stdin.flush();
		std::thread::sleep(Duration::from_millis(800));
		let _ = self.child.kill();
	}
}

/// Next client event matching `pred`, within `secs`.
async fn wait_event(rx: &mut UnboundedReceiver<Event>, secs: u64, pred: impl Fn(&Event) -> bool) -> Event {
	let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
	loop {
		let ev = tokio::time::timeout_at(deadline, rx.recv()).await.expect("client event within time").expect("event channel open");
		if pred(&ev) {
			return ev;
		}
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_on_a_real_hub() {
	let tmp = tempfile::tempdir().unwrap();
	let mut h = Harness::spawn(tmp.path(), PORTS_MAIN);
	let me = Arc::new(Identity::generate("rusty"));

	// ── §4 join: the hub trusts us and we pin its key ──
	let info = HubClient::join(&h.hub_url, &me).await.expect("join");
	assert!(!info.pending);
	assert_eq!(info.hub_key_string, h.hub_key, "the key we pin is the hub's own");
	assert_eq!(info.hub_name, "harness-hub");
	// Joining again is harmless.
	HubClient::join(&h.hub_url, &me).await.expect("re-join");

	// ── §5 announce + socket + bind; §6 roster ──
	let (client, mut rx) = HubClient::connect(&h.hub_url, &info.hub_key_string, me.clone(), [], []).await.expect("connect");
	assert!(matches!(wait_event(&mut rx, 10, |e| matches!(e, Event::Connected { .. })).await, Event::Connected { hub_name } if hub_name == "harness-hub"));
	let member_id = h.member_id.clone();
	wait_event(&mut rx, 20, |e| matches!(e, Event::Roster(m) if m.iter().any(|x| x.instance_id == member_id && x.connected))).await;
	let peer_b = client.member("peer-b").expect("peer-b on the roster");
	assert!(peer_b.x_public.is_some() && peer_b.mlkem_public.is_some(), "peer-b announces both sealing keys");
	assert!(!peer_b.key_string.is_empty());

	// ── §7 chat, us → them, through the relay ──
	client.accept(&member_id); // we'll accept their chat too
	let v = client.send_chat(&member_id, "hello from rust", None).await;
	assert_eq!(v, Verdict::Delivered, "the hub forwarded and acknowledged");
	let got = h.wait_where(15, |v| v["ev"] == "volenet:chat" && v["text"] == "hello from rust");
	assert_eq!(got["fromName"], json!("rusty"));
	assert_eq!(got["from"], json!(me.instance_id()));
	assert_eq!(got["relayed"], json!(true));

	// ── §7 chat, them → us ──
	h.cmd(json!({"cmd": "send", "to": me.instance_id(), "text": "hi rust"}));
	let sent = h.wait_ev("sent", 15);
	assert_eq!(sent["delivered"], json!(true), "{sent}");
	let ev = wait_event(&mut rx, 15, |e| matches!(e, Event::Chat { .. })).await;
	assert!(matches!(&ev, Event::Chat { from, from_name, text, .. } if from == &member_id && from_name == "peer-b" && text == "hi rust"), "{ev:?}");

	// ── they go away: the hub holds, we keep it in the outbox ──
	h.cmd(json!({"cmd": "stop-member"}));
	h.wait_ev("member-stopped", 15);
	wait_event(&mut rx, 60, |e| matches!(e, Event::Roster(m) if m.iter().any(|x| x.instance_id == member_id && !x.connected))).await;
	let written = now_ms();
	assert_eq!(client.send_chat(&member_id, "while you were out", Some(written)).await, Verdict::Held);
	let mut outbox = Outbox::new();
	outbox.add(&member_id, "peer-b", "while you were out", written);
	assert_eq!(outbox.len(), 1);
	// Flushing while they are still away does nothing (they are not connected in the roster).
	let r = outbox.flush(&client, now_ms(), rust_lib_vole_chat::outbox::DEFAULT_TTL_MS).await;
	assert!(r.is_empty(), "{r:?}");
	assert_eq!(outbox.len(), 1);

	// ── they come back: the roster says so, the outbox delivers, stamped with when it was written ──
	h.cmd(json!({"cmd": "start-member"}));
	h.wait_ev("member-started", 30);
	wait_event(&mut rx, 60, |e| matches!(e, Event::Roster(m) if m.iter().any(|x| x.instance_id == member_id && x.connected))).await;
	let r = outbox.flush(&client, now_ms(), rust_lib_vole_chat::outbox::DEFAULT_TTL_MS).await;
	assert!(matches!(r.as_slice(), [rust_lib_vole_chat::outbox::Flushed::Delivered { .. }]), "{r:?}");
	assert!(outbox.is_empty());
	let got = h.wait_where(20, |v| v["ev"] == "volenet:chat" && v["text"] == "while you were out");
	assert_eq!(got["timestamp"], json!(written), "the recipient records when it was written");
	// The hub also told them who tried while they were away; our delivery may have won that
	// race, but nothing must still read as pending once the message is there.
	h.cmd(json!({"cmd": "pending"}));
	let p = h.wait_ev("pending-list", 10);
	assert!(p["pending"].as_array().unwrap().iter().all(|n| n["from"] != json!(me.instance_id())), "{p}");

	// ── we go away: they write, the hub keeps only a notice, and tells us on return ──
	client.close().await;
	drop(client);
	// Give the hub a moment to see the socket drop, then have peer-b write to us.
	tokio::time::sleep(Duration::from_secs(2)).await;
	h.cmd(json!({"cmd": "send", "to": me.instance_id(), "text": "you missed this"}));
	let sent = h.wait_ev("sent", 15);
	assert_eq!(sent["queued"], json!(true), "peer-b's core held it in its own outbox: {sent}");

	// peer-b is online holding the message, so on our return it delivers straight away — and
	// the notice about the attempt must not survive the message it announced.
	let (client, mut rx) = HubClient::connect(&h.hub_url, &info.hub_key_string, me.clone(), [member_id.clone()], []).await.expect("reconnect");
	let ev = wait_event(&mut rx, 30, |e| matches!(e, Event::Chat { text, .. } if text == "you missed this")).await;
	assert!(matches!(ev, Event::Chat { .. }));
	tokio::time::sleep(Duration::from_millis(500)).await;
	assert!(client.pending().iter().all(|n| n.from != member_id), "notice cleared by the message: {:?}", client.pending());
	client.close().await;
}

/// The case a phone actually hits: an agent asks to connect while the phone is closed.
///
/// Before openvole 4.20.0 the request was sealed, handed to the hub and forgotten, so it
/// reached nobody and nobody was told. It must now wait on the agent and arrive when the phone
/// is back — which is also what proves the app's background-close is worth doing, since the
/// hub only holds once it knows the member is gone.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_sent_while_the_phone_is_closed_arrives_when_it_reopens() {
	let tmp = tempfile::tempdir().unwrap();
	let mut h = Harness::spawn(tmp.path(), PORTS_REQUEST);
	let me = Arc::new(Identity::generate("pocket"));
	let info = HubClient::join(&h.hub_url, &me).await.expect("join");
	let member_id = h.member_id.clone();

	// Be seen, then close the way the app does when the screen locks.
	let (client, mut rx) = HubClient::connect(&h.hub_url, &info.hub_key_string, me.clone(), [], []).await.expect("connect");
	wait_event(&mut rx, 20, |e| matches!(e, Event::Roster(m) if m.iter().any(|x| x.instance_id == member_id && x.connected))).await;
	client.close().await;
	drop(client);

	// The hub notices at once, because we closed rather than vanished.
	let me_id = me.instance_id().to_string();
	h.cmd(json!({"cmd": "connect", "to": me_id, "note": "it's your orchestrator"}));
	let res = h.wait_ev("connect-result", 20);
	assert_eq!(res["ok"], json!(true), "{res}");
	assert_eq!(res["queued"], json!(true), "the agent held it rather than losing it: {res}");
	h.cmd(json!({"cmd": "outbox"}));
	let ob = h.wait_ev("outbox", 10);
	let held = ob["outbox"].as_array().unwrap();
	assert_eq!(held.len(), 1, "{ob}");
	assert_eq!(held[0]["kind"], json!("connect-request"));
	assert_eq!(held[0]["note"], json!("it's your orchestrator"));

	// Reopen the app: the request is delivered and surfaces for the user to accept.
	let (client, mut rx) = HubClient::connect(&h.hub_url, &info.hub_key_string, me.clone(), [], []).await.expect("reconnect");
	let ev = wait_event(&mut rx, 40, |e| matches!(e, Event::ConnectRequest { .. })).await;
	assert!(
		matches!(&ev, Event::ConnectRequest { from, from_name, note }
			if from == &member_id && from_name == "peer-b" && note.as_deref() == Some("it's your orchestrator")),
		"{ev:?}"
	);

	// Accepting tells them, and the agent's outbox is empty again.
	client.accept(&member_id);
	assert_ne!(client.approve(&member_id).await, Verdict::Refused("not connected".into()));
	h.cmd(json!({"cmd": "outbox"}));
	let ob = h.wait_ev("outbox", 10);
	assert!(ob["outbox"].as_array().unwrap().is_empty(), "delivered, so nothing still waits: {ob}");
	client.close().await;
}
