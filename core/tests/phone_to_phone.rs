//! Two phones on one network, with nothing in between — no hub, no agent, no internet.
//!
//! One listens, the other dials it exactly as it dials an agent. The trust contract is the
//! same as everywhere else: connecting proves nothing, so an unaccepted caller is refused and
//! only a pair request the person accepts opens the link.

use rust_lib_vole_chat::client::ClientError;
use rust_lib_vole_chat::{node_info, node_pair, DirectClient, Event, Identity, Listener, Verdict};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

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
async fn one_phone_dials_another() {
	let alice = Arc::new(Identity::generate("alice-phone"));
	let bob = Arc::new(Identity::generate("bob-phone"));

	// Alice makes herself reachable; the port is whatever the OS gives her.
	let (listener, mut alice_events) = Listener::start(alice.clone(), 0, []).await.expect("listen");
	let url = format!("http://127.0.0.1:{}", listener.port());
	assert!(listener.is_listening());

	// Bob fingerprints her before trusting anything. This needs no trust in either direction.
	let info = node_info(&url).await.expect("info");
	assert_eq!(info.instance_id, alice.instance_id());
	assert_eq!(info.name.as_deref(), Some("alice-phone"));
	assert!(info.post_quantum);

	// Connecting is refused until Alice says yes — reaching her proves nothing.
	match DirectClient::connect(&url, &info.key_string, bob.clone()).await {
		Err(ClientError::NotTrusted) => {}
		other => panic!("expected NotTrusted before Alice accepts, got {:?}", other.map(|_| ())),
	}

	// Bob asks. Alice sees it, with his note, as a request she can act on.
	let p = node_pair(&url, &bob, Some("bob, from the sofa")).await.expect("pair");
	assert!(p.queued && !p.already_trusted);
	let ev = wait_event(&mut alice_events, 10, |e| matches!(e, Event::ConnectRequest { .. })).await;
	assert!(
		matches!(&ev, Event::ConnectRequest { from, from_name, note }
			if from == bob.instance_id() && from_name == "bob-phone" && note.as_deref() == Some("bob, from the sofa")),
		"{ev:?}"
	);
	assert_eq!(listener.requests().len(), 1);

	// She accepts, and gets back his key string to keep.
	let key = listener.accept(bob.instance_id()).expect("accepted");
	assert_eq!(key, bob.key_string());
	assert!(listener.requests().is_empty());
	assert!(node_pair(&url, &bob, None).await.unwrap().already_trusted);

	// Now the link opens.
	let (client, mut bob_events) = DirectClient::connect(&url, &info.key_string, bob.clone()).await.expect("connect");
	assert!(matches!(wait_event(&mut bob_events, 10, |e| matches!(e, Event::Connected { .. })).await,
		Event::Connected { hub_name } if hub_name == "alice-phone"));
	assert!(client.seals(), "both phones announce sealing keys, so chat is sealed end to end");

	// Bob → Alice.
	assert_eq!(client.send_chat("hey, it's bob", None), Verdict::Delivered);
	let ev = wait_event(&mut alice_events, 10, |e| matches!(e, Event::Chat { .. })).await;
	assert!(
		matches!(&ev, Event::Chat { from, from_name, text, .. }
			if from == bob.instance_id() && from_name == "bob-phone" && text == "hey, it's bob"),
		"{ev:?}"
	);

	// Alice → Bob, down the socket he opened, sealed the same way.
	let written = rust_lib_vole_chat::message::now_ms() - 5_000;
	assert!(listener.send_chat(bob.instance_id(), "hey bob", Some(written)));
	let ev = wait_event(&mut bob_events, 10, |e| matches!(e, Event::Chat { .. })).await;
	assert!(
		matches!(&ev, Event::Chat { from, from_name, text, sent_at, .. }
			if from == alice.instance_id() && from_name == "alice-phone" && text == "hey bob" && *sent_at == written),
		"{ev:?}"
	);

	// Alice can see he is connected, and stops being reachable when she turns it off.
	assert_eq!(listener.callers(), vec![(bob.instance_id().to_string(), "bob-phone".to_string(), true)]);
	client.close().await;
	listener.stop();
	assert!(!listener.is_listening());
	assert!(node_info(&url).await.is_err(), "nothing answers once she stops listening");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_caller_she_never_accepted_gets_nowhere() {
	let alice = Arc::new(Identity::generate("alice"));
	let stranger = Arc::new(Identity::generate("stranger"));
	let (listener, _events) = Listener::start(alice.clone(), 0, []).await.expect("listen");
	let url = format!("http://127.0.0.1:{}", listener.port());
	let info = node_info(&url).await.expect("info is public");

	// Asking and being ignored is not acceptance.
	node_pair(&url, &stranger, None).await.expect("may ask");
	assert!(matches!(
		DirectClient::connect(&url, &info.key_string, stranger.clone()).await,
		Err(ClientError::NotTrusted)
	));

	// Denying removes the request and changes nothing about the answer.
	listener.deny(stranger.instance_id());
	assert!(listener.requests().is_empty());
	assert!(matches!(
		DirectClient::connect(&url, &info.key_string, stranger).await,
		Err(ClientError::NotTrusted)
	));
	assert!(listener.callers().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn callers_accepted_before_are_still_trusted_after_a_restart() {
	let alice = Arc::new(Identity::generate("alice"));
	let bob = Arc::new(Identity::generate("bob"));

	// The app persists the key string it was handed; starting again with it skips the handshake.
	let (listener, mut events) = Listener::start(alice.clone(), 0, [bob.key_string()]).await.expect("listen");
	let url = format!("http://127.0.0.1:{}", listener.port());
	let info = node_info(&url).await.expect("info");
	let (client, _bob_events) = DirectClient::connect(&url, &info.key_string, bob.clone()).await.expect("connect");
	assert_eq!(client.send_chat("still trusted", None), Verdict::Delivered);
	let ev = wait_event(&mut events, 10, |e| matches!(e, Event::Chat { .. })).await;
	assert!(matches!(&ev, Event::Chat { text, .. } if text == "still trusted"), "{ev:?}");
	client.close().await;
}

/// A phone is not an agent. Asking it to think is refused here rather than sent, and a peer
/// that asks anyway is told so — silence would leave it waiting for an answer nobody will write.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_has_no_brain_to_ask() {
	let alice = Arc::new(Identity::generate("alice"));
	let bob = Arc::new(Identity::generate("bob"));
	let (listener, _alice_events) = Listener::start(alice.clone(), 0, [bob.key_string()]).await.expect("listen");
	let url = format!("http://127.0.0.1:{}", listener.port());
	let info = node_info(&url).await.expect("info");
	let (client, mut bob_events) = DirectClient::connect(&url, &info.key_string, bob.clone()).await.expect("connect");
	wait_event(&mut bob_events, 10, |e| matches!(e, Event::Connected { .. })).await;

	// She announced herself as a client, so the question never leaves his phone.
	assert!(!client.has_brain());
	assert!(matches!(client.ask_brain("what is the weather"), Err(ClientError::Refused(_))));

	// An older build that asks anyway gets a refusal back, not silence.
	let delegate = rust_lib_vole_chat::message::Message::create(
		&bob,
		"task:delegate",
		alice.instance_id(),
		serde_json::json!({ "taskId": "t-1", "input": "think about this", "fromName": "bob" }),
	);
	reqwest::Client::new()
		.post(format!("{url}/volenet/message"))
		.header("content-type", "application/json")
		.body(delegate.to_json())
		.send()
		.await
		.expect("post");
	let ev = wait_event(&mut bob_events, 10, |e| matches!(e, Event::BrainReply { .. })).await;
	assert!(
		matches!(&ev, Event::BrainReply { task_id, text, failed }
			if task_id == "t-1" && *failed && text.contains("not an agent")),
		"{ev:?}"
	);
	client.close().await;
}
