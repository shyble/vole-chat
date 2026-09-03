//! Scanning someone's card takes the hub out of the trust path.
//!
//! A key string proves signatures but says nothing about sealing: the X25519 and ML-KEM keys
//! come from the hub's roster, so a hostile hub could hand out its own and read everything it
//! forwards. Those keys are far too big for a QR, so a card commits to them with a fingerprint
//! instead and the hub's copy is checked against it. This proves the check bites: a card
//! committing to somebody else's sealing keys stops the send outright.

use rust_lib_vole_chat::card::fingerprint;
use rust_lib_vole_chat::{ContactCard, Event, HubClient, Identity, Verdict};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

const PORTS: (u16, u16) = (19973, 19974);

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

	/// Nothing matching arrived in the window — used to assert a message did *not* get through.
	fn nothing(&mut self, secs: u64, pred: impl Fn(&Value) -> bool) -> bool {
		let deadline = Instant::now() + Duration::from_secs(secs);
		loop {
			let left = deadline.saturating_duration_since(Instant::now());
			if left.is_zero() {
				return true;
			}
			match self.lines.recv_timeout(left) {
				Ok(v) if pred(&v) => return false,
				Ok(_) => continue,
				Err(_) => return true,
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
async fn a_scanned_card_decides_whose_keys_are_accepted() {
	let tmp = tempfile::tempdir().unwrap();
	let mut h = Harness::spawn(tmp.path());
	let me = Arc::new(Identity::generate("scanner"));
	let info = HubClient::join(&h.hub_url_owned(), &me).await.expect("join");
	let member_id = h.ready["memberId"].as_str().unwrap().to_string();
	let member_key = h.ready["memberKey"].as_str().unwrap().to_string();
	let ed = member_key.split(' ').nth(1).unwrap().to_string();
	let mldsa = member_key.split(' ').nth(3).unwrap_or("").to_string();

	// Connect with nothing pinned first, to see what the hub vouches for.
	let (client, mut rx) =
		HubClient::connect(&h.hub_url_owned(), &info.hub_key_string, me.clone(), [member_id.clone()], [], [])
.await
			.expect("connect");
	wait_event(&mut rx, 20, |e| matches!(e, Event::Roster(m) if m.iter().any(|x| x.instance_id == member_id && x.connected))).await;
	let vouched = client.member(&member_id).expect("on the roster");
	let real_x = vouched.x_public.clone().expect("a real sealing key");
	let real_mlkem = vouched.mlkem_public.clone().unwrap_or_default();
	client.close().await;
	drop(client);

	// Their real card: the fingerprint commits to exactly those keys.
	let good = ContactCard {
		ed25519: ed.clone(),
		name: "peer-b".into(),
		x_public: real_x.clone(),
		mlkem_public: real_mlkem.clone(),
		fingerprint: fingerprint(&ed, &mldsa, &real_x, &real_mlkem),
		hub: None,
	};
	assert!(good.seals());
	assert_eq!(good.instance_id(), member_id);
	assert!(good.encode().len() < 2953, "a card has to fit in a QR: {} chars", good.encode().len());
	let (client, mut rx) = HubClient::connect(
		&h.hub_url_owned(),
		&info.hub_key_string,
		me.clone(),
		[member_id.clone()],
		[good],
		[],
	)
	.await
	.expect("connect pinned");
	wait_event(&mut rx, 20, |e| matches!(e, Event::Roster(m) if m.iter().any(|x| x.instance_id == member_id && x.connected))).await;
	assert_eq!(client.send_chat(&member_id, "verified and delivered", None).await, Verdict::Delivered);
	let got = h.wait(15, |v| v["ev"] == "volenet:chat" && v["text"] == "verified and delivered");
	assert_eq!(got["from"], json!(me.instance_id()));
	client.close().await;
	drop(client);

	// The same identity, but the card commits to somebody else's sealing keys — which is what
	// a hostile hub substituting its own would look like from our side. The keys the hub offers
	// no longer match, so nothing is sent at all and the reason says why.
	let impostor = Identity::generate("impostor");
	// The card carries the impostor's sealing keys and a fingerprint that agrees with them, so
	// it is internally consistent — and still contradicts what the hub says this identity uses.
	let bad = ContactCard {
		ed25519: ed.clone(),
		name: "peer-b".into(),
		x_public: impostor.x25519_public_b64(),
		mlkem_public: impostor.mlkem_public_b64(),
		fingerprint: fingerprint(&ed, &mldsa, &impostor.x25519_public_b64(), &impostor.mlkem_public_b64()),
		hub: None,
	};
	let (client, mut rx) =
		HubClient::connect(&h.hub_url_owned(), &info.hub_key_string, me.clone(), [member_id.clone()], [bad], [])
			.await
			.expect("connect mispinned");
	wait_event(&mut rx, 20, |e| matches!(e, Event::Roster(m) if m.iter().any(|x| x.instance_id == member_id && x.connected))).await;
	match client.send_chat(&member_id, "should never leave", None).await {
		Verdict::Refused(reason) => assert!(
			reason.contains("passed off"),
			"refused for the right reason, got: {reason}"
		),
		other => panic!("a mismatched card must stop the send, got {other:?}"),
	}
	assert!(
		h.nothing(4, |v| v["ev"] == "volenet:chat" && v["text"] == "should never leave"),
		"and nothing reached them"
	);
	client.close().await;
}

impl Harness {
	fn hub_url_owned(&self) -> String {
		self.ready["hubUrl"].as_str().unwrap().to_string()
	}
}
