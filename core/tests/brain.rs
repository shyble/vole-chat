//! Asking an agent's brain from a phone, over a direct link.
//!
//! An agent answers a peer's question with its own brain when it is asked as `task:delegate`
//! carrying a `fromName` — the same thing one agent sends another. The agent frames it as a peer
//! message, runs it in a session kept per peer so the conversation has continuity, and replies
//! with `task:result`.
//!
//! Its operator decides. Until they grant this identity brain access the answer is a refusal
//! that says so, which is the first thing to check when it does not work.
//!
//! This is a **direct** link only. A hub refuses to relay anything executable — its allowlist is
//! chat, the consent handshake and file control — so a brain cannot be reached through one.

use rust_lib_vole_chat::{node_info, node_pair, DirectClient, Event, Identity};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

const PORTS: (u16, u16) = (19957, 19958);

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
			.arg(PORTS.0.to_string())
			.arg(PORTS.1.to_string())
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
		std::thread::sleep(Duration::from_millis(600));
		let _ = self.child.kill();
	}
}

async fn brain_reply(rx: &mut UnboundedReceiver<Event>, secs: u64) -> (String, String, bool) {
	let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
	loop {
		let ev = tokio::time::timeout_at(deadline, rx.recv())
			.await
			.expect("an answer within time")
			.expect("channel open");
		if let Event::BrainReply { task_id, text, failed } = ev {
			return (task_id, text, failed);
		}
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_asks_an_agents_brain() {
	let tmp = tempfile::tempdir().unwrap();
	let mut h = Harness::spawn(tmp.path());
	let url = h.ready["memberUrl"].as_str().unwrap().to_string();
	let me = Arc::new(Identity::generate("pocket"));

	// Pair the way a phone does, then open the direct link.
	let info = node_info(&url).await.expect("info");
	node_pair(&url, &me, Some("my phone")).await.expect("pair");
	h.wait(15, |v| v["ev"] == "volenet:pair:request");
	h.cmd(json!({"cmd": "pair-accept", "to": "pocket"}));
	h.wait(10, |v| v["ev"] == "pair-accept-result");
	let (client, mut rx) = DirectClient::connect(&url, &info.key_string, me.clone()).await.expect("connect");
	tokio::time::sleep(Duration::from_secs(1)).await;

	// Brain access is off, as on any agent until its operator grants it. The refusal names
	// the setting, which is the whole value of asking rather than timing out.
	let asked = client.ask_brain("what are you working on?").expect("sent");
	let (task_id, text, failed) = brain_reply(&mut rx, 25).await;
	assert_eq!(task_id, asked, "the answer carries the id we were given");
	assert!(failed, "refused, since nobody granted access: {text}");
	assert!(text.contains("allowBrain"), "and it says how to grant it: {text}");

	// The operator grants it.
	h.cmd(json!({"cmd": "allow-brain"}));
	h.wait(10, |v| v["ev"] == "allow-brain-result");

	// Now the request is accepted and run. This harness agent carries no brain of its own, so
	// what comes back says exactly that — which still proves the question reached the place
	// where a real agent would think about it, past every gate.
	let asked = client.ask_brain("what are you working on?").expect("sent");
	let (task_id, text, _failed) = brain_reply(&mut rx, 25).await;
	assert_eq!(task_id, asked);
	assert!(
		!text.contains("allowBrain"),
		"past the gate now, so the answer is about running it, not permission: {text}"
	);
	client.close().await;
}
