# Security Hardening Design — Critical & High Fixes

**Date:** 2026-03-02
**Status:** Approved
**Scope:** Fix all critical and high-severity findings from security audit

## Context

A comprehensive security audit identified 8 critical/high findings that undermine chattor's privacy-first design goals. This design addresses all of them.

## Findings Addressed

| # | Severity | Finding |
|---|----------|---------|
| 1 | CRITICAL | SQLCipher encryption key never set — database is plaintext |
| 2 | CRITICAL | Incoming friend requests not signature-verified |
| 3 | CRITICAL | Friend request accepts not signature-verified; PreKeyBundle VXEdDSA unchecked |
| 4 | HIGH | No memory zeroization of private keys |
| 5 | HIGH | PreKey private material stored indefinitely |
| 6 | HIGH | Rate limiter implemented but not wired in |
| 7 | HIGH | No incoming connection concurrency limit |
| 8 | HIGH | No RPC authentication on daemon socket |

Additional fix bundled: socket permission race condition on daemon startup.

---

## Fix 1: SQLCipher Encryption via Argon2id Passphrase

### Problem

`Database::open()` never issues `PRAGMA key`. Despite bundling SQLCipher, the database is stored as plaintext. All private keys, messages, Signal sessions, and PreKey material are exposed to any process with file read access.

### Design

**Key derivation:** Argon2id with OWASP-recommended parameters:
- Memory: 64 MB (`m = 65536`)
- Iterations: 3 (`t = 3`)
- Parallelism: 4 (`p = 4`)
- Output: 32 bytes (256-bit key)

**Salt:** 16 random bytes generated on first run, stored in `{data_dir}/chattor.salt` (file permissions 0600).

**Passphrase flow:**
- **TUI mode:** Password input screen shown before main UI. First launch prompts for new passphrase (min 8 chars, confirmation required). Subsequent launches prompt for existing passphrase.
- **Daemon mode:** Reads passphrase from file descriptor specified by `--passphrase-fd N` flag, or prompts on stdin if it's a TTY.
- **CLI commands:** Don't need the passphrase — they communicate via Unix socket to the already-running daemon.

**Database open changes:**
```rust
// New signature
Database::open_encrypted<P: AsRef<Path>>(path: P, key: &[u8; 32]) -> Result<Self>

// Immediately after Connection::open_with_flags():
conn.execute_batch(&format!(
    "PRAGMA key = \"x'{}'\";
     PRAGMA cipher_page_size = 4096;",
    hex::encode(key)
))?;
// Then existing PRAGMAs (WAL, synchronous, etc.)
```

**Migration of existing unencrypted databases:**
1. Detect unencrypted DB: attempt `SELECT count(*) FROM schema_version` without key — if it succeeds, DB is unencrypted.
2. Prompt user to set a passphrase.
3. Create new encrypted DB at `{path}.encrypted`.
4. `ATTACH '{path}.encrypted' AS encrypted KEY 'x''...''';`
5. Copy all tables via `sqlcipher_export('encrypted')`.
6. Detach, replace old file with new encrypted file.

**Dependencies:** `argon2 = "0.5"` (already in Cargo.toml but unused), `hex` (for key encoding).

### Files Changed
- `src/db/connection.rs` — New `open_encrypted()`, `PRAGMA key`, migration logic
- `src/app.rs` — Pass key to `Database::open_encrypted()`
- `src/main.rs` — Passphrase prompt before app init (TUI mode)
- `src/daemon/mod.rs` — `--passphrase-fd` handling
- `src/cli.rs` — Add `--passphrase-fd` flag to daemon subcommand

---

## Fix 2: Friend Request Signature Verification (TOFU)

### Problem

`validate_request()` exists but is never called. Even if called, it verifies against a public key extracted from the `.onion` address — but the Ed25519 identity key and the arti-managed .onion key are completely independent, so verification would always fail.

### Design

**TOFU (Trust On First Use) model:**
- Include the Ed25519 public key (base64) in friend request and accept messages.
- On first contact, verify the signature against the included pubkey.
- Store the pubkey in the `friends` table.
- On subsequent interactions, verify against the stored pubkey.
- This is the standard SSH-style trust model.

**Wire format changes:**
```rust
// Add to FriendRequestMessage:
pub ed25519_pubkey: String,  // base64-encoded Ed25519 public key

// Add to FriendRequestAcceptMessage:
pub ed25519_pubkey: String,  // base64-encoded Ed25519 public key
```

**Schema v10 migration:**
```sql
ALTER TABLE friends ADD COLUMN ed25519_pubkey BLOB;
```

**Verification flow — incoming friend request:**
1. Decode `ed25519_pubkey` from the message.
2. Reconstruct signed data: `format!("{}{}{}", from_onion, from_friendcode, timestamp)`.
3. Verify Ed25519 signature against the included pubkey.
4. Reject if signature invalid or timestamp >5 minutes old.
5. Store request only if verification passes.

**Verification flow — incoming accept:**
1. Decode `ed25519_pubkey` from the accept message.
2. Verify Ed25519 signature against the included pubkey.
3. Verify PreKeyBundle VXEdDSA signature (`bundle.verify_signature()`).
4. Reject if either verification fails.
5. Store pubkey in `friends` table.

### Files Changed
- `src/protocol/message.rs` — Add `ed25519_pubkey` field to both message types
- `src/protocol/friend_request.rs` — Include pubkey in `create_request()`, update `validate_request()` to use included pubkey
- `src/handlers/messaging.rs` — Call validation before storing friend requests
- `src/handlers/friend_request.rs` — Verify accept signatures and PreKeyBundle VXEdDSA
- `src/db/connection.rs` — Schema v10 migration
- `src/db/schema.rs` — Update schema version, add column to friends table

