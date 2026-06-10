# Design: Crypto-Session Facade (route all Signal orchestration through one module)

**Date:** 2026-06-10
**Status:** Approved
**Type:** Refactor (behavior-preserving; security-sensitive)

## Motivation

The graphify analysis flagged that crypto reaches up into the message handlers. On
inspection the problem is broader: the Signal/X3DH session orchestration is inlined in
**four** production sites, and a parallel **dead** facade exists in `net/`:

1. `src/handlers/messaging.rs` — inbound decrypt **and** PreKey session establishment
   (loads 4 pieces of material, `from_prekey_message_real`, decrypt, KV cleanup) (~190 lines).
2. `src/handlers/friend_request.rs` — accept-bundle generation (raw
   `libsignal_protocol::vxeddsa`, persists material) **and** incoming-accept
   establishment + handshake encrypt (~180 lines).
3. `src/main.rs:909` — TUI outbound: `SessionStore` → load → `encrypt` → build `TextMessage`.
4. `src/daemon/rpc/messaging.rs:71` — daemon outbound: a near-duplicate of #3.

`net::MessageSender::prepare_message` / `MessageReceiver::decrypt_message` already
implement the encrypt/decrypt paths, but are constructed **only in their own tests** —
dead scaffolding superseded by the inline copies. Nothing outside `net/{sender,receiver}.rs`
imports them.

The X3DH establishment material is stored as stringly-typed `app_settings` rows
(`prekey_identity:/prekey_spk:/prekey_opk:/signal_identity_secret:/prekey_created_at:<onion>`)
written in `friend_request.rs`, read+deleted in `messaging.rs`, and deleted again in
`db/queries/settings.rs::cleanup_stale_prekey_material` — the key format duplicated
across three files.

## Goal & Non-Goals

**Goal:** Introduce one crypto-session facade (`SessionManager`) plus a typed
`PreKeyStore`, route all four production sites through it, and delete the dead `net/`
scaffolding — with **no change to wire protocol, crypto behavior, or the on-the-wire
bytes**.

**Non-Goals:**
- No change to the Signal primitives (`SignalSession`, `PreKeyBundle`,
  `PreKeyPrivateMaterial`) in `signal.rs`, or to `SessionStore`.
- No change to the JSON wire format, message types, or X3DH protocol.
- No change to the Ed25519 friend-request signing/TOFU logic (stays in handlers).
- No new features; no `Database` trait seam (separate future work).

## Constraints (verified against source)

- The two outbound sites (`main.rs`, `daemon/rpc/messaging.rs`) are identical at the
  crypto core and both set `x3dh_init: None` on the built `TextMessage` even when
  `is_prekey` could be true. This is safe in practice (the session is always
  established by send-time via the friend-request handshake) and **must be preserved
  exactly** — `encrypt_for` produces no `x3dh_init`.
- The two outbound sites differ only in error handling: the daemon returns an
  `RpcResponse` error on "no session"; the TUI silently drops (stores locally, doesn't
  send). Both behaviors must be preserved.
- The inbound handler has three outcomes: decrypt via existing session; establish from
  a PreKey message (consuming material); or — no session and not a PreKey message — log
  and drop. All three must be preserved.
- The e2e tests (`tests/e2e_messaging.rs`, 6 tests) reimplement the X3DH flow **inline**
  via `SignalSession::from_prekey_*` / `SessionStore` — they do **not** call the
  handlers, so they will NOT catch a facade-wiring bug. New facade tests are required
  (see Verification).
- Baseline: `cargo test` green (361 lib, 32 daemon, 6 e2e, 16 integration).

## Design

### Module layout
```
src/crypto/
  signal.rs            // unchanged (SignalSession, PreKeyBundle, PreKeyPrivateMaterial)
  session_store.rs     // unchanged (SessionStore — low-level session persistence)
  identity.rs          // unchanged
  prekey_store.rs      // NEW — typed accessor for X3DH establishment material
  session_manager.rs   // NEW — SessionManager orchestration facade
  mod.rs               // add `pub mod prekey_store; pub mod session_manager;` + re-exports
```
DELETED: `src/net/sender.rs`, `src/net/receiver.rs`, and their two `pub mod` lines in
`src/net/mod.rs`. Their useful crypto-roundtrip test coverage is re-homed into
`SessionManager` tests (see Verification).

### `PreKeyStore<'a>` (prekey_store.rs)

