//! The two-phone dance against the real Club: one is put down as the other is picked up.
//!
//! Not a test — it needs the network and a live hub. It exists because the failure it chases
//! only shows up when the two sides are never awake together, which no local harness reproduces
//! and no amount of reading the code settles.
//!
//!   cargo run --example twophone
use rust_lib_vole_chat::outbox::{Outbox, OutboxKind, DEFAULT_TTL_MS};
use rust_lib_vole_chat::{ContactCard, Event, HubClient, Identity, Verdict};
use std::sync::Arc;
use std::time::Duration;

const HUB: &str = "https://club.openvole.com/mesh";
fn now() -> i64 { rust_lib_vole_chat::message::now_ms() }

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let a = Arc::new(Identity::generate("phone-17"));
    let b = Arc::new(Identity::generate("phone-15"));
    let ja = HubClient::join(HUB, &a).await.expect("a joins");
    let jb = HubClient::join(HUB, &b).await.expect("b joins");
    let card_b = ContactCard::mine(&b, Some(HUB));
    println!("a(17)={}  b(15)={}", a.instance_id(), b.instance_id());

    // ── 17 is in hand, 15 is in a pocket. Scan and ask. ──
    let (ca, _ra) = HubClient::connect(HUB, &ja.hub_key_string, a.clone(), [], [card_b], [])
        .await.expect("a connects");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let v = ca.request_connect(b.instance_id(), Some("scanned your code")).await;
    println!("17 asks while 15 is away -> {v:?}");
    let mut outbox = Outbox::new();
    if v == Verdict::Held {
        outbox.add_kind(OutboxKind::ConnectRequest, b.instance_id(), "phone-15", "", now(),
                        Some("scanned your code".into()));
        println!("   held in the outbox ({} waiting)", outbox.len());
    }

    // ── Put the 17 down and pick up the 15. The 17 lingers 20s. ──
    println!("\n[17 backgrounded — lingering 20s]");
    tokio::time::sleep(Duration::from_secs(4)).await;
    println!("[15 picked up, connecting]");
    let (cb, mut rb) = HubClient::connect(HUB, &jb.hub_key_string, b.clone(), [], [], [])
        .await.expect("b connects");

    // The 17 is still up during its linger: its retry finds the 15 and delivers.
    for i in 1..=8 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let done = outbox.flush(&ca, now(), DEFAULT_TTL_MS).await;
        let sees = ca.member(b.instance_id()).map(|m| m.connected).unwrap_or(false);
        println!("  t+{:02}s  17 sees 15 online={sees}  outbox={}  {:?}", i * 2, outbox.len(), done);
        if outbox.is_empty() { break; }
    }

    match tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match rb.recv().await {
                Some(Event::ConnectRequest { from_name, note, .. }) => return Some((from_name, note)),
                Some(_) => continue,
                None => return None,
            }
        }
    }).await {
        Ok(Some((n, note))) => println!("\n*** 15 received the request from {n} (note {note:?})"),
        _ => println!("\n!! 15 never received the request"),
    }
    ca.close().await;
    cb.close().await;
}