---

## Fix 3: Memory Zeroization

### Problem

`PreKeyPrivateMaterial`, intermediate `[u8; 32]` secrets, and Signal session state are never zeroized when dropped. `PreKeyPrivateMaterial` derives `Debug`, making accidental logging of secrets trivial.

### Design

- Add `zeroize = { version = "1", features = ["derive"] }` as a direct dependency.
- `#[derive(Zeroize, ZeroizeOnDrop)]` on `PreKeyPrivateMaterial`.
- Replace `#[derive(Debug)]` on `PreKeyPrivateMaterial` with a custom impl that prints `PreKeyPrivateMaterial { [REDACTED] }`.
- Wrap intermediate `[u8; 32]` secret variables in handlers with `zeroize::Zeroizing<>`.

### Files Changed
- `Cargo.toml` — Add `zeroize` dependency
- `src/crypto/signal.rs` — Zeroize derives on `PreKeyPrivateMaterial`, custom Debug
- `src/handlers/friend_request.rs` — Wrap intermediate secrets in `Zeroizing`
- `src/handlers/messaging.rs` — Wrap intermediate secrets in `Zeroizing`

---

## Fix 4: PreKey Material TTL Cleanup

### Problem

When a friend request is accepted, X25519 private key material is stored in `app_settings`. If the peer never completes the handshake (goes offline, queue expires), this material persists forever.

### Design

- Store `prekey_created_at:{onion}` timestamp alongside each prekey entry.
- Add periodic cleanup task (runs every hour) that deletes prekey entries older than 7 days.
- Log warnings when stale material is cleaned up.

### Files Changed
- `src/handlers/friend_request.rs` — Store timestamp alongside prekey material
- `src/main.rs` — Spawn periodic cleanup task
- `src/daemon/tasks.rs` — Add cleanup task for daemon mode
- New function in `src/db/queries.rs` — `cleanup_stale_prekey_material()`

---

## Fix 5: Wire Rate Limiter

### Problem

`RateLimiter` is fully implemented and tested but never instantiated. No inbound message throttling exists. A malicious peer can flood the node.

### Design

- Add `rate_limiter: Arc<RateLimiter>` to `App` struct, initialized with defaults (5/s sustained, 20 burst).
- In the incoming message handler path: extract `from_onion` from the message, call `rate_limiter.check(from_onion)`.
- Rate-limited messages are silently dropped with a `tracing::warn!` log.
- Remove `#![allow(dead_code)]` from `rate_limit.rs`.
- The rate limiter check happens in `handle_incoming_message()` as the first operation, before any crypto or DB work.

### Files Changed
- `src/app.rs` — Add `rate_limiter` field
- `src/net/rate_limit.rs` — Remove `#![allow(dead_code)]`
- `src/handlers/messaging.rs` — Add rate limit check at top of `handle_incoming_message()`
- `src/main.rs` — Pass rate limiter to handler

---

## Fix 6: Incoming Connection Concurrency Limit

### Problem

`listen_for_tor_connections()` spawns unbounded tasks per incoming connection. An attacker can exhaust memory via connection flood.

### Design

- Add `Semaphore::new(50)` shared across all incoming connection handlers.
- Acquire permit before `tokio::spawn` for each stream handler.
- If all 50 permits are held, new connections wait (backpressure via the Tor rendezvous stream).
- Also apply to `listen_for_connections()` (TCP fallback) for consistency.

### Files Changed
- `src/net/listener.rs` — Add semaphore parameter, acquire before spawn

---

## Fix 7: Daemon Token Auth

### Problem

Any same-user process can connect to the Unix socket and execute any RPC method — reading messages, sending as the user, managing friends.

### Design

- On daemon start: generate 32-byte random token via `rand::thread_rng()`.
- Write token to `{data_dir}/daemon.token` with 0600 permissions.
- CLI client: reads token file, sends as `"auth": "<hex>"` field in first JSON-RPC request.
- Daemon: first request per connection must include valid `auth` field. Reject with error code -32001 ("Unauthorized") if missing or wrong. Subsequent requests on the same connection don't need to re-auth (connection is "authenticated").
- Clean up token file on daemon shutdown.
- MCP server (stdio) skips auth — already same-process.

### Files Changed
- `src/daemon/mod.rs` — Generate token on start, clean up on shutdown
- `src/daemon/socket.rs` — Validate auth on first request per connection
- `src/daemon/rpc.rs` — Add auth-related error code
- `src/client.rs` — Read token and include in requests
- `src/mcp/server.rs` — No changes (stdio, no auth needed)

---

## Fix 8: Socket Permission Race

### Problem

Socket is created with default umask, then `chmod 0600` is applied. Brief window where socket is world-accessible. `set_permissions` error is silently ignored.

### Design

- Set process umask to `0o077` before `UnixListener::bind()`, restore original umask after.
- Fail hard (propagate error, not `.ok()`) if `set_permissions` returns an error.

### Files Changed
- `src/daemon/socket.rs` — Umask handling, error propagation

---

## Testing Strategy

- **Unit tests:** For Argon2 key derivation, TOFU validation, rate limiter wiring, token auth.
- **Integration tests:** Encrypted DB open/close cycle, migration from unencrypted to encrypted, friend request with signature verification end-to-end.
- **Existing tests:** Must continue passing — `cargo test` green throughout.
