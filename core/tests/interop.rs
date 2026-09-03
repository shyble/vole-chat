//! Interoperability with OpenVole core — the real TypeScript, driven through `harness/interop.ts`.
//!
//! Every primitive is proven in both directions: what core signs, Rust verifies; what Rust
//! signs, core verifies; what core seals to Rust's keys, Rust opens; what Rust seals to core's
//! keys, core opens. If any of this fails, the app would not talk to a hub, so these tests are
//! not optional — they fail loudly when OpenVole is not checked out beside this repo.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use rust_lib_vole_chat::keys::parse_key_string;
use rust_lib_vole_chat::message::{now_ms, Message};
use rust_lib_vole_chat::seal::{seal, unseal, SealedBox};
use rust_lib_vole_chat::Identity;

fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// Run one harness command; returns its JSON output.
fn harness(args: &[&str]) -> Value {
	let root = repo_root();
	let tsx = root.join("harness/node_modules/.bin/tsx");
	assert!(tsx.exists(), "run `pnpm install` in harness/ first");
	assert!(
		root.parent().unwrap().join("openvole/src/core/src/net/seal.ts").exists(),
		"OpenVole must be checked out beside this repo (../openvole)"
	);
	let out = Command::new(tsx)
		.arg(root.join("harness/interop.ts"))
		.args(args)
		.current_dir(root.join("harness"))
		.output()
		.expect("spawn harness");
	assert!(
		out.status.success(),
		"harness {:?} failed:\n{}",
		args,
		String::from_utf8_lossy(&out.stderr)
	);
	let text = String::from_utf8(out.stdout).unwrap();
	serde_json::from_str(text.trim().lines().last().unwrap_or("")).unwrap_or_else(|e| panic!("harness output not JSON ({e}): {text}"))
}

struct TsNode {
	net_dir: PathBuf,
	key_string: String,
	instance_id: String,
	x_public: String,
	mlkem_public: String,
}

fn ts_node(tmp: &tempfile::TempDir, name: &str) -> TsNode {
	let net_dir = tmp.path().join(name);
	let v = harness(&["keygen", net_dir.to_str().unwrap(), name]);
	TsNode {
		net_dir,
		key_string: v["keyString"].as_str().unwrap().to_string(),
		instance_id: v["instanceId"].as_str().unwrap().to_string(),
		x_public: v["xPublicKey"].as_str().unwrap().to_string(),
		mlkem_public: v["mlkemPublicKey"].as_str().unwrap_or("").to_string(),
	}
}

#[test]
fn rust_reads_a_core_identity_exactly_as_core_does() {
	let tmp = tempfile::tempdir().unwrap();
	let ts = ts_node(&tmp, "core-node");
	let peer = parse_key_string(&ts.key_string).expect("core's key string parses");
	assert_eq!(peer.instance_id, ts.instance_id, "instance id derivation must match");
	assert_eq!(peer.name, "core-node");
	assert!(peer.mldsa.is_some(), "core on Node 24+ emits a hybrid key string");
	assert!(!ts.mlkem_public.is_empty(), "core on Node 24+ announces an ML-KEM key");
	rust_lib_vole_chat::keys::parse_x25519_public_b64(&ts.x_public).expect("core's X25519 SPKI parses");
	rust_lib_vole_chat::keys::parse_mlkem_public_b64(&ts.mlkem_public).expect("core's ML-KEM SPKI parses");
}

#[test]
fn what_core_signs_rust_verifies() {
	let tmp = tempfile::tempdir().unwrap();
	let ts = ts_node(&tmp, "signer");
	let peer = parse_key_string(&ts.key_string).unwrap();
	let payload = json!({"text": "héllo \"quoted\" \n", "nested": {"z": [1, true, null], "a": "b"}, "fromName": "signer"});
	let m: Message = serde_json::from_value(harness(&[
		"sign",
		ts.net_dir.to_str().unwrap(),
		"chat:message",
		"someone",
		&payload.to_string(),
	]))
	.expect("core's message deserializes into ours");
	assert_eq!(m.from, ts.instance_id);
	assert!(m.sig_pq.is_some());
	m.verify(&peer, now_ms()).expect("both of core's signatures verify over our canonical string");

	// And the canonical string is what binds it: touch the payload, both fail.
	let mut tampered = m.clone();
	tampered.payload["text"] = json!("hello");
	assert!(tampered.verify(&peer, now_ms()).is_err());
}

#[test]
fn what_rust_signs_core_verifies() {
	let me = Identity::generate("rusty");
	let m = Message::create(&me, "ping", "*", json!({"timestamp": now_ms(), "note": "ünïcode ok"}));
	let v = harness(&["verify", &me.key_string(), &m.to_json()]);
	assert_eq!(v["valid"], json!(true), "core rejected our message: {v}");

	// Core requires the PQ signature once it knows our ML-DSA key — prove it is checking it.
	let mut no_pq = m.clone();
	no_pq.sig_pq = None;
	let v = harness(&["verify", &me.key_string(), &no_pq.to_json()]);
	assert_eq!(v["valid"], json!(false));
	assert!(v["error"].as_str().unwrap().contains("post-quantum"), "{v}");
}

#[test]
fn what_core_seals_to_us_we_open_hybrid_and_legacy() {
	let me = Identity::generate("phone");
	let aad = format!("hub-id|{}", me.instance_id());

	let hybrid: SealedBox = serde_json::from_value(harness(&[
		"seal",
		&me.x25519_public_b64(),
		&me.mlkem_public_b64(),
		&aad,
		"sealed by openssl's ml-kem",
	]))
	.unwrap();
	assert!(hybrid.kem.is_some(), "core produced a hybrid envelope for our ML-KEM key");
	assert_eq!(unseal(&me, &hybrid, &aad).unwrap(), b"sealed by openssl's ml-kem");
	assert!(unseal(&me, &hybrid, "other|aad").is_err());

	let legacy: SealedBox =
		serde_json::from_value(harness(&["seal", &me.x25519_public_b64(), "-", &aad, "x25519 only"])).unwrap();
	assert!(legacy.kem.is_none());
	assert_eq!(unseal(&me, &legacy, &aad).unwrap(), b"x25519 only");
}

#[test]
fn what_we_seal_to_core_core_opens() {
	let tmp = tempfile::tempdir().unwrap();
	let ts = ts_node(&tmp, "recipient");
	let aad = format!("phone-id|{}", ts.instance_id);

	let hybrid = seal(&ts.x_public, Some(&ts.mlkem_public), "from rust, hybrid".as_bytes(), &aad).unwrap();
	let v = harness(&["unseal", ts.net_dir.to_str().unwrap(), &aad, &serde_json::to_string(&hybrid).unwrap()]);
	assert_eq!(v["plaintext"], json!("from rust, hybrid"), "{v}");

	let legacy = seal(&ts.x_public, None, "from rust, x25519".as_bytes(), &aad).unwrap();
	let v = harness(&["unseal", ts.net_dir.to_str().unwrap(), &aad, &serde_json::to_string(&legacy).unwrap()]);
	assert_eq!(v["plaintext"], json!("from rust, x25519"), "{v}");

	// Wrong aad on core's side fails closed too.
	let v = harness(&["unseal", ts.net_dir.to_str().unwrap(), "x|y", &serde_json::to_string(&hybrid).unwrap()]);
	assert_eq!(v["plaintext"], Value::Null);
}
