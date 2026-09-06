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

## 6b. Endpoint directory (proposed)

> Status: proposed, not implemented. Written down first because it changes what a hub knows about
> its members, and that is worth arguing with before it is worth writing.

A peer's address moves — DHCP renews, a laptop changes network, a phone leaves Wi-Fi. Identity does
not. Today a client learns a peer's endpoint once, at pairing, and is stuck with it: the pair is
permanently unreachable directly the moment either side moves, with no signal that anything changed.
The hub already knows the current address of everyone connected to it, and already pushes a roster
whenever membership changes. This publishes that one field, under consent.

**What the hub stores:** for each member, its current `endpoint` as observed at announce time — the
same string already carried in `discover`. Nothing else changes. The hub still stores no message,
no ciphertext, and no history; an endpoint is live state, dropped with the member after 45 s of
silence like the rest of its record.

**Who may see it.** Not the roster at large. An endpoint is a rough location, and publishing the map
would make the hub an enumeration surface for where its members physically are — worse than the
relay it replaces, since today only the hub knows that and contacts do not.

- Off unless the member opts in (`publishEndpoint: true` in its `discover` payload).
- Handed only to peers that member has **mutual consent** with (§7). A stranger sees the roster
  entry with no `endpoint`, exactly as now.
- Never in `/volenet/info`, which is unauthenticated.

**Roster entries then carry an optional field:**

```json
{ "instanceId", "name", "publicKey", "xPublicKey", "mlkemPublicKey", "connected": true, "endpoint": "https://host:9700" }
```

Absent means: no opt-in, no consent, or the hub has no current address (a phone announces
`endpoint: ""`). A changed address needs no new message — it arrives in the next roster, which the
hub already pushes on membership change.

**An endpoint is a hint, never authority.** Trust is the pinned key and nothing else:

- A client MUST NOT change what it trusts because an endpoint changed. Signatures still verify
  against the key from the card or the roster, and envelopes still seal to that key.
- A hostile hub can therefore point a client at a machine it controls, and gain nothing readable:
  that machine holds no key, so it cannot decrypt what arrives, cannot sign anything the client will
  accept, and cannot impersonate. It can deny service, which a hub can do anyway by not relaying.
- A client MUST verify the peer's identity on connect (the `discover:response` signature, §5)
  before treating a dialled endpoint as that peer.

**Client behaviour.** Direct is an optimisation; relay is the floor.

1. With an endpoint for a consented peer, try direct (§4–§5 to a node).
2. On failure — refused, timed out, unreachable — fall back to relay (§7) and keep going. Most
   pairs of phones will fail here permanently: carrier-grade NAT and home routers drop unsolicited
   inbound, and this protocol is TCP, where hole punching mostly does not work. An endpoint being
   published says the hub can see an address, not that anyone else can reach it.
3. Remember which route worked per peer, and re-try direct when its endpoint changes.
4. Never require direct for correctness. Anything that only works over a direct link (§7b) stays
   unavailable to a peer that cannot be dialled, and must say so rather than hang.

**What this does not solve.** Reachability. It fixes stale addresses, which is why a pair of agents
silently stops being able to reach each other; it does not make an unreachable peer reachable.

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

## 7c. Rooms (proposed)

> Status: proposed, not implemented. Written first because it is the one feature that changes what
> a hub holds about its members, and because the cheap version of it quietly gives up the property
> the rest of this document is built on.

Everything so far is one-to-one: an envelope is sealed to one recipient's static keys, and that is
why a hub can carry it while reading nothing. A room is many-to-one-to-many, and the obvious
implementation — one envelope under a shared room key — would end that. This is the version that
does not.

### The rule

**A post is sealed once per member.** The sender fans out: for each member of the room it seals the
same inner message to that member's keys (§3) and hands the hub one envelope per recipient, exactly
as it would for that many private messages. The hub routes them. It learns who is in the room, who
posted, when, and how big — the routing metadata it already sees — and nothing else.

There is no room key. That is the whole point:

- **Removing a member takes effect immediately and by construction.** You stop sealing to them, so
  they cannot read what comes next. With a shared key you would have to rotate it, and a member
  removed without rotation keeps reading — the problem that group-messaging protocols exist to
  solve, and not one worth importing for rooms this size.
- **A shared key would not even avoid the work.** The room key still has to be sealed to each
  member, once per rotation. You keep the per-member sealing and add the rotation to it.
- **Nothing new is trusted.** No new primitive, no new key material, no new place a secret can sit.

The cost is bandwidth: N envelopes per post, each carrying the ML-KEM ciphertext (~1.2 KB) on top
of the message. A short line to ten members is on the order of 15 KB. That is fine for a room and
absurd for a broadcast channel, so:

**A room has a member ceiling of 64.** Enforced by the hub at join, and stated here so it fails
loudly at the size where the design stops making sense rather than quietly getting slow. A room
that needs more members needs a different design, and should say so rather than degrade.

