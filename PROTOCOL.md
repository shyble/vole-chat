# VoleNet wire protocol — as a chat client sees it

Extracted from OpenVole core 4.19.0 (`src/core/src/net/`). Everything here is what a hub actually checks. Where the client has a choice, the choice is stated. Section numbers are referenced from the Rust code.

## 1. Identity

A node has four keypairs. Signing keys sign; agreement keys agree; nothing is reused.

| purpose | algorithm | public key on the wire |
|---|---|---|
| identity / signing | Ed25519 | base64 of SPKI DER (44 chars) |
| identity / signing, post-quantum | ML-DSA-65 (FIPS 204) | base64 of SPKI DER |
| sealing, agreement | X25519 | base64 of SPKI DER |
| sealing, post-quantum | ML-KEM-768 (FIPS 203) | base64 of SPKI DER |

SPKI DER is `SEQUENCE { SEQUENCE { OID }, BIT STRING(0 unused bits ‖ raw key) }` with OIDs
Ed25519 `1.3.101.112`, X25519 `1.3.101.110`, ML-KEM-768 `2.16.840.1.101.3.4.4.2`, ML-DSA-65 `2.16.840.1.101.3.4.3.18`. Raw key sizes: 32, 32, 1184, 1952 bytes.

**Key string** (how an identity is exchanged and pinned):

```
vole-ed25519 <base64 SPKI Ed25519> <name> [<base64 SPKI ML-DSA-65>]
```

**Instance id** = first 16 hex chars of `sha256(utf8(<base64 SPKI Ed25519>))` — the hash is over the base64 *text*, not the DER bytes.

The X25519 and ML-KEM public keys are not in the key string; they travel in discovery and rosters as `xPublicKey` / `mlkemPublicKey`.

## 2. Signed messages

```json
{ "version": 2, "id": "<uuid v4>", "type": "<type>", "from": "<instanceId>", "to": "<instanceId or *>",
  "timestamp": <ms since epoch>, "signature": "<base64>", "sigPq": "<base64, optional>", "payload": <any> }
```

**Signed string** (UTF-8):

```
2:<id>:<type>:<from>:<to>:<timestamp>:<canonical(payload)>
```

`canonical(payload)` is JSON with object keys sorted recursively, `undefined` members omitted, and the empty string when payload is `null`/absent. Strings, numbers and escapes follow `JSON.stringify`; a client should only put strings, integers, booleans, arrays and objects in payloads it creates.

`signature` = Ed25519 over the string. `sigPq` = ML-DSA-65 over the same string, pure mode, empty context. **A receiver that knows the sender's ML-DSA key requires `sigPq`** and rejects its absence; one that doesn't accepts Ed25519 alone.

Freshness: rejected if older than **60 s** or more than 5 s in the future. Replay: `(from, id)` remembered for the window. Both signatures are checked on every message, on every transport.

## 3. Sealed envelopes

```json
{ "epk": "<base64 SPKI X25519, ephemeral>", "kem": "<base64 ML-KEM-768 ciphertext, hybrid only>", "n": "<base64 12-byte nonce>", "c": "<base64 ciphertext ‖ 16-byte tag>" }
```

- `ssX = X25519(ephemeral secret, recipient X25519 public)`
- hybrid: `(ssPq, kem) = ML-KEM-768.Encapsulate(recipient ML-KEM public)`; `ikm = ssX ‖ ssPq`, scheme `volenet-seal-v2`. Otherwise `ikm = ssX`, scheme `volenet-seal-v1`.
- `key = HKDF-SHA256(ikm, salt = empty, info = "<scheme>|<aad>", 32)`
- ChaCha20-Poly1305 with `aad` as associated data.
- **`aad = "<sender instanceId>|<recipient instanceId>"`.** The recipient recomputes it from who the hub says the envelope is from, so a re-addressed envelope fails the tag.
- Scheme is chosen by the presence of `kem`; a stripped `kem` fails the tag rather than downgrading.

The plaintext is itself a signed message (§2), so the recipient verifies the inner signature against the sender's key from the roster.

## 4. Joining a hub

1. `GET <hub>/health` → `{status:"ok", protocol:"volenet", version:1}`
2. `POST <hub>/volenet/join` `{ publicKey: "<key string>", name }` → `{ ok, hubPublicKey: "<key string>", instanceName, port }`. 202 `{pending:true}` if the hub requires approval. 404 means the hub has no public join.
3. **Pin `hubPublicKey`.** It is the only key that may sign hub traffic from now on. Optionally fingerprint it first from `GET /volenet/info` (`publicKey` field).

Rate limit: 5 joins per minute per IP.

## 5. Connecting and staying connected

1. `POST <hub>/volenet/message` with a signed `discover` (to `*`):
   `{ name, publicKey: "<key string>", endpoint, capabilities: [], role: "peer", version: "3.0.0", xPublicKey, mlkemPublicKey }`.
   Response body `{ ok, response: <signed discover:response> }` carries the hub's record: name, key string, endpoint, `xPublicKey`, `mlkemPublicKey`.
   **A phone advertises `endpoint: ""`** (or omits it): the hub then never tries to dial it, and reaches it only over the socket the phone opens.
