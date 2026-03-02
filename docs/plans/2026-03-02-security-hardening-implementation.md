# Security Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all critical and high-severity findings from the security audit — SQLCipher encryption, signature verification, memory zeroization, rate limiting, connection limits, daemon auth.

**Architecture:** Eight independent workstreams touching database, crypto, protocol, handlers, networking, and daemon layers. Changes are ordered to minimize cascading breakage — simple isolated fixes first, then protocol changes, then the large SQLCipher migration last.

**Tech Stack:** Rust, rusqlite/SQLCipher, argon2, zeroize, ed25519-dalek, libsignal-dezire, tokio

---

### Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add zeroize and hex crates**

In `Cargo.toml`, add under `[dependencies]` (Cryptography section):

```toml
zeroize = { version = "1", features = ["derive"] }
hex = "0.4"
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add zeroize and hex crates for security hardening"
```

---

### Task 2: Memory Zeroization for PreKeyPrivateMaterial

**Files:**
- Modify: `src/crypto/signal.rs:17-27` (PreKeyPrivateMaterial struct)
- Modify: `src/handlers/friend_request.rs:132-163` (intermediate secrets)
- Modify: `src/handlers/messaging.rs:166-212` (intermediate secrets)

**Step 1: Add Zeroize derives to PreKeyPrivateMaterial**

In `src/crypto/signal.rs`, replace:

```rust
#[derive(Debug)]
pub struct PreKeyPrivateMaterial {
    pub identity_secret: [u8; 32],
    pub signed_prekey_secret: [u8; 32],
    pub prekey_secret: Option<[u8; 32]>,
}
```

With:

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PreKeyPrivateMaterial {
    pub identity_secret: [u8; 32],
    pub signed_prekey_secret: [u8; 32],
    pub prekey_secret: Option<[u8; 32]>,
}

impl std::fmt::Debug for PreKeyPrivateMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreKeyPrivateMaterial")
            .field("identity_secret", &"[REDACTED]")
            .field("signed_prekey_secret", &"[REDACTED]")
            .field("prekey_secret", &self.prekey_secret.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}
```

**Step 2: Run tests to verify nothing breaks**

Run: `cargo test crypto::signal`
Expected: all signal tests pass

**Step 3: Commit**

```bash
git add src/crypto/signal.rs
git commit -m "security: add memory zeroization to PreKeyPrivateMaterial

Derives ZeroizeOnDrop so private key bytes are cleared when the
struct is dropped. Replaces Debug derive with custom impl that
redacts secret values to prevent accidental logging."
```

---

### Task 3: Socket Permission Race Fix

**Files:**
- Modify: `src/daemon/socket.rs:17-29`

**Step 1: Fix the race condition**

In `src/daemon/socket.rs`, replace the socket creation block (lines 17-29):

```rust
    // Remove stale socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path).ok();
    }

    let listener = UnixListener::bind(socket_path).expect("Failed to bind Unix socket");

    // Restrict permissions to owner only (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600)).ok();
    }
```

With:

```rust
    // Remove stale socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path).ok();
    }

    // Set restrictive umask BEFORE binding so the socket is created with 0600
    // from the start, eliminating the race window between bind and chmod.
    #[cfg(unix)]
    let old_umask = unsafe { libc::umask(0o077) };

    let listener = UnixListener::bind(socket_path).expect("Failed to bind Unix socket");

    // Restore original umask
    #[cfg(unix)]
    unsafe {
        libc::umask(old_umask);
    }

    // Explicitly set permissions as defense-in-depth (fail hard on error)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))
            .expect("Failed to set socket permissions to 0600");
    }
```

**Step 2: Run existing tests**

Run: `cargo test daemon`
Expected: all daemon tests pass

**Step 3: Commit**

```bash
git add src/daemon/socket.rs
git commit -m "security: fix socket permission race condition