### Messages

Rooms are hub state, so every message goes to the hub signed and unsealed — the hub has to read it,
and none of it is private. Only the *posts* are sealed, and those are ordinary §7 relay envelopes.

| type | direction | payload |
|---|---|---|
| `room:create` | member → hub | `{ name, topic? }` → `room:info` or `room:error` |
| `room:join` | member → hub | `{ room }` — subject to the room's policy and the ceiling |
| `room:leave` | member → hub | `{ room }` |
| `room:invite` | member → hub | `{ room, member }` — for a closed room |
| `room:list` | member → hub | `{}` → `room:list:response` `{ rooms: [{ room, name, members, joined }] }` |
| `room:info` | hub → member | `{ room, name, topic?, members: [ { instanceId, name, publicKey, xPublicKey, mlkemPublicKey } ] }` |
| `room:members` | hub → member | pushed on every membership change, same shape as `room:info` |
| `room:error` | hub → member | `{ room?, reason }` — `full`, `not-a-member`, `no-such-room`, `refused` |

`room:info` and `room:members` carry the members' **keys**, not just their ids. A sender cannot fan
out without them, and this is the same directory role the roster already plays (§6): the hub
supplies keys it cannot alter undetected, because every inner message is signed by its author and
verified against the key the roster gave.

### Posting

A post is not a new envelope type. For each member other than yourself, seal a `chat:message`
inner (§7) with one extra field and send it as an ordinary `sealed` envelope:

```json
{ "text": "...", "fromName": "...", "sentAt": 0, "room": "<room id>" }
```

A recipient that sees `room` on an inner `chat:message` files it under that room instead of under a
private conversation with the sender. A recipient that does not understand rooms shows it as a
direct message from a person it has consented to — degraded, but not broken or silently dropped.

Because posts are ordinary relayed chat, everything §7 already says applies unchanged: the hub
answers `relay:ack` or `relay:error` per envelope with its own `ref`, a member who is away has
their copy **held in the sender's outbox** and delivered when they return, and consent still gates
whether a member accepts anything from that sender at all.

That last point is a real consequence and not a bug: **a room does not create consent.** Joining a
room with someone you have not accepted does not let them message you. Their posts are dropped like
any other unconsented chat, and the room shows a gap. Clients should say so plainly rather than let
it look like a delivery failure.

### What a hub stores

Membership, a name, an optional topic. No posts, no history, no ciphertext — a room changes nothing
about §9. A room with no members for its TTL is forgotten.

### What this does not do

- **No history.** A member who joins sees what is posted from then on. There is nowhere for the
  past to have been kept, by design.
- **No ordering guarantee.** N envelopes are N deliveries; recipients order by the `sentAt` each
  post carries, and two posts in the same millisecond may be seen differently by different members.
- **No read state.** The hub does not know who has read what, and cannot.

## 7b. Asking a node's brain

Over a **direct link only** (§4–§5 to a node, or the same inverted). A hub will not carry this: a relay accepts `chat:message`, `relay:connect-*` and `file:*`, and drops everything else, so executable traffic never travels through a third party.

The two are different conversations at both ends — chat reaches the person, delegation reaches the agent's own per-peer session — so a client keeps a transcript for each rather than one interleaved.

Client → node: signed `task:delegate` `{ taskId, input, fromName }`, sealed as `sealed:direct` when the node announced sealing keys. `fromName` is what makes it *chat*: the node frames the input as a message from a named peer and runs it through its own brain in a session kept per peer, rather than as an unattributed task.

Node → client: `task:result` `{ taskId, status, result?, error? }`. `status: "completed"` carries `result`; anything else carries `error`. There is no progress and no partial output; an answer takes as long as a model takes.

The node decides whether to answer at all. It checks the asking identity against its own peer trust (`allowBrain`), falling back to what it allows anyone who joins publicly; a refusal is a `task:result` naming `allowBrain`, so a client can tell the person what to ask its operator for.

A client that serves the node endpoints (§ phone-to-phone) answers `task:delegate` with a failed `task:result` — it has no brain, and silence would leave the asker waiting.

## 8. What a client must persist

Own four keypairs (export/backup as a QR of the seeds); pinned hub key strings per hub URL; roster snapshot per hub (for names and keys while offline); rooms joined and their member lists (§7c); per-peer transcript; which chats are switched to the node's brain, and their transcripts kept apart from the chats with its operator; the last known endpoint per peer and which route last worked (§6b); **outbox** (`{ref, to, toName, text, sentAt, attempts}`, a week TTL); pending notices; relay consents given and received.

## 9. Things the hub will never tell you

- Whether a forwarded message was *read*, or even decrypted — `relay:ack` means it left the hub.
- Anything about a member not in the roster.
- The content of anything: it holds ciphertext in flight and routing metadata (from, to, size, time) at rest — and for a member who is away, the notice only.
