//! Direct to a node, no hub: fingerprint it, ask to be trusted, get refused until the operator
//! accepts, then connect and chat both ways, sealed end to end — against a real OpenVole node
//! (harness/hub.ts's peer-b, which also listens on its own port).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;
use rust_lib_vole_chat::client::ClientError;
use rust_lib_vole_chat::{node_info, node_pair, DirectClient, Event, Identity, Verdict};

const HUB_PORT: u16 = 19985;
const MEMBER_PORT: u16 = 19986;

struct Harness {
	child: Child,
	stdin: std::process::ChildStdin,
	lines: std_mpsc::Receiver<Value>,
	ready: Value,
}

impl Harness {
	fn spawn(dir: &Path) -> Harness {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
		let tsx: PathBuf = root.join("harness/node_modules/.bin/tsx");
		let mut child = Command::new(tsx)
			.arg(root.join("harness/hub.ts"))
			.arg(HUB_PORT.to_string())
			.arg(MEMBER_PORT.to_string())
			.arg(dir)
			.current_dir(root.join("harness"))
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit())
			.spawn()
			.expect("spawn harness");
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
		let mut h = Harness { child, stdin, lines, ready: Value::Null };
		h.ready = h.wait(40, |v| v["ev"] == "ready");
		h
	}

	fn cmd(&mut self, c: Value) {
		writeln!(self.stdin, "{c}").unwrap();
		self.stdin.flush().unwrap();
	}

	fn wait(&mut self, secs: u64, pred: impl Fn(&Value) -> bool) -> Value {
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

async fn wait_event(rx: &mut UnboundedReceiver<Event>, secs: u64, pred: impl Fn(&Event) -> bool) -> Event {
	let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
	loop {
		let ev = tokio::time::timeout_at(deadline, rx.recv()).await.expect("event within time").expect("channel open");
		if pred(&ev) {
			return ev;
		}
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_straight_to_a_node() {
	let tmp = tempfile::tempdir().unwrap();
	let mut h = Harness::spawn(tmp.path());
	let url = h.ready["memberUrl"].as_str().unwrap().to_string();
	let node_id = h.ready["memberId"].as_str().unwrap().to_string();
	let me = Arc::new(Identity::generate("rusty-phone"));

	// Fingerprint first: the key we will pin is the node's own.
	let info = node_info(&url).await.expect("info");
	assert_eq!(info.key_string, h.ready["memberKey"].as_str().unwrap());
	assert_eq!(info.instance_id, node_id);
	assert_eq!(info.name.as_deref(), Some("peer-b"), "the node publishes its name");
	assert!(info.post_quantum);

	// Nothing is trusted yet: connecting is refused, cleanly.
	match DirectClient::connect(&url, &info.key_string, me.clone()).await {
		Err(ClientError::NotTrusted) => {}
		other => panic!("expected NotTrusted before pairing, got {:?}", other.map(|_| ())),
	}

	// Ask, and the request lands on the node's operator.
	let pair = node_pair(&url, &me, Some("it's my phone")).await.expect("pair");
	assert!(pair.queued && !pair.already_trusted);
	let req = h.wait(15, |v| v["ev"] == "volenet:pair:request");
	assert_eq!(req["fromName"], json!("rusty-phone"));
	assert_eq!(req["note"], json!("it's my phone"));
	h.cmd(json!({"cmd": "pair-list"}));
	let list = h.wait(10, |v| v["ev"] == "pair-list");
	assert!(list["requests"].as_array().unwrap().iter().any(|r| r["id"] == json!(me.instance_id())));

	// The operator accepts; asking again is now a no-op.
	h.cmd(json!({"cmd": "pair-accept", "to": "rusty-phone"}));
	let acc = h.wait(10, |v| v["ev"] == "pair-accept-result");
	assert_eq!(acc["ok"], json!(true), "{acc}");
	let again = node_pair(&url, &me, None).await.expect("pair again");
	assert!(again.already_trusted);

	// Connect straight to it.
	let (client, mut rx) = DirectClient::connect(&url, &info.key_string, me.clone()).await.expect("connect");
	assert!(matches!(wait_event(&mut rx, 10, |e| matches!(e, Event::Connected { .. })).await, Event::Connected { hub_name } if hub_name == "peer-b"));
	assert!(client.seals(), "peer-b announced sealing keys, so chat is sealed end to end");
	assert_eq!(client.node_instance_id(), node_id);

	// us → node, sealed:direct — the node unwraps it and records an ordinary chat.
	assert_eq!(client.send_chat("straight to you", None), Verdict::Delivered);
	let got = h.wait(15, |v| v["ev"] == "volenet:chat" && v["text"] == "straight to you");
	assert_eq!(got["fromName"], json!("rusty-phone"));
	assert_eq!(got["from"], json!(me.instance_id()));
	assert_ne!(got["relayed"], json!(true), "no relay was involved");

	// node → us, over the socket we opened.
	h.cmd(json!({"cmd": "send", "to": me.instance_id(), "text": "back at you"}));
	let sent = h.wait(15, |v| v["ev"] == "sent");
	assert_eq!(sent["delivered"], json!(true), "{sent}");
	let ev = wait_event(&mut rx, 15, |e| matches!(e, Event::Chat { .. })).await;
	assert!(matches!(&ev, Event::Chat { from, from_name, text, .. } if from == &node_id && from_name == "peer-b" && text == "back at you"), "{ev:?}");

	client.close().await;
}