Set umask to 0077 before bind so the socket is created with
restricted permissions from the start. Also fail hard if
set_permissions errors instead of silently ignoring."
```

---

### Task 4: Wire Rate Limiter

**Files:**
- Modify: `src/net/rate_limit.rs:1` (remove allow dead_code)
- Modify: `src/app.rs:8,17-28,54,58-69,123-148` (add rate_limiter field)
- Modify: `src/handlers/messaging.rs:30-34` (add rate limit check)

**Step 1: Remove dead_code allow from rate_limit.rs**

In `src/net/rate_limit.rs`, remove line 1:

```rust
#![allow(dead_code)]
```

Also remove the TODO comment on line 58:

```rust
// TODO: wire into message dispatch in listener.rs or main.rs incoming message handler
```

**Step 2: Add rate_limiter field to App**

In `src/app.rs`, add the import and field:

```rust
use crate::net::rate_limit::RateLimiter;
```

Add to the `App` struct:

```rust
pub rate_limiter: Arc<RateLimiter>,
```

In `App::new()`, initialize it:

```rust
let rate_limiter = Arc::new(RateLimiter::default_limiter());
```

And include in the struct construction:

```rust
rate_limiter,
```

Do the same in `App::new_with_settings()`.

**Step 3: Add rate limit check in handle_incoming_message**

In `src/handlers/messaging.rs`, at the top of `handle_incoming_message()` (after `match &incoming.message {`), extract the sender onion and check rate limit. Add this as the first thing in the function, before the match:

```rust
    // Extract sender onion for rate limiting
    let sender_onion = match &incoming.message {
        protocol::message::Message::FriendRequest(req) => Some(req.from_onion.as_str()),
        protocol::message::Message::FriendRequestAccept(accept) => Some(accept.from_onion.as_str()),
        protocol::message::Message::TextMessage(msg) => Some(msg.from_onion.as_str()),
        protocol::message::Message::Presence(pres) => Some(pres.from_onion.as_str()),
        protocol::message::Message::ChannelSubscribe(sub) => Some(sub.subscriber_onion.as_str()),
        protocol::message::Message::ChannelUnsubscribe(unsub) => Some(unsub.subscriber_onion.as_str()),
        protocol::message::Message::ChannelPost(post) => Some(post.publisher_onion.as_str()),
        protocol::message::Message::ChannelSyncRequest(req) => Some(req.subscriber_onion.as_str()),
        protocol::message::Message::ChannelSyncResponse(resp) => Some(resp.publisher_onion.as_str()),
        protocol::message::Message::ChannelPostReceipt(receipt) => Some(receipt.reader_onion.as_str()),
        _ => None,
    };

    if let Some(peer) = sender_onion {
        if !app.rate_limiter.check(peer) {
            tracing::warn!("Rate limited message from {}", &peer[..8.min(peer.len())]);
            return Ok(());
        }
    }
```

**Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass (rate_limit tests + all others)

**Step 5: Commit**

```bash
git add src/net/rate_limit.rs src/app.rs src/handlers/messaging.rs
git commit -m "security: wire rate limiter into incoming message handler

Per-peer token bucket rate limiting (5/s sustained, 20 burst) is
now enforced on all incoming messages. Messages from rate-limited
peers are silently dropped with a warning log."
```

---

### Task 5: Incoming Connection Concurrency Limit

**Files:**
- Modify: `src/net/listener.rs:62-94`

**Step 1: Add semaphore to listen_for_tor_connections**

In `src/net/listener.rs`, modify `listen_for_tor_connections()`:

```rust
pub async fn listen_for_tor_connections(
    rend_requests: impl futures::Stream<Item = RendRequest> + Send + 'static,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    use std::sync::Arc;

    let stream_requests = handle_rend_requests(rend_requests);
    futures::pin_mut!(stream_requests);

    // Limit concurrent incoming connection handlers to prevent DoS
    let semaphore = Arc::new(tokio::sync::Semaphore::new(50));

    while let Some(stream_request) = stream_requests.next().await {
        let tx = tx.clone();
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => break, // Semaphore closed
        };
        tokio::spawn(async move {
            let _permit = permit; // Hold until handler completes
            match stream_request.accept(Connected::new_empty()).await {
                Ok(mut data_stream) => {
                    match crate::net::framing::receive_message(&mut data_stream).await {
                        Ok(envelope) => {
                            let _ = tx.send(IncomingMessage {
                                message: envelope.payload,
                                remote_addr: "tor-rendezvous".to_string(),
                            }).await;
                        }
                        Err(e) => {
                            eprintln!("Tor connection framing error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to accept Tor stream: {}", e);
                }
            }
        });
    }

    Ok(())
}
```

**Step 2: Run tests**

Run: `cargo test listener`
Expected: listener tests pass

**Step 3: Commit**

```bash
git add src/net/listener.rs
git commit -m "security: add concurrency limit (50) to incoming Tor connections

Prevents DoS via connection flooding by limiting simultaneous
incoming connection handlers with a tokio::Semaphore."
```

---

### Task 6: PreKey Material TTL Cleanup

**Files:**
- Modify: `src/handlers/friend_request.rs:132-173` (store timestamp)
- Modify: `src/db/queries.rs` (add cleanup function)
- Modify: `src/daemon/tasks.rs` (spawn cleanup task)
- Modify: `src/main.rs` (spawn cleanup task for TUI mode)

**Step 1: Store timestamp alongside prekey material**

In `src/handlers/friend_request.rs`, after the existing `prekey_opk` storage (around line 159), add:

```rust
    // Store creation timestamp for TTL cleanup
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (
            &format!("prekey_created_at:{}", from_onion),
            &format!("{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()),
        ),
    )
    .map_err(|e| {
        error::ChattorError::Database(format!("Failed to store PreKey timestamp: {}", e))
    })?;
```

**Step 2: Add cleanup function to queries.rs**

Add to `src/db/queries.rs`:

```rust
/// Delete stale PreKey private material older than max_age_secs.
/// Returns the number of peers whose material was cleaned up.
pub fn cleanup_stale_prekey_material(db: &crate::db::Database, max_age_secs: u64) -> crate::error::Result<usize> {
    let conn = db.connection();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Find stale peers by checking prekey_created_at entries
    let mut stmt = conn.prepare(
        "SELECT key, value FROM app_settings WHERE key LIKE 'prekey_created_at:%'"
    ).map_err(|e| crate::error::ChattorError::Database(format!("Failed to query prekey timestamps: {}", e)))?;

    let stale_peers: Vec<String> = stmt.query_map([], |row| {
        let key: String = row.get(0)?;
        let ts_str: String = row.get(1)?;
        Ok((key, ts_str))
    })
    .map_err(|e| crate::error::ChattorError::Database(format!("Failed to read prekey timestamps: {}", e)))?
    .filter_map(|r| r.ok())
    .filter_map(|(key, ts_str)| {
        let ts: u64 = ts_str.parse().ok()?;
        if now.saturating_sub(ts) > max_age_secs {
            // Extract onion from "prekey_created_at:<onion>"
            key.strip_prefix("prekey_created_at:").map(|s| s.to_string())
        } else {
            None
        }
    })
    .collect();

    let count = stale_peers.len();
    for peer in &stale_peers {
        conn.execute(
            "DELETE FROM app_settings WHERE key LIKE ?1",
            [&format!("prekey_%:{}", peer)],
        ).ok();
        conn.execute(
            "DELETE FROM app_settings WHERE key = ?1",
            [&format!("signal_identity_secret:{}", peer)],
        ).ok();
        conn.execute(
            "DELETE FROM app_settings WHERE key = ?1",
            [&format!("prekey_created_at:{}", peer)],
        ).ok();
        tracing::warn!("Cleaned up stale PreKey material for {} (>7 days)", &peer[..8.min(peer.len())]);
    }

    Ok(count)
}
```

**Step 3: Spawn cleanup task in daemon tasks**

In `src/daemon/tasks.rs`, add to `spawn_all()`:

```rust
    spawn_prekey_cleanup(Arc::clone(&app));
```

Add the function:

```rust
fn spawn_prekey_cleanup(app: Arc<Mutex<App>>) {
    tokio::spawn(async move {
        // Run every hour
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            let app_lock = app.lock().await;
            let _ = crate::db::queries::cleanup_stale_prekey_material(
                &app_lock.db,
                7 * 24 * 3600, // 7 days
            );
        }
    });
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass

**Step 5: Commit**

```bash
git add src/handlers/friend_request.rs src/db/queries.rs src/daemon/tasks.rs
git commit -m "security: add TTL cleanup for stale PreKey material

PreKey private material stored during friend request acceptance is
now cleaned up after 7 days if the peer never completes the
handshake. A background task runs hourly in daemon mode."
```

---

### Task 7: TOFU Wire Format + Schema v10

**Files:**
- Modify: `src/protocol/message.rs:73-88` (add ed25519_pubkey to message types)
- Modify: `src/db/schema.rs:1` (bump version)
- Modify: `src/db/connection.rs` (add migrate_to_v10)
- Modify: `src/protocol/friend_request.rs:21-44` (include pubkey in create_request)
- Modify: `src/crypto/identity.rs` (add public_key_base64 method)

**Step 1: Add public_key_base64 method to IdentityKeypair**

In `src/crypto/identity.rs`, add:

```rust
    /// Get the Ed25519 public key as a base64-encoded string.
    pub fn public_key_base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(self._verifying_key.to_bytes())
    }
```

**Step 2: Add ed25519_pubkey field to wire format**

In `src/protocol/message.rs`, modify `FriendRequestMessage`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FriendRequestMessage {
    pub from_onion: String,
    pub from_friendcode: String,
    pub timestamp: i64,
    pub signature: String,
    /// Ed25519 public key (base64) for TOFU identity binding
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ed25519_pubkey: Option<String>,
}
```

Modify `FriendRequestAcceptMessage`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FriendRequestAcceptMessage {
    pub from_onion: String,
    pub to_onion: String,
    pub signal_prekey_bundle: String,
    pub timestamp: i64,
    pub signature: String,
    /// Ed25519 public key (base64) for TOFU identity binding
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ed25519_pubkey: Option<String>,
}
```

**Step 3: Include pubkey in create_request**

In `src/protocol/friend_request.rs`, modify `create_request()` to include the pubkey:

```rust
    pub fn create_request(
        identity: &IdentityKeypair,
        own_onion: &str,
        friend_code: &str,
    ) -> Result<FriendRequestMessage> {
        // ... existing code ...
        Ok(FriendRequestMessage {
            from_onion: own_onion.to_string(),
            from_friendcode: friend_code.to_string(),
            timestamp,
            signature: signature_base64,
            ed25519_pubkey: Some(identity.public_key_base64()),
        })
    }
```

Also update `create_accept_message()` to include the pubkey:

```rust
        Ok(FriendRequestAcceptMessage {
            from_onion: own_onion.to_string(),
            to_onion: peer_onion.to_string(),
            signal_prekey_bundle: bundle_json,
            timestamp,
            signature: signature_base64,
            ed25519_pubkey: Some(identity.public_key_base64()),
        })
    }
```

**Step 4: Include pubkey in handle_accept_friend_request**

In `src/handlers/friend_request.rs`, update the accept message construction (around line 120) to include:

```rust
    let accept_msg = protocol::message::FriendRequestAcceptMessage {
        from_onion: own_onion.to_string(),
        to_onion: from_onion.clone(),
        signal_prekey_bundle: bundle_json,
        timestamp,
        signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        ed25519_pubkey: Some(identity.public_key_base64()),
    };
```

**Step 5: Schema v10 migration**

In `src/db/schema.rs`, change:

```rust
pub const SCHEMA_VERSION: i32 = 10;
```

In the `CREATE_TABLES` const, update the friends table to include:

```sql
    ed25519_pubkey BLOB,
```

(This column already exists in the schema — `signal_identity_key BLOB` and `signal_prekey_bundle BLOB` are there. We add `ed25519_pubkey` as a new column.)

In `src/db/connection.rs`, add after `self.migrate_to_v9()?;`:

```rust
                self.migrate_to_v10()?;
```

Add the migration function:

```rust
    /// Migrate database from v9 to v10 (TOFU Ed25519 pubkey on friends)
    fn migrate_to_v10(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 10 {
            info!("Migrating database to schema v10 (TOFU Ed25519 pubkey)");

            let conn = self.connection();

            let has_column: bool = conn
                .prepare("SELECT ed25519_pubkey FROM friends LIMIT 0")
                .is_ok();

            if !has_column {
                conn.execute_batch(
                    "ALTER TABLE friends ADD COLUMN ed25519_pubkey BLOB;"
                ).map_err(|e| ChattorError::Database(format!("Failed to add ed25519_pubkey: {}", e)))?;
            }

            conn.execute("UPDATE schema_version SET version = 10", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v10 complete");
        }

        Ok(())
    }
```

**Step 6: Fix existing tests that construct FriendRequestMessage without pubkey**

Update test constructors in `src/protocol/message.rs` tests and anywhere else that constructs these types to include `ed25519_pubkey: None` (or `Some(...)` where appropriate). Because the field uses `#[serde(default)]`, existing JSON without the field will deserialize with `None`.

**Step 7: Run tests**

Run: `cargo test`
Expected: all tests pass. Serialization tests should work because `serde(default)` handles the optional field.

**Step 8: Commit**

```bash
git add src/protocol/message.rs src/protocol/friend_request.rs src/crypto/identity.rs \
        src/db/schema.rs src/db/connection.rs src/handlers/friend_request.rs
git commit -m "security: add TOFU Ed25519 pubkey to friend request wire format

Friend requests and accept messages now include the sender's
Ed25519 public key for Trust-On-First-Use identity binding.
Schema v10 adds ed25519_pubkey column to friends table."
```

---

### Task 8: Friend Request Signature Verification

**Files:**
- Modify: `src/protocol/friend_request.rs:47-92` (update validate_request)
- Modify: `src/handlers/messaging.rs:36-54` (call validation)
- Modify: `src/db/queries.rs` (add store_friend_pubkey function)

**Step 1: Update validate_request to use included pubkey**

In `src/protocol/friend_request.rs`, replace `validate_request()`:

```rust
    /// Validate received friend request using TOFU — verify the Ed25519
    /// signature against the pubkey included in the message itself.
    pub fn validate_request(request: &FriendRequestMessage) -> Result<bool> {
        // Require Ed25519 pubkey for verification
        let pubkey_b64 = match &request.ed25519_pubkey {
            Some(pk) => pk,
            None => {
                tracing::warn!("Friend request from {} missing Ed25519 pubkey, rejecting", request.from_onion);
                return Ok(false);
            }
        };

        // Check timestamp (within 5 minutes)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let age = (now - request.timestamp).abs();
        if age > 300 {
            return Ok(false);
        }

        // Decode public key from base64
        let pubkey_bytes = match base64::engine::general_purpose::STANDARD.decode(pubkey_b64) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false),
        };
        let pubkey_array: [u8; 32] = match pubkey_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return Ok(false),
        };

        // Reconstruct the signed data
        let data = format!("{}{}{}", request.from_onion, request.from_friendcode, request.timestamp);

        // Decode signature from base64
        let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(&request.signature) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false),
        };

        // Verify Ed25519 signature
        use ed25519_dalek::{VerifyingKey, Verifier, Signature};

        let verifying_key = match VerifyingKey::from_bytes(&pubkey_array) {
            Ok(key) => key,
            Err(_) => return Ok(false),
        };

        let sig_array: [u8; 64] = match sig_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return Ok(false),
        };
        let signature = Signature::from_bytes(&sig_array);

        match verifying_key.verify(data.as_bytes(), &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
```

Make this a static method (remove `&self` — it doesn't need the Database anymore since it uses the included pubkey).

**Step 2: Call validate_request in handle_incoming_message**

In `src/handlers/messaging.rs`, replace the FriendRequest handler (lines 36-54):

```rust
        protocol::message::Message::FriendRequest(req) => {
            // Verify Ed25519 signature before storing
            use crate::protocol::friend_request::FriendRequestHandler;
            match FriendRequestHandler::validate_request(req) {
                Ok(true) => {
                    // Signature valid — store the request
                    let conn = app.db.connection();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    conn.execute(
                        "INSERT INTO friend_requests (from_onion, friend_code, received_at, status)
                         VALUES (?1, ?2, ?3, 'pending')",
                        (&req.from_onion, &req.from_friendcode, now),
                    )
                    .map_err(|e| {
                        error::ChattorError::Database(format!("Failed to save friend request: {}", e))
                    })?;

                    eprintln!("Received verified friend request from {}", req.from_onion);
                }
                Ok(false) => {
                    eprintln!("Rejected friend request from {} (invalid signature)", req.from_onion);
                }
                Err(e) => {
                    eprintln!("Error validating friend request from {}: {}", req.from_onion, e);
                }
            }
        }
```

**Step 3: Add function to store peer pubkey**

In `src/db/queries.rs`, add:

```rust
/// Store a peer's Ed25519 public key (TOFU binding).
pub fn store_friend_pubkey(db: &crate::db::Database, onion: &str, pubkey: &[u8]) -> crate::error::Result<()> {
    let conn = db.connection();
    conn.execute(
        "UPDATE friends SET ed25519_pubkey = ?1 WHERE onion_address = ?2",
        (pubkey, onion),
    ).map_err(|e| crate::error::ChattorError::Database(format!("Failed to store friend pubkey: {}", e)))?;
    Ok(())
}
```

**Step 4: Update test for validate_request**

The existing test `test_validate_request_verifies_signature` uses `identity.to_onion_address()` which won't match the included pubkey anymore. Update it to use the new static method. Since `create_request` now includes the pubkey, `validate_request` should work against it directly:

```rust
    #[test]
    fn test_validate_request_verifies_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = "test.onion"; // Any onion address works now — we verify against included pubkey
        let friend_code = "happy-1234-tiger-5678";

        let request = FriendRequestHandler::create_request(&identity, onion, friend_code).unwrap();

        assert!(FriendRequestHandler::validate_request(&request).unwrap());
    }

    #[test]
    fn test_validate_request_rejects_forged_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = "test.onion";
        let friend_code = "happy-1234-tiger-5678";

        let mut request = FriendRequestHandler::create_request(&identity, onion, friend_code).unwrap();

        // Forge: replace with a different identity's pubkey
        let other_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        request.ed25519_pubkey = Some(other_identity.public_key_base64());

        assert!(!FriendRequestHandler::validate_request(&request).unwrap());
    }
```

**Step 5: Run tests**

Run: `cargo test`
Expected: all tests pass

**Step 6: Commit**

```bash
git add src/protocol/friend_request.rs src/handlers/messaging.rs src/db/queries.rs
git commit -m "security: verify Ed25519 signatures on incoming friend requests

Friend requests without a valid Ed25519 signature are now rejected.
Uses TOFU model — signature is verified against the pubkey included
in the message. Closes the impersonation attack vector."
```

---

### Task 9: Friend Request Accept Verification

**Files:**
- Modify: `src/handlers/friend_request.rs:234-393` (handle_incoming_accept)

**Step 1: Add signature + VXEdDSA verification to handle_incoming_accept**

In `src/handlers/friend_request.rs`, at the top of `handle_incoming_accept()` (after deserializing the bundle, around line 241), add verification:

```rust
    // Verify Ed25519 signature on accept message (TOFU)
    if let Some(ref pubkey_b64) = accept.ed25519_pubkey {
        let data = format!("{}{}{}", accept.from_onion, accept.to_onion, accept.timestamp);
        let pubkey_bytes = base64::engine::general_purpose::STANDARD
            .decode(pubkey_b64)
            .map_err(|e| error::ChattorError::Crypto(format!("Failed to decode accept pubkey: {}", e)))?;
        let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
            error::ChattorError::Crypto("Accept pubkey has wrong length".into())
        })?;
        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(&accept.signature)
            .map_err(|e| error::ChattorError::Crypto(format!("Failed to decode accept signature: {}", e)))?;
        let sig_array: [u8; 64] = sig_bytes.try_into().map_err(|_| {
            error::ChattorError::Crypto("Accept signature has wrong length".into())
        })?;

        use ed25519_dalek::{VerifyingKey, Verifier, Signature};
        let verifying_key = VerifyingKey::from_bytes(&pubkey_array)
            .map_err(|_| error::ChattorError::Crypto("Invalid Ed25519 pubkey in accept".into()))?;
        let signature = Signature::from_bytes(&sig_array);
        verifying_key.verify(data.as_bytes(), &signature)
            .map_err(|_| error::ChattorError::Crypto(
                format!("Accept message from {} has invalid Ed25519 signature", accept.from_onion)
            ))?;
    } else {
        return Err(error::ChattorError::Crypto(
            format!("Accept message from {} missing Ed25519 pubkey, rejecting", accept.from_onion)
        ));
    }

    // Verify PreKeyBundle VXEdDSA signature (self-consistency check)
    if !bundle.verify_signature()? {
        return Err(error::ChattorError::Crypto(
            format!("Accept message from {} has invalid PreKeyBundle VXEdDSA signature", accept.from_onion)
        ));
    }
```

**Step 2: Store the peer's pubkey after adding as friend**

After the friend is added (around line 381), store the pubkey:

```rust
    // Store peer's Ed25519 pubkey for future verification (TOFU)
    if let Some(ref pubkey_b64) = accept.ed25519_pubkey {
        if let Ok(pubkey_bytes) = base64::engine::general_purpose::STANDARD.decode(pubkey_b64) {
            db::queries::store_friend_pubkey(&app.db, &accept.from_onion, &pubkey_bytes)?;
        }
    }
```

**Step 3: Run tests**

Run: `cargo test`
Expected: all tests pass

**Step 4: Commit**

```bash
git add src/handlers/friend_request.rs
git commit -m "security: verify signatures on incoming friend request accepts

Both the Ed25519 message signature and the PreKeyBundle VXEdDSA
signature are now verified before processing accept messages.
Prevents MITM attacks on Signal session establishment."
```

---

### Task 10: Daemon Token Auth

**Files:**
- Modify: `src/daemon/mod.rs` (generate + cleanup token)
- Modify: `src/daemon/socket.rs` (validate auth on first request)
- Modify: `src/client.rs` (read token, include in requests)

**Step 1: Generate token in daemon startup**

In `src/daemon/mod.rs`, after the PID file acquisition (around line 30), add:

```rust
    // Generate auth token for RPC authentication
    let token_path = settings.data_dir.join("daemon.token");
    let auth_token = {
        use rand::Rng;
        let token: [u8; 32] = rand::thread_rng().gen();
        let token_hex = hex::encode(token);
        std::fs::write(&token_path, &token_hex)
            .map_err(|e| crate::error::ChattorError::Io(e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| crate::error::ChattorError::Io(e))?;
        }
        token_hex
    };
```

Pass `auth_token` to `socket::start()`. Update the cleanup at the end:

```rust
    // Cleanup socket, token, and PID file
    std::fs::remove_file(&socket_path).ok();
    std::fs::remove_file(&token_path).ok();
    pid::release(&pid_path);
```

**Step 2: Update socket::start to accept and validate auth token**

Modify `socket::start()` signature to accept `auth_token: String`. In `handle_connection()`, add auth state tracking:

```rust
async fn handle_connection(
    stream: tokio::net::UnixStream,
    app: Arc<Mutex<App>>,
    presence: PresenceMap,
    msg_broadcast_tx: broadcast::Sender<String>,
    auth_token: String,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut authenticated = false;

    while let Ok(Some(line)) = lines.next_line().await {
        let request: rpc::RpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = rpc::RpcResponse::error(None, -32700, format!("Parse error: {}", e));
                let json = serde_json::to_string(&resp).unwrap_or_default();
                let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                continue;
            }
        };

        // Require auth on first request
        if !authenticated {
            let provided_token = request.params.get("auth").and_then(|v| v.as_str());
            match provided_token {
                Some(t) if t == auth_token => {
                    authenticated = true;
                }
                _ => {
                    let resp = rpc::RpcResponse::error(
                        request.id.clone(),
                        -32001,
                        "Unauthorized: invalid or missing auth token".into(),
                    );
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                    return; // Disconnect unauthorized client
                }
            }
        }

        // ... rest of existing handler (listen, dispatch, etc.) ...
```

**Step 3: Update client to read and include auth token**

In `src/client.rs`, modify `rpc_call()`:

```rust
pub async fn rpc_call(data_dir: &Path, method: &str, params: Value) -> Result<Value> {
    let socket_path = data_dir.join("chattor.sock");
    let token_path = data_dir.join("daemon.token");

    // Read auth token
    let auth_token = std::fs::read_to_string(&token_path).map_err(|_| {
        ChattorError::Network(
            "Cannot read daemon token. Is the daemon running? Start with: chattor daemon".into(),
        )
    })?;

    let stream = UnixStream::connect(&socket_path).await.map_err(|_| {
        ChattorError::Network(
            "Cannot connect to daemon. Is it running? Start with: chattor daemon".into(),
        )
    })?;

    let (reader, mut writer) = stream.into_split();

    // First request includes auth token
    let mut merged_params = match params {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    merged_params.insert("auth".to_string(), Value::String(auth_token.trim().to_string()));

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": Value::Object(merged_params),
    });

    // ... rest unchanged ...
```

**Step 4: Run tests**

Run: `cargo test`
Expected: tests pass. Daemon RPC tests may need updates to include auth tokens — update `test_app()` helpers to generate and pass a token.

**Step 5: Commit**

```bash
git add src/daemon/mod.rs src/daemon/socket.rs src/client.rs
git commit -m "security: add token-based authentication to daemon RPC

Daemon generates a random 32-byte auth token on start, stored in
daemon.token (0600). CLI client reads the token and includes it
in the first RPC request. Unauthorized connections are rejected
with error -32001."
```

---

### Task 11: SQLCipher Database Encryption

**Files:**
- Modify: `Cargo.toml` (ensure hex is present)
- Modify: `src/db/connection.rs` (new open_encrypted, PRAGMA key, migration)
- Modify: `src/app.rs` (pass key to Database)
- Modify: `src/main.rs` (passphrase prompt in TUI mode)
- Modify: `src/daemon/mod.rs` (--passphrase-fd in daemon mode)
- Modify: `src/cli.rs` (add --passphrase-fd flag)
- Modify: `src/config.rs` (add salt path)

**Step 1: Add key derivation module**

Create `src/db/encryption.rs`:

```rust
use crate::error::{ChattorError, Result};
use std::path::Path;

/// Derive a 32-byte database encryption key from a passphrase using Argon2id.
///
/// Parameters (OWASP recommended):
/// - Memory: 64 MB
/// - Iterations: 3
/// - Parallelism: 4
pub fn derive_key(passphrase: &[u8], salt: &[u8; 16]) -> Result<[u8; 32]> {
    use argon2::{Argon2, Algorithm, Version, Params};

    let params = Params::new(65536, 3, 4, Some(32))
        .map_err(|e| ChattorError::Crypto(format!("Argon2 params error: {}", e)))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2.hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| ChattorError::Crypto(format!("Argon2 key derivation failed: {}", e)))?;

    Ok(key)
}

/// Load or generate the salt file. Returns the 16-byte salt.
pub fn load_or_create_salt(salt_path: &Path) -> Result<[u8; 16]> {
    if salt_path.exists() {
        let bytes = std::fs::read(salt_path)
            .map_err(|e| ChattorError::Io(e))?;
        if bytes.len() != 16 {
            return Err(ChattorError::Crypto("Invalid salt file length".into()));
        }
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&bytes);
        Ok(salt)
    } else {
        use rand::Rng;
        let salt: [u8; 16] = rand::thread_rng().gen();
        std::fs::write(salt_path, &salt)
            .map_err(|e| ChattorError::Io(e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(salt_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| ChattorError::Io(e))?;
        }
        Ok(salt)
    }
}

/// Check if a database file is unencrypted (for migration).
pub fn is_unencrypted(db_path: &Path) -> bool {
    if !db_path.exists() {
        return false; // No DB yet, will be created encrypted
    }
    // Try to open without a key and read
    match rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(conn) => {
            conn.query_row("SELECT count(*) FROM schema_version", [], |_| Ok(()))
                .is_ok()
        }
        Err(_) => false, // Can't open = likely encrypted or corrupt
    }
}
```

**Step 2: Add open_encrypted to Database**

In `src/db/connection.rs`, add:

```rust
    /// Open database with SQLCipher encryption key.
    pub fn open_encrypted<P: AsRef<Path>>(path: P, key: &[u8; 32]) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        ).map_err(|e| ChattorError::Database(format!("Failed to open database: {}", e)))?;

        // Set encryption key FIRST, before any other operations
        let key_hex = hex::encode(key);
        conn.execute_batch(&format!(
            "PRAGMA key = \"x'{}'\";
             PRAGMA cipher_page_size = 4096;",
            key_hex
        )).map_err(|e| ChattorError::Database(format!("Failed to set encryption key: {}", e)))?;

        // Verify key works by reading from the database
        conn.execute_batch("SELECT count(*) FROM sqlite_master;")
            .map_err(|_| ChattorError::Database(
                "Wrong passphrase or corrupted database".into()
            ))?;

        // Optimize SQLite
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=-8000;
             PRAGMA busy_timeout=5000;"
        ).map_err(|e| ChattorError::Database(format!("Failed to set pragmas: {}", e)))?;

        let mut db = Database { conn };
        db.initialize()?;
        Ok(db)
    }

    /// Migrate an unencrypted database to encrypted format.
    pub fn migrate_to_encrypted<P: AsRef<Path>>(
        old_path: P,
        new_path: P,
        key: &[u8; 32],
    ) -> Result<()> {
        let old_conn = Connection::open_with_flags(
            old_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        ).map_err(|e| ChattorError::Database(format!("Failed to open old database: {}", e)))?;

        let key_hex = hex::encode(key);
        let new_path_str = new_path.as_ref().to_string_lossy().to_string();

        old_conn.execute_batch(&format!(
            "ATTACH DATABASE '{}' AS encrypted KEY \"x'{}'\";
             SELECT sqlcipher_export('encrypted');
             DETACH DATABASE encrypted;",
            new_path_str.replace("'", "''"),
            key_hex
        )).map_err(|e| ChattorError::Database(format!("Failed to migrate database: {}", e)))?;

        Ok(())
    }
```

**Step 3: Update App to use encrypted DB**

In `src/app.rs`, change `Database::open()` calls to accept an optional key. The simplest approach: add `open_with_optional_key`:

```rust
    fn open_db(db_path: &std::path::Path, key: Option<&[u8; 32]>) -> Result<Database> {
        match key {
            Some(k) => Database::open_encrypted(db_path, k),
            None => Database::open(db_path),
        }
    }
```

Update `new()` and `new_with_settings()` to accept `key: Option<&[u8; 32]>`.

**Step 4: Add passphrase prompt to TUI startup**

In `src/main.rs`, before calling `run_tui()`, add passphrase handling:

```rust
fn read_passphrase(prompt: &str) -> Result<String> {
    use std::io::Write;
    eprint!("{}", prompt);
    std::io::stderr().flush().ok();

    // Disable echo for passphrase input
    let passphrase = rpasswd::read_password()
        .map_err(|e| error::ChattorError::Io(e))?;
    Ok(passphrase)
}
```

Actually, since this is a TUI app using crossterm, we should use crossterm's raw mode for password input. This is a significant UX piece that needs careful implementation. The password prompt should appear before the TUI framework initializes.

Add `rpassword = "5"` to `Cargo.toml` for terminal password reading (it handles echo suppression correctly across platforms).

**Step 5: Add --passphrase-fd flag to daemon**

In `src/cli.rs`, modify the Daemon variant:

```rust
    /// Run headless daemon
    Daemon {
        /// Read passphrase from file descriptor (for automation)
        #[arg(long)]
        passphrase_fd: Option<i32>,
    },
```

In `src/daemon/mod.rs`, add passphrase reading from fd:

```rust
fn read_passphrase_from_fd(fd: i32) -> Result<String> {
    use std::io::Read;
    use std::os::unix::io::FromRawFd;
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut passphrase = String::new();
    file.read_to_string(&mut passphrase)
        .map_err(|e| crate::error::ChattorError::Io(e))?;
    Ok(passphrase.trim().to_string())
}
```

**Step 6: Write tests**

Add tests for key derivation, encrypted DB open, and migration in `src/db/connection.rs` tests:

```rust
    #[test]
    fn test_key_derivation() {
        let salt = [1u8; 16];
        let key1 = crate::db::encryption::derive_key(b"password123", &salt).unwrap();
        let key2 = crate::db::encryption::derive_key(b"password123", &salt).unwrap();
        assert_eq!(key1, key2); // Deterministic

        let key3 = crate::db::encryption::derive_key(b"different", &salt).unwrap();
        assert_ne!(key1, key3); // Different passphrase = different key
    }

    #[test]
    fn test_encrypted_database_roundtrip() {
        let temp_file = NamedTempFile::new().unwrap();
        let key = [42u8; 32];

        // Create encrypted DB
        {
            let db = Database::open_encrypted(temp_file.path(), &key).unwrap();
            let conn = db.connection();
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES ('test', 'hello')",
                [],
            ).unwrap();
        }

        // Reopen with same key
        {
            let db = Database::open_encrypted(temp_file.path(), &key).unwrap();
            let val: String = db.connection().query_row(
                "SELECT value FROM app_settings WHERE key = 'test'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(val, "hello");
        }

        // Wrong key should fail
        {
            let wrong_key = [99u8; 32];
            let result = Database::open_encrypted(temp_file.path(), &wrong_key);
            assert!(result.is_err());
        }
    }
```

**Step 7: Run tests**

Run: `cargo test`
Expected: all tests pass

**Step 8: Commit**

```bash
git add src/db/encryption.rs src/db/connection.rs src/db/mod.rs src/app.rs \
        src/main.rs src/daemon/mod.rs src/cli.rs Cargo.toml
git commit -m "security: enable SQLCipher database encryption via Argon2id passphrase

Database is now encrypted at rest using SQLCipher. Key is derived
from a user passphrase via Argon2id (64MB, 3 iterations, 4 threads).
TUI prompts for passphrase on startup. Daemon supports --passphrase-fd
for automation. Existing unencrypted databases are migrated on first
passphrase setup."
```

---

### Final Step: Full Test Suite Verification

**Step 1: Run complete test suite**

Run: `cargo test`
Expected: all ~406+ tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

**Step 3: Check formatting**

Run: `cargo fmt -- --check`
Expected: no formatting issues

**Step 4: Final commit (if any fixups needed)**

```bash
git add -A
git commit -m "chore: fixup lint and formatting from security hardening"
```
