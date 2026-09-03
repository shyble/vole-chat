//! Two vole-chat clients on one hub, adding each other by card — the phone-to-phone case.
//!
//! Presence is easy and was never the problem; delivery is where the ordering bites. A client
//! that asks to connect the instant its session opens is asking before the hub has sent it a
//! roster, so it does not yet know the member exists, and the request goes nowhere. The other
//! side then never consents, and every message it is sent is dropped as unsolicited.

use rust_lib_vole_chat::{ContactCard, Event, HubClient, Identity, Verdict};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

// Tests in a file run in parallel, so each hub takes its own pair of ports.
const PORTS_EARLY: (u16, u16) = (19971, 19972);
const PORTS_BOTH: (u16, u16) = (19969, 19970);
const PORTS_UNACCEPTED: (u16, u16) = (19967, 19968);
const PORTS_DOUBLE: (u16, u16) = (19965, 19966);
const PORTS_RECONNECT: (u16, u16) = (19963, 19964);
const PORTS_FORGOTTEN: (u16, u16) = (19961, 19962);

struct Hub {
	child: Child,
	stdin: std::process::ChildStdin,
	url: String,
	key: String,
}

impl Hub {
	fn spawn(dir: &Path, ports: (u16, u16)) -> Hub {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
		let tsx: PathBuf = root.join("harness/node_modules/.bin/tsx");
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
		let deadline = Instant::now() + Duration::from_secs(40);
		loop {
			let v = lines.recv_timeout(deadline.saturating_duration_since(Instant::now())).expect("ready");
			if v["ev"] == "ready" {
				return Hub {
					child,
					stdin,
					url: v["hubUrl"].as_str().unwrap().to_string(),
					key: v["hubKey"].as_str().unwrap().to_string(),
				};
			}
		}
	}
}