Owns the establishment-material key formats in one place:
```rust
pub struct PreKeyStore<'a> { db: &'a Database }

impl<'a> PreKeyStore<'a> {
    pub fn new(db: &'a Database) -> Self;

    /// Persist private material + signal identity secret + creation timestamp for `peer`.
    pub fn store(&self, peer: &str, material: &PreKeyPrivateMaterial,
                 signal_identity_secret: &[u8; 32], created_at: i64) -> Result<()>;

    /// Load the stored PreKeyPrivateMaterial for `peer` (None if absent).
    pub fn load(&self, peer: &str) -> Result<Option<PreKeyPrivateMaterial>>;

    /// Load the stored signal identity secret for `peer` (None if absent).
    pub fn load_signal_identity_secret(&self, peer: &str) -> Result<Option<[u8; 32]>>;

    /// Delete all establishment material for `peer` (idempotent).
    pub fn delete(&self, peer: &str) -> Result<()>;

    /// Delete material older than `max_age_secs`; returns the count of peers cleaned.
    pub fn cleanup_stale(&self, max_age_secs: u64) -> Result<usize>;
}
```
The key strings (`prekey_identity:`, `prekey_spk:`, `prekey_opk:`,
`signal_identity_secret:`, `prekey_created_at:`) become private constants of this
module — the single source of truth. `db/queries/settings.rs::cleanup_stale_prekey_material`
is reduced to a thin delegate: `PreKeyStore::new(db).cleanup_stale(max_age_secs)` (its
callers and signature are unchanged, so no churn at the daemon-task call site).

### `SessionManager<'a>` (session_manager.rs)

```rust
pub struct SessionManager<'a> { db: &'a Database }

/// Crypto fields for an outgoing TextMessage. `x3dh_init` is Some only for the
/// initiator handshake (establish_from_accept); None for normal sends.
pub struct OutgoingCrypto {
    pub header: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub is_prekey: bool,
    pub x3dh_init: Option<X3DHInitData>,
}

impl<'a> SessionManager<'a> {
    pub fn new(db: &'a Database) -> Self;

    /// Encrypt `plaintext` for an established session with `peer`.
    /// Ok(Some) = encrypted; Ok(None) = no session (caller drops/errors as today).
    /// x3dh_init is always None (preserves current outbound behavior).
    pub fn encrypt_for(&self, peer: &str, plaintext: &[u8]) -> Result<Option<OutgoingCrypto>>;

    /// Decrypt an incoming TextMessage. Establishes a session from stored PreKey
    /// material if there's no session and the message is a PreKey message (consuming
    /// the material on success). Ok(Some)=payload; Ok(None)=no session & not a PreKey
    /// message (caller drops). Err=crypto failure.
    pub fn decrypt_incoming(&self, msg: &TextMessage) -> Result<Option<PlaintextPayload>>;

    /// Acceptor side: generate a dedicated Signal identity + PreKey bundle, persist the
    /// private material (via PreKeyStore), and return the bundle for the accept message.
    pub fn create_accept_bundle(&self, peer: &str) -> Result<PreKeyBundle>;

    /// Initiator side: verify the bundle's VXEdDSA self-signature, establish a session
    /// (loading or generating our signal identity secret), store it, and encrypt the
    /// handshake PreKey message. Returns its crypto fields (with x3dh_init).
    pub fn establish_from_accept(&self, peer: &str, bundle: &PreKeyBundle) -> Result<OutgoingCrypto>;
}
```

**Responsibility boundary.** `SessionManager` owns *only* Signal crypto: sessions,
encrypt/decrypt, X3DH establishment, the raw `libsignal_protocol::vxeddsa` calls, and
PreKey material (via `PreKeyStore`). It builds the internal "handshake"
`PlaintextPayload` and computes `X3DHInitData`. Callers retain all **protocol/wire**
concerns: assembling `Message::TextMessage` envelopes, base64-encoding header/ciphertext,
Ed25519 friend-request signing/verification (TOFU), message queueing, and DB
friend/conversation/message writes. The facade depends on the protocol types it needs
(`TextMessage`, `PlaintextPayload`, `X3DHInitData`) but not on `App`, handlers, or net.

### The four site migrations (behavior-preserving)

- **`handlers/messaging.rs`** inbound `TextMessage` branch → replace the ~190 inline
  lines with `SessionManager::new(&app.db).decrypt_incoming(text_msg)?`. Keep: the
  `Some(payload)` path, the `None` → log+`return Ok(())` drop, the
  `payload.message_type == "handshake"` early return, and the friend/conversation/store
  writes.
