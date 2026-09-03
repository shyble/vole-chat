# vole-chat

A chat app for [VoleNet](https://openvole.com/openvole/volenet): end-to-end encrypted with post-quantum sealing, relayed through hubs that cannot read a word, and built so a message to someone who is away waits on *your* device — never on a server. Your own OpenVole agents are on the same mesh, so you can talk to them from the same app.

Flutter UI on both stores, Rust protocol core underneath. Both are here.

## Layout

| path | what |
|---|---|
| `PROTOCOL.md` | The wire protocol as a client sees it, extracted from OpenVole core. Section numbers are referenced from the Rust code. |
| `core/` | Rust crate: identity, signed messages, hybrid X25519 + ML-KEM-768 sealed envelopes, the hub client, the direct-node client, the outbox policy, and the bridge API. No UI, no storage. |
| `core/tests/interop.rs` | The tests that matter: every primitive proven against OpenVole's own TypeScript, in both directions. |
| `harness/` | A tiny Node program that runs core's real implementation one primitive at a time, driven by the Rust tests. Needs OpenVole checked out beside this repo. |
| `app/` | The Flutter app (iOS, Android, macOS). |

You add people by **identity**, not address: scan their code once and they stay reachable wherever either of you moves. Underneath, three routes carry it — **nearby** (one phone listens, the other dials, no hub and no internet), **a node** (dial an agent at its address), and **a hub** (relayed, for when neither side has an address) — and the app uses whichever is up.

A code is small on purpose. Post-quantum keys are far too big for a QR (ML-DSA-65 alone is 1952 bytes, all four about 4.4 KB of base64, against a 2953-byte ceiling), so a code carries the Ed25519 key plus a **fingerprint over the whole key set**, about 160 characters. The bulk still arrives from the hub, and is checked against that fingerprint — so the hub supplies the keys but cannot change them, and cannot read or impersonate.

## Design decisions, and why

- **Hub keeps nothing readable.** Envelopes are sealed to static keys with no ratchet, so ciphertext at rest on a third party would be retroactively readable on a later key compromise. An undelivered message stays in the sender's outbox; the hub records only that somebody tried. See `PROTOCOL.md` §7.
- **No background work, no push required.** The app is live while open, drains what waited when it opens, and is told on reconnect who tried to reach it. Push can be added later as an option, not a dependency.
- **Rust for the protocol, not Dart.** ML-KEM-768 and ML-DSA-65 have audited Rust implementations and no mature pure-Dart ones. Bridged with `flutter_rust_bridge`.
- **Seeds, not PKCS8.** Every keypair regenerates from 160 bytes of seed, which is what a backup QR holds.

## Building and testing

```bash
# Rust core: unit tests, interop with OpenVole's TypeScript, and a client against a real hub
# (OpenVole must be at ../openvole, Node 24+ with OpenSSL 3.5)
cd harness && pnpm install && cd ..
cd core && cargo test

# The Dart side of the bridge against a real hub, using the same native library the app ships
cd core && cargo build && cd ../app && flutter test test/bridge_test.dart

# The app (macOS today; iOS and Android once their toolchains are present)
cd app && flutter run -d macos
```

The bridge's Dart bindings are generated: after changing `core/src/api/`, run `flutter_rust_bridge_codegen generate` in `app/`.

## Status

- [x] Identity, canonical signing, sealed envelopes — interoperating with OpenVole core
- [x] Hub client: join, announce, roster, relay, consent, outbox, notices — tested against a real hub and member (`core/tests/hub.rs`)
- [x] Flutter app over the bridge: onboarding, hubs, chats (consent, held messages, notices), key backup — macOS build and Dart-side hub and node tests pass (`app/test/bridge_test.dart`)
- [x] Direct connections to nodes (your own agents): fingerprint, pair request, operator accepts once, then sealed chat straight over the node's address — `core/tests/node.rs`
- [x] Phone to phone on one network, with nothing in between: one listens, the other dials it the same way — `core/tests/phone_to_phone.rs`
- [x] Live while open, closed cleanly in the background, reconnects on resume
- [ ] Store builds (iOS: needs `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`, then `flutter build ios`; Android: needs the Android SDK and NDK installed, then `flutter build apk`)