2. Open a WebSocket to the hub's endpoint (`ws://` or `wss://`, any path). Send a signed `ping` `{timestamp}` immediately; **the first verified message binds the socket** to the sender id. Until then the hub cannot route to you.
3. Answer every `ping` with a signed `pong` `{timestamp}`. The hub pings every 15 s and **forgets a peer silent for 45 s** (`instances` entry and socket both dropped). Send your own `ping` every 15 s as well.
4. The hub also sends `tool:list`; reply `tool:list:response` with payload `[]`.
5. The hub may wrap any post-handshake message to you in `sealed:direct { box }` when it runs with `encrypt: true` and you announced an ML-KEM key. Unseal with `aad = "<hub id>|<my id>"`; the plaintext is a signed message — process it as if it had arrived bare.
6. Re-announce (`discover`) after any reconnect. The hub's record of you is gone after 45 s of silence.

Transport-level liveness: the hub also pings at the WebSocket level (20 s) and terminates a socket that misses a pong. Browser and native WebSocket stacks answer protocol pings automatically.

## 6. Roster

Hub → member on every membership change:

```json
{ "members": [ { "instanceId", "name", "publicKey": "<key string>", "xPublicKey", "mlkemPublicKey", "connected": true } ] }
```

Contains every instance the hub knows, connected or not. The roster is a **directory, not authority**: it tells you where to seal and which key a sender's inner signature must verify against.

## 7. Relayed chat

Member → hub: signed `sealed` `{ to: "<recipient id>", box: <§3>, ref: "<opaque>" }` where the box seals a signed inner message with `to = recipient`.

Hub verdict, always, to a sender that attached `ref`:

- `relay:ack { to, ref }` — forwarded.
- `relay:error { to, reason, ref, held: true }` with `reason: "peer-unreachable"` — the member is away; the hub recorded a **notice** (sender, count, times) and kept nothing else. **Keep the message in your own outbox** and resend when the member shows as `connected` in a roster.
- `relay:error { to, reason: "too-large" | "rate-limited", ref }` — refused; do not queue.

A hub too old to answer sends nothing; treat silence after 4 s as the legacy "delivered".

Hub → member: `relay:deliver { from, box }`. Unseal with `aad = "<from>|<me>"`, verify the inner signature against `from`'s roster key, drop replays of `(from, inner.id)` within 2 minutes.

Inner message types a member accepts over relay:

| type | payload |
|---|---|
| `chat:message` | `{ text, fromName, sentAt? }` — `sentAt` is when it was written (an outboxed message re-signed later); record that, bounded to ≤ inner timestamp + 60 s |
| `relay:connect-request` | `{ fromName, note? }` |
| `relay:connect-accept` / `relay:connect-deny` | `{ fromName }` |

**Consent:** a member's relayed chat is dropped until you accept it. Sending a `relay:connect-request` counts as accepting *them*; receiving one is surfaced for the user to approve or deny, which sends accept/deny back the same way.

Hub → member on (re)connect: `relay:pending { senders: [ { from, fromName, count, first, last } ] }` — who tried to reach you while away. Drop an entry if you have already received chat from that sender at or after its `last`.

Limits: sealed envelope ≤ 64 KiB as serialized JSON; 30 forwards per minute per sender→recipient pair.

## 7b. Asking a node's brain

Over a **direct link only** (§4–§5 to a node, or the same inverted). A hub will not carry this: a relay accepts `chat:message`, `relay:connect-*` and `file:*`, and drops everything else, so executable traffic never travels through a third party.

Client → node: signed `task:delegate` `{ taskId, input, fromName }`, sealed as `sealed:direct` when the node announced sealing keys. `fromName` is what makes it *chat*: the node frames the input as a message from a named peer and runs it through its own brain in a session kept per peer, rather than as an unattributed task.

Node → client: `task:result` `{ taskId, status, result?, error? }`. `status: "completed"` carries `result`; anything else carries `error`. There is no progress and no partial output; an answer takes as long as a model takes.

The node decides whether to answer at all. It checks the asking identity against its own peer trust (`allowBrain`), falling back to what it allows anyone who joins publicly; a refusal is a `task:result` naming `allowBrain`, so a client can tell the person what to ask its operator for.

A client that serves the node endpoints (§ phone-to-phone) answers `task:delegate` with a failed `task:result` — it has no brain, and silence would leave the asker waiting.

## 8. What a client must persist

Own four keypairs (export/backup as a QR of the seeds); pinned hub key strings per hub URL; roster snapshot per hub (for names and keys while offline); per-peer transcript; which chats are switched to the node's brain; **outbox** (`{ref, to, toName, text, sentAt, attempts}`, a week TTL); pending notices; relay consents given and received.

## 9. Things the hub will never tell you

- Whether a forwarded message was *read*, or even decrypted — `relay:ack` means it left the hub.
- Anything about a member not in the roster.
- The content of anything: it holds ciphertext in flight and routing metadata (from, to, size, time) at rest — and for a member who is away, the notice only.