- **`handlers/friend_request.rs::handle_accept_friend_request`** → replace the raw
  `vxeddsa` keypair gen, `PreKeyBundle::generate_real`, and the 5 KV writes with
  `let bundle = SessionManager::new(&app.db).create_accept_bundle(&from_onion)?;`. Keep:
  the accept-message construction, Ed25519 `identity.sign(...)`, the
  `friends`/`friend_requests`/subscription DB writes, and the queue.
- **`handlers/friend_request.rs::handle_incoming_accept`** → keep the Ed25519 TOFU
  signature verification (it's friend-request, not Signal), then call
  `let hs = SessionManager::new(&app.db).establish_from_accept(&accept.from_onion, &bundle)?;`
  and wrap `hs` into the handshake `TextMessage` + queue (as today). The bundle parse
  stays in the handler (or moves in — see Ambiguity resolution below).
- **`main.rs:909` & `daemon/rpc/messaging.rs:71`** → replace each inline
  load/encrypt/store block with `SessionManager::new(&db).encrypt_for(&peer, &plaintext)?`.
  The daemon maps `Ok(None)` to its existing "No encryption session with {peer}" error
  and `Err` to its "Encrypt error"; the TUI maps `Ok(None)`/`Err` to its existing drop
  (the `_ => None` arm). Each site keeps building its own `TextMessage` with
  `x3dh_init: None`.

**Ambiguity resolution:** `handle_incoming_accept` currently parses the
`PreKeyBundle` from JSON *before* verifying the Ed25519 accept signature. To preserve
ordering exactly, the handler keeps parsing the bundle and doing the Ed25519 verify;
`establish_from_accept` receives the already-parsed `&PreKeyBundle` and does the
VXEdDSA self-consistency check internally (as the inline code does today).

### Retiring the dead facade

Delete `net/sender.rs` and `net/receiver.rs` and remove their `pub mod` lines from
`net/mod.rs`. Confirmed no production importers. Their crypto-roundtrip assertions are
re-homed into `SessionManager` tests so net coverage is preserved or improved.

## Error Handling & Data Flow

Unchanged on the wire. The facade returns `Result`/`Result<Option<_>>`; every caller
maps those to its **existing** behavior (error response, drop, or proceed). The
`OutgoingCrypto`/`PlaintextPayload`/`X3DHInitData` shapes match what the inline code
produces today. No SQL schema change — `PreKeyStore` reads/writes the same
`app_settings` rows with the same key strings, just centralized.

## Testing & Verification

This is **not** a byte-identical move, and the existing e2e tests don't exercise the
handler path, so correctness rests on new facade tests. Done only when **all** pass:

1. **Keystone integration test** (in `session_manager.rs` `#[cfg(test)]`): run the full
   handshake through *only* the facade API against two temp DBs —
   acceptor `create_accept_bundle` → initiator `establish_from_accept(bundle)` →
   acceptor `decrypt_incoming(handshake TextMessage)` (establishes + consumes material)
   → bidirectional `encrypt_for`/`decrypt_incoming` roundtrip on a real payload →
   assert the acceptor's PreKey material is deleted afterward. This covers the
   cross-flow KV contract that is implicit today.
2. **`PreKeyStore` unit tests**: `store`→`load`/`load_signal_identity_secret` round-trip
   (incl. the no-OPK case), `delete` idempotency, and `cleanup_stale` honoring the age
   threshold.
3. **Existing suite stays green**, unchanged: `cargo test` — 361 lib (minus the deleted
   net/sender+receiver tests, plus the new facade tests; net total tracked in the plan),
   32 daemon, 6 e2e, 16 integration. Run with `--test-threads=1` (pre-existing
   `app.rs` `$HOME`-race flake).
4. `cargo clippy --lib -- -D warnings` clean; `cargo fmt --check` clean.

## Risk Assessment

**Medium-high** — the highest-risk refactor in the backlog: security-sensitive crypto
orchestration, restructured (not moved), and not covered end-to-end by the existing
handler tests. Mitigations: (a) the facade centralizes the previously-duplicated logic,
so the cross-flow KV contract becomes testable in one place; (b) the keystone
integration test exercises the entire handshake through the public facade; (c) the
primitives in `signal.rs`/`session_store.rs` are untouched, so the Signal math itself
does not change — only where it is called from. Rollback is per-commit revert.