impl Drop for Hub {
	fn drop(&mut self) {
		let _ = writeln!(self.stdin, r#"{{"cmd":"exit"}}"#);
		let _ = self.stdin.flush();
		std::thread::sleep(Duration::from_millis(600));
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

/// Wait until the hub has told us `who` exists. Everything addressed by identity needs this.
async fn until_rostered(client: &HubClient, who: &str, secs: u64) -> bool {
	let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
	while tokio::time::Instant::now() < deadline {
		if client.member(who).is_some() {
			return true;
		}
		tokio::time::sleep(Duration::from_millis(100)).await;
	}
	false
}

#[tokio::test(flavor = "multi_thread")]
async fn asking_to_connect_before_the_roster_arrives_reaches_nobody() {
	let tmp = tempfile::tempdir().unwrap();
	let hub = Hub::spawn(tmp.path(), PORTS_EARLY);
	let a = Arc::new(Identity::generate("phone-a"));
	let b = Arc::new(Identity::generate("phone-b"));
	HubClient::join(&hub.url, &a).await.expect("a joins");
	HubClient::join(&hub.url, &b).await.expect("b joins");

	let (ca, _ra) = HubClient::connect(&hub.url, &hub.key, a.clone(), [], [], [])
.await.expect("a connects");
	// This is what the app did: ask the moment the session opens.
	let v = ca.request_connect(b.instance_id(), Some("scanned your card")).await;
	assert!(
		matches!(&v, Verdict::Refused(r) if r.contains("no such member")),
		"the roster has not arrived yet, so there is nobody to ask: {v:?}"
	);
	ca.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_clients_add_each_other_by_card_and_chat() {
	let tmp = tempfile::tempdir().unwrap();
	let hub = Hub::spawn(tmp.path(), PORTS_BOTH);
	let a = Arc::new(Identity::generate("phone-a"));
	let b = Arc::new(Identity::generate("phone-b"));
	HubClient::join(&hub.url, &a).await.expect("a joins");
	HubClient::join(&hub.url, &b).await.expect("b joins");

	// Each scans the other's code.
	let card_a = ContactCard::mine(&a, Some(&hub.url));
	let card_b = ContactCard::mine(&b, Some(&hub.url));
	let (ca, mut ra) = HubClient::connect(&hub.url, &hub.key, a.clone(), [], [card_b.clone()], []).await.expect("a");
	let (cb, mut rb) = HubClient::connect(&hub.url, &hub.key, b.clone(), [], [card_a.clone()], []).await.expect("b");

	// Wait to be told the other exists — the step the app was skipping.
	assert!(until_rostered(&ca, b.instance_id(), 30).await, "a sees b");
	assert!(until_rostered(&cb, a.instance_id(), 30).await, "b sees a");

	// A asks. B is told, and accepts.
	assert_eq!(ca.request_connect(b.instance_id(), Some("scanned your card")).await, Verdict::Delivered);
	let ev = wait_event(&mut rb, 20, |e| matches!(e, Event::ConnectRequest { .. })).await;
	assert!(
		matches!(&ev, Event::ConnectRequest { from, note, .. }
			if from == a.instance_id() && note.as_deref() == Some("scanned your card")),
		"{ev:?}"
	);
	cb.accept(a.instance_id());
	assert_eq!(cb.approve(a.instance_id()).await, Verdict::Delivered);
	wait_event(&mut ra, 20, |e| matches!(e, Event::ConnectAccepted { from, .. } if from == b.instance_id())).await;

	// Now both directions carry, with the keys each pinned from the other's code.
	assert_eq!(ca.send_chat(b.instance_id(), "hello from a", None).await, Verdict::Delivered);
	let ev = wait_event(&mut rb, 20, |e| matches!(e, Event::Chat { .. })).await;
	assert!(matches!(&ev, Event::Chat { from, text, .. } if from == a.instance_id() && text == "hello from a"), "{ev:?}");

	assert_eq!(cb.send_chat(a.instance_id(), "hello from b", None).await, Verdict::Delivered);
	let ev = wait_event(&mut ra, 20, |e| matches!(e, Event::Chat { .. })).await;
	assert!(matches!(&ev, Event::Chat { from, text, .. } if from == b.instance_id() && text == "hello from b"), "{ev:?}");

	ca.close().await;
	cb.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_from_someone_unaccepted_surfaces_as_a_request_not_a_chat() {
	let tmp = tempfile::tempdir().unwrap();
	let hub = Hub::spawn(tmp.path(), PORTS_UNACCEPTED);
	let a = Arc::new(Identity::generate("sender"));
	let b = Arc::new(Identity::generate("recipient"));
	HubClient::join(&hub.url, &a).await.expect("a joins");
	HubClient::join(&hub.url, &b).await.expect("b joins");
	let (ca, _ra) = HubClient::connect(&hub.url, &hub.key, a.clone(), [b.instance_id().to_string()], [], [])
.await.expect("a");
	let (cb, mut rb) = HubClient::connect(&hub.url, &hub.key, b.clone(), [], [], [])
.await.expect("b");
	assert!(until_rostered(&ca, b.instance_id(), 30).await);

	// The hub forwards it and says so; the recipient still holds it, because nobody agreed to
	// hear from this identity yet. That is why a request has to actually arrive.
	assert_eq!(ca.send_chat(b.instance_id(), "let me in", None).await, Verdict::Delivered);
	let ev = wait_event(&mut rb, 20, |e| matches!(e, Event::ConnectRequest { .. })).await;
	assert!(matches!(&ev, Event::ConnectRequest { from, .. } if from == a.instance_id()), "{ev:?}");
	ca.close().await;
	cb.close().await;
}

/// Two sessions from one identity, which is what the app did when scanning a card also joined
/// a hub: the join connected, and applying the pinned card connected again before the first
/// had finished. The hub keeps one socket per peer and drops the other — which one depends on
/// how the two instance ids compare, so one phone would work and the other would not.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_session_from_the_same_identity_leaves_one_of_them_deaf() {
	let tmp = tempfile::tempdir().unwrap();
	let hub = Hub::spawn(tmp.path(), PORTS_DOUBLE);
	let me = Arc::new(Identity::generate("double"));
	let other = Arc::new(Identity::generate("other"));
	HubClient::join(&hub.url, &me).await.expect("join");
	HubClient::join(&hub.url, &other).await.expect("other joins");

	let (first, _r1) = HubClient::connect(&hub.url, &hub.key, me.clone(), [], [], [])
.await.expect("first");
	let (second, _r2) = HubClient::connect(&hub.url, &hub.key, me.clone(), [], [], [])
.await.expect("second");
	// Give the hub time to bind, roster, and resolve the duplicate.
	let (_o, _ro) = HubClient::connect(&hub.url, &hub.key, other.clone(), [], [], [])
.await.expect("other");
	tokio::time::sleep(Duration::from_secs(3)).await;

	// Both sessions believe they are up, because each opened a socket and got a reply. But only
	// one of them is the socket the hub kept, so only one is ever told who is there.
	let saw_first = first.member(other.instance_id()).is_some();
	let saw_second = second.member(other.instance_id()).is_some();
	assert!(
		!(saw_first && saw_second),
		"the hub keeps one socket per peer, so two sessions cannot both be served"
	);
	assert!(saw_first || saw_second, "one of them should have been served");
	first.close().await;
	second.close().await;
}

/// Closing and immediately reconnecting, which is what applying a scanned card did.
///
/// A hub keeps one socket per peer. If our close has not reached it yet, the socket it still
/// believes in looks live, and it may drop the new one instead — choosing by how the two
/// instance ids compare, so it fails on some devices and not others. The new session then sits
/// there connected, never rostered, answering every send with "no such member".
#[tokio::test(flavor = "multi_thread")]
async fn reconnecting_at_once_still_gets_a_roster() {
	let tmp = tempfile::tempdir().unwrap();
	let hub = Hub::spawn(tmp.path(), PORTS_RECONNECT);
	let me = Arc::new(Identity::generate("rejoiner"));
	let other = Arc::new(Identity::generate("somebody"));
	HubClient::join(&hub.url, &me).await.expect("join");
	HubClient::join(&hub.url, &other).await.expect("other joins");
	let (_o, _ro) = HubClient::connect(&hub.url, &hub.key, other.clone(), [], [], [])
.await.expect("other");

	let (first, _r1) = HubClient::connect(&hub.url, &hub.key, me.clone(), [], [], [])
.await.expect("first");
	assert!(until_rostered(&first, other.instance_id(), 30).await, "the first session is served");

	// Close and reconnect the way the app does after scanning a card.
	first.close().await;
	drop(first);
	let (second, _r2) = HubClient::connect(&hub.url, &hub.key, me.clone(), [], [], [])
.await.expect("second");
	assert!(
		until_rostered(&second, other.instance_id(), 30).await,
		"the reconnected session must be told who is there, or every send fails with 'no such member'"
	);
	second.close().await;
}

/// Writing to someone the hub has never described — the case that broke it.
///
/// You cannot hold two phones at once, so the other one is locked, its app closed, and a hub
/// forgets a member 45 s after they go quiet. Scan their code then and the hub can tell you
/// nothing about them. With only a fingerprint there would be nothing to seal to and every
/// send would fail with "no such member"; the card carries their sealing keys, so the message
/// is sealed, offered, and held for them.
#[tokio::test(flavor = "multi_thread")]
async fn someone_the_hub_never_mentioned_can_still_be_written_to() {
	let tmp = tempfile::tempdir().unwrap();
	let hub = Hub::spawn(tmp.path(), PORTS_FORGOTTEN);
	let a = Arc::new(Identity::generate("awake"));
	let b = Arc::new(Identity::generate("asleep"));
	HubClient::join(&hub.url, &a).await.expect("a joins");
	HubClient::join(&hub.url, &b).await.expect("b joins");

	// B never connects at all, so no roster ever names them. A has only their scanned code.
	let card_b = ContactCard::mine(&b, Some(&hub.url));
	assert!(card_b.seals(), "a code carries the keys needed to reach someone");
	let (ca, _ra) = HubClient::connect(&hub.url, &hub.key, a.clone(), [], [card_b.clone()], []).await.expect("a");
	assert!(ca.member(b.instance_id()).is_none(), "the hub has said nothing about them");

	assert_eq!(
		ca.send_chat(b.instance_id(), "written before we ever met", None).await,
		Verdict::Held,
		"sealed from the code and held for them, not refused"
	);

	// And when they do arrive, it is delivered — the outbox flush is the app's job, so here we
	// simply prove the same send now goes straight through.
	let (cb, mut rb) = HubClient::connect(&hub.url, &hub.key, b.clone(), [a.instance_id().to_string()], [], [])
		.await
		.expect("b arrives");
	assert!(until_rostered(&ca, b.instance_id(), 30).await, "now the hub describes them");
	assert_eq!(ca.send_chat(b.instance_id(), "and again once you woke", None).await, Verdict::Delivered);
	let ev = wait_event(&mut rb, 20, |e| matches!(e, Event::Chat { .. })).await;
	assert!(matches!(&ev, Event::Chat { text, .. } if text == "and again once you woke"), "{ev:?}");

	ca.close().await;
	cb.close().await;
}
