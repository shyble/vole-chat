# vole-chat

A chat app for [VoleNet](https://openvole.com/openvole/volenet): end-to-end encrypted with post-quantum sealing, relayed through hubs that cannot read a word, and built so a message to someone who is away waits on *your* device — never on a server. Your own OpenVole agents are on the same mesh, so you can talk to them from the same app.

Flutter UI on both stores, Rust protocol core underneath. Both are here.

## Layout

| path | what |
|---|---|
| `PROTOCOL.md` | The wire protocol as a client sees it, extracted from OpenVole core. Section numbers are referenced from the Rust code. |
| `core/` | Rust crate `volenet`: identity, signed messages, hybrid X25519 + ML-KEM-768 sealed envelopes, and (next) the hub client. No UI, no storage. |
| `core/tests/interop.rs` | The tests that matter: every primitive proven against OpenVole's own TypeScript, in both directions. |
| `harness/` | A tiny Node program that runs core's real implementation one primitive at a time, driven by the Rust tests. Needs OpenVole checked out beside this repo. |
| `app/` | The Flutter app (iOS, Android, macOS). |

## Design decisions, and why

- **Hub keeps nothing readable.** Envelopes are sealed to static keys with no ratchet, so ciphertext at rest on a third party would be retroactively readable on a later key compromise. An undelivered message stays in the sender's outbox; the hub records only that somebody tried. See `PROTOCOL.md` §7.
- **No background work, no push required.** The app is live while open, drains what waited when it opens, and is told on reconnect who tried to reach it. Push can be added later as an option, not a dependency.
- **Rust for the protocol, not Dart.** ML-KEM-768 and ML-DSA-65 have audited Rust implementations and no mature pure-Dart ones. Bridged with `flutter_rust_bridge`.
- **Seeds, not PKCS8.** Every keypair regenerates from 160 bytes of seed, which is what a backup QR holds.

## Building

```bash
# Rust core + interop tests (OpenVole must be at ../openvole, Node 24+ with OpenSSL 3.5)
cd harness && pnpm install && cd ..
cd core && cargo test

# Flutter app
cd app && flutter run
```

## Status

- [x] Identity, canonical signing, sealed envelopes — interoperating with OpenVole core
- [x] Hub client: join, announce, roster, relay, consent, outbox, notices — tested against a real hub and member (`core/tests/hub.rs`)
- [ ] Flutter app over the bridge: onboarding, hubs, contacts, chat, key backup
- [ ] Store builds
