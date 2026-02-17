# P2P Messaging Hardening — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the existing peer-to-peer messaging pipeline robust for real-world Tor conditions by adding connection pooling, exponential backoff retries, and concurrent per-peer queue processing.

**Architecture:** New `ConnectionPool` module in `src/net/pool.rs` caches Tor connections per peer. Queue retry policy changes from flat 30s/10-retry to exponential backoff with 24h expiry. Queue processor spawns concurrent tasks per unique peer instead of sequential processing.

**Tech Stack:** Rust, tokio (JoinSet, Semaphore, Mutex), arti-client DataStream, existing framing/protocol modules

---

## Task 1: Add `created_at` to QueuedMessage and update query

**Files:**
- Modify: `src/net/queue.rs:12-18` (QueuedMessage struct)
- Modify: `src/net/queue.rs:66-121` (get_pending_messages query)
- Test: inline in `src/net/queue.rs`

The `QueuedMessage` struct is missing `created_at`, which we need for the 24h expiry check. The column already exists in the database table — we just need to SELECT and store it.

**Step 1: Add `created_at` field to QueuedMessage**

In `src/net/queue.rs`, add `created_at` to the struct:

```rust
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub id: i64,
    pub peer_onion: String,
    pub message: Message,
    pub retry_count: i64,
    pub priority: String,
    pub created_at: i64,
}
```

**Step 2: Update the SQL query and deserialization in `get_pending_messages`**

Change the SELECT to also fetch `created_at`:

```sql
SELECT id, peer_onion, message_json, retry_count, priority, created_at
FROM message_queue
WHERE status = 'pending' AND next_retry_at <= ?1
ORDER BY CASE priority WHEN 'high' THEN 0 ELSE 1 END, created_at ASC
```

Update the raw_rows type to `Vec<(i64, String, String, i64, String, i64)>` and add `row.get(5)?` to the query_map closure. Pass `created_at` into the `QueuedMessage` constructor.

**Step 3: Verify it compiles and tests pass**

Run: `cargo test net::queue 2>&1 | tail -10`
Expected: All 7 existing queue tests pass (they don't read `created_at` so they're unaffected)

**Step 4: Commit**

```bash
git add src/net/queue.rs
git commit -m "feat: add created_at field to QueuedMessage for expiry tracking"
```

---

## Task 2: Add exponential backoff helper with tests

**Files:**
- Modify: `src/net/queue.rs` (add function + tests)

**Step 1: Write failing tests**

Add these tests to the `#[cfg(test)]` block in `src/net/queue.rs`:

```rust
    #[test]
    fn test_compute_next_retry_backoff_schedule() {
        let now = 1000000;
        let created_at = now - 60; // created 60s ago

        // retry 0 → 30s delay
        assert_eq!(compute_next_retry(0, created_at, now), Some(now + 30));
        // retry 1 → 60s delay
        assert_eq!(compute_next_retry(1, created_at, now), Some(now + 60));
        // retry 2 → 120s delay
        assert_eq!(compute_next_retry(2, created_at, now), Some(now + 120));
        // retry 3 → 240s delay
        assert_eq!(compute_next_retry(3, created_at, now), Some(now + 240));
        // retry 4 → 480s delay
        assert_eq!(compute_next_retry(4, created_at, now), Some(now + 480));
        // retry 5 → 900s (capped at 15 min)
        assert_eq!(compute_next_retry(5, created_at, now), Some(now + 900));
        // retry 10 → still 900s (cap holds)
        assert_eq!(compute_next_retry(10, created_at, now), Some(now + 900));
    }

    #[test]
    fn test_compute_next_retry_expires_after_24h() {
        let now = 1000000;
        let created_at = now - 86401; // created 24h + 1s ago

        // Should return None — message expired
        assert_eq!(compute_next_retry(0, created_at, now), None);
    }

    #[test]
    fn test_compute_next_retry_just_within_window() {
        let now = 1000000;
        let created_at = now - 86399; // created 24h - 1s ago

        // Still within window — should return a retry time
        assert!(compute_next_retry(0, created_at, now).is_some());
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test net::queue::tests::test_compute_next_retry 2>&1 | tail -5`
Expected: FAIL — `compute_next_retry` not found

**Step 3: Implement `compute_next_retry`**

Add this function to `src/net/queue.rs` (above the `#[cfg(test)]` block, after the `impl MessageQueue` block):

```rust
/// 24 hours in seconds
const MESSAGE_EXPIRY_SECS: i64 = 86400;

/// Maximum retry delay in seconds (15 minutes)
const MAX_RETRY_DELAY_SECS: i64 = 900;

/// Base retry delay in seconds
const BASE_RETRY_DELAY_SECS: i64 = 30;

/// Compute the next retry timestamp using exponential backoff.
///
/// Returns `None` if the message has expired (older than 24 hours).
/// Formula: min(30 * 2^retry_count, 900) seconds from now.
pub fn compute_next_retry(retry_count: i64, created_at: i64, now: i64) -> Option<i64> {
    // Check 24h expiry
    if now - created_at >= MESSAGE_EXPIRY_SECS {
        return None;
    }

    let delay = BASE_RETRY_DELAY_SECS
        .checked_shl(retry_count as u32)
        .unwrap_or(MAX_RETRY_DELAY_SECS)
        .min(MAX_RETRY_DELAY_SECS);

    Some(now + delay)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test net::queue 2>&1 | tail -10`
Expected: All 10 tests pass (7 existing + 3 new)

**Step 5: Commit**

```bash
git add src/net/queue.rs
git commit -m "feat: add exponential backoff retry with 24h expiry window"
```

---

## Task 3: Add CHATTOR_PORT constant and clean up connection.rs

**Files:**
- Modify: `src/tor/connection.rs`

**Step 1: Add port constant and clean up dead code**

Replace the entire `src/tor/connection.rs` with:

```rust
use crate::error::{Result, TorrentChatError};
use crate::tor::client::TorClient;
use crate::protocol::message::Message;
use crate::net::framing::send_message;
use arti_client::DataStream;
use tracing::info;

/// Port used for chattor peer-to-peer communication over Tor
pub const CHATTOR_PORT: u16 = 9051;

/// Connection to peer over Tor
pub struct TorConnection {
    stream: DataStream,
}

impl TorConnection {
    /// Connect to peer via Tor (real DataStream)
    pub async fn connect(
        tor_client: &TorClient,
        remote_onion: &str,
    ) -> Result<Self> {
        use arti_client::StreamPrefs;

        let stream = tor_client.inner()
            .connect_with_prefs((remote_onion, CHATTOR_PORT), &StreamPrefs::default())
            .await
            .map_err(|e| TorrentChatError::Tor(format!("Failed to connect to {}: {}", remote_onion, e)))?;

        info!("Connected to {} via Tor", remote_onion);

        Ok(TorConnection { stream })
    }

    /// Send message over connection
    pub async fn send(&mut self, message: &Message) -> Result<()> {
        send_message(&mut self.stream, message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chattor_port_constant() {
        assert_eq!(CHATTOR_PORT, 9051);
    }

    #[tokio::test]
    #[ignore] // Requires Tor daemon
    async fn test_real_tor_connection() {
        let tor_client = crate::tor::client::TorClient::new().await.unwrap();
        let result = TorConnection::connect(&tor_client, "test.onion").await;
        assert!(result.is_err());
    }
}
```

This removes: `receive()` method (dead code), `remote_onion` field (never read), `connect_direct()` test helper (unimplemented), unused `receive_message` import, and the `should_panic` test for the deleted helper.

**Step 2: Verify it compiles and tests pass**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors. Connection warnings should be gone.

Run: `cargo test tor::connection 2>&1 | tail -5`
Expected: 1 test passes (the port constant test), 1 ignored (real Tor test)

**Step 3: Commit**

```bash
git add src/tor/connection.rs
git commit -m "feat: add CHATTOR_PORT constant, remove dead code from TorConnection"
```

---

## Task 4: Create ConnectionPool module

**Files:**
- Create: `src/net/pool.rs`
- Modify: `src/net/mod.rs:1-14`

This is the core new component.

**Step 1: Write the ConnectionPool module**

Create `src/net/pool.rs`:

```rust
use crate::error::{Result, TorrentChatError};
use crate::protocol::message::Message;
use crate::tor::client::TorClient;
use crate::tor::connection::TorConnection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// How long an idle connection is kept before eviction
const IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Timeout for establishing a new Tor circuit
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for sending a message on an established connection
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the cleanup task sweeps for idle connections
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

struct PooledConnection {
    conn: TorConnection,
    last_used: Instant,
}

/// Connection pool that caches Tor circuits per peer.
///
/// Reuses existing connections when possible, creates new ones on demand,
/// and automatically evicts connections idle for more than 5 minutes.
pub struct ConnectionPool {
    connections: Arc<Mutex<HashMap<String, PooledConnection>>>,
    tor_client: Arc<TorClient>,
}

impl ConnectionPool {
    /// Create a new pool and spawn the background cleanup task.
    pub fn new(tor_client: Arc<TorClient>) -> Arc<Self> {
        let pool = Arc::new(ConnectionPool {
            connections: Arc::new(Mutex::new(HashMap::new())),
            tor_client,
        });

        // Spawn background cleanup task
        let cleanup_conns = Arc::clone(&pool.connections);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(CLEANUP_INTERVAL).await;
                let mut conns = cleanup_conns.lock().await;
                let before = conns.len();
                conns.retain(|_, pc| pc.last_used.elapsed() < IDLE_TIMEOUT);
                let evicted = before - conns.len();
                if evicted > 0 {
                    tracing::debug!("Connection pool: evicted {} idle connections", evicted);
                }
            }
        });

        pool
    }

    /// Send a message to a peer, reusing a cached connection if available.
    ///
    /// On send failure with a cached connection, evicts it and retries once
    /// with a fresh circuit. Returns error only if the fresh attempt also fails.
    pub async fn send(&self, peer_onion: &str, message: &Message) -> Result<()> {
        // Try cached connection first
        let mut conns = self.connections.lock().await;
        if let Some(pooled) = conns.get_mut(peer_onion) {
            pooled.last_used = Instant::now();
            let send_result = tokio::time::timeout(
                SEND_TIMEOUT,
                pooled.conn.send(message),
            ).await;

            match send_result {
                Ok(Ok(())) => return Ok(()),
                _ => {
                    // Stale connection — evict and fall through to fresh connect
                    conns.remove(peer_onion);
                    tracing::debug!("Evicted stale connection to {}", peer_onion);
                }
            }
        }
        drop(conns);

        // Create fresh connection
        let mut conn = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TorConnection::connect(&self.tor_client, peer_onion),
        )
        .await
        .map_err(|_| TorrentChatError::ConnectionTimeout(peer_onion.to_string()))??;

        // Send on fresh connection
        tokio::time::timeout(SEND_TIMEOUT, conn.send(message))
            .await
            .map_err(|_| TorrentChatError::Network(
                format!("Send timed out ({}s) to {}", SEND_TIMEOUT.as_secs(), peer_onion),
            ))??;

        // Cache the connection for reuse
        let mut conns = self.connections.lock().await;
        conns.insert(peer_onion.to_string(), PooledConnection {
            conn,
            last_used: Instant::now(),
        });

        Ok(())
    }

    /// Explicitly remove a cached connection for a peer.
    pub async fn evict(&self, peer_onion: &str) {
        let mut conns = self.connections.lock().await;
        conns.remove(peer_onion);
    }
}
```

**Step 2: Register the module in `src/net/mod.rs`**

Add `pub mod pool;` and the re-export. The full file becomes:

```rust
// Network module - connection and delivery management

pub mod queue;
pub mod listener;
pub mod framing;
pub mod sender;
pub mod receiver;
pub mod pool;

pub use queue::MessageQueue;
pub use listener::{listen_for_connections, listen_for_tor_connections, IncomingMessage};
pub use framing::{send_message, receive_message};
pub use sender::MessageSender;
pub use receiver::MessageReceiver;
pub use pool::ConnectionPool;
```

**Step 3: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors

**Step 4: Commit**

```bash
git add src/net/pool.rs src/net/mod.rs
git commit -m "feat: add ConnectionPool with per-peer caching and idle eviction"
```

---

## Task 5: Wire ConnectionPool into App

**Files:**
- Modify: `src/app.rs:1-9` (imports)
- Modify: `src/app.rs:16-26` (App struct)
- Modify: `src/app.rs:55-65` (App::new return)
- Modify: `src/app.rs:68-113` (init_tor)
- Modify: `src/app.rs:116-140` (new_with_settings)

**Step 1: Add the field to App struct**

In `src/app.rs`, add to imports:

```rust
use crate::net::pool::ConnectionPool;
```

Add to the `App` struct (after `message_queue`):

```rust
    pub connection_pool: Option<Arc<ConnectionPool>>,
```

**Step 2: Initialize to `None` in constructors**

In `App::new()`, inside the `Ok(App { ... })` block, add:

```rust
            connection_pool: None,
```

Do the same in `App::new_with_settings()`.

**Step 3: Create pool in `init_tor()`**

In `init_tor()`, after `self.tor_client = Some(Arc::new(client));` on line 108, the client is stored. But we need the Arc *before* storing it. Restructure the end of `init_tor()` to:

```rust
        // Store in app state
        let client = Arc::new(client);
        let pool = ConnectionPool::new(Arc::clone(&client));
        self.tor_client = Some(client);
        self.hidden_service = Some(hidden_service);
        self.onion_address = Some(onion_address);
        self.connection_pool = Some(pool);
```

**Step 4: Update tests**

In the `test_app_has_phase2_components` test, add:

```rust
        assert!(app.connection_pool.is_none());
```

**Step 5: Verify it compiles and tests pass**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors

Run: `cargo test app::tests 2>&1 | tail -10`
Expected: All app tests pass

**Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat: wire ConnectionPool into App, create in init_tor()"
```

---

## Task 6: Simplify try_send_direct to use ConnectionPool

**Files:**
- Modify: `src/main.rs:845-871` (try_send_direct function)

**Step 1: Rewrite `try_send_direct`**

Replace the function with:

```rust
/// Try to send a message directly to peer via the connection pool
async fn try_send_direct(
    app: &App,
    peer_onion: &str,
    message: &protocol::message::Message,
) -> Result<()> {
    let pool = app.connection_pool.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Connection pool not initialized".into()))?;

    pool.send(peer_onion, message).await
}
```

This replaces 25 lines with 5. The pool handles connection creation, caching, timeouts, and retry-on-stale internally.

**Step 2: Verify it compiles and tests pass**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "refactor: simplify try_send_direct to use ConnectionPool"
```

---

## Task 7: Rewrite process_message_queue with concurrency and backoff

**Files:**
- Modify: `src/main.rs:1069-1101` (process_message_queue function)

**Step 1: Rewrite `process_message_queue`**

Replace the function with:

```rust
/// Process pending messages in the queue with per-peer concurrency
async fn process_message_queue(app: &App) -> Result<()> {
    use std::collections::HashMap;
    use tokio::task::JoinSet;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let pending = app.message_queue.get_pending_messages(&app.db, now)?;

    if pending.is_empty() {
        return Ok(());
    }

    // Group messages by peer
    let mut by_peer: HashMap<String, Vec<net::queue::QueuedMessage>> = HashMap::new();
    for msg in pending {
        by_peer.entry(msg.peer_onion.clone()).or_default().push(msg);
    }

    let pool = app.connection_pool.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Connection pool not initialized".into()))?;
    let pool = Arc::clone(pool);

    // Semaphore limits concurrent peer tasks to 10
    let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
    let mut join_set = JoinSet::new();

    // Collect DB operations to run after tasks complete
    // Each task returns a vec of (queue_id, outcome) pairs
    for (peer_onion, messages) in by_peer {
        let pool = Arc::clone(&pool);
        let sem = Arc::clone(&semaphore);

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let mut results: Vec<(i64, i64, bool)> = Vec::new(); // (id, created_at, success)

            for queued in messages {
                let success = pool.send(&peer_onion, &queued.message).await.is_ok();
                results.push((queued.id, queued.created_at, success));

                if !success {
                    // If one message fails, remaining to this peer will likely fail too
                    // Mark remaining as needing retry without attempting
                    break;
                }
            }

            results
        });
    }

    // Collect results and update DB
    while let Some(result) = join_set.join_next().await {
        if let Ok(outcomes) = result {
            for (id, created_at, success) in outcomes {
                if success {
                    app.message_queue.mark_delivered(&app.db, id)?;
                } else {
                    match net::queue::compute_next_retry(
                        // We don't have retry_count here, but schedule_retry increments it.
                        // Just compute from created_at — if expired, mark failed.
                        0, created_at, now,
                    ) {
                        Some(_) => {
                            // Get current retry_count from DB to compute correct backoff
                            let retry_count: i64 = app.db.connection().query_row(
                                "SELECT retry_count FROM message_queue WHERE id = ?1",
                                [id],
                                |row| row.get(0),
                            ).unwrap_or(0);

                            match net::queue::compute_next_retry(retry_count, created_at, now) {
                                Some(next) => {
                                    app.message_queue.schedule_retry(&app.db, id, next)?;
                                }
                                None => {
                                    app.message_queue.mark_failed(&app.db, id)?;
                                    eprintln!("Message #{} expired after 24h", id);
                                }
                            }
                        }
                        None => {
                            app.message_queue.mark_failed(&app.db, id)?;
                            eprintln!("Message #{} expired after 24h", id);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
```

**Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors

**Step 3: Run all tests**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: concurrent per-peer queue processing with exponential backoff"
```

---

## Task 8: Update CLAUDE.md and final cleanup

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update CLAUDE.md**

Update the following sections:

- In **Phase 6 status**, add under "Chunk 2":
  ```
  **Chunk 3 — P2P Messaging Hardening: ✅**
  - ConnectionPool with per-peer caching, idle eviction (5min), retry-on-stale
  - Exponential backoff retries (30s→15min cap, 24h expiry window)
  - Concurrent per-peer queue processing (10 parallel peers via semaphore)
  - CHATTOR_PORT constant, TorConnection dead code cleanup
  ```

- In the **Message Flow** section, update to reflect connection pooling:
  ```
  4. Send via ConnectionPool (reuses cached Tor circuit or creates new one)
  ```

- In **What's Stubbed**, remove any reference to TorConnection send being stubbed (it's fully real now).

- Update test count to current value.

**Step 2: Run full test suite**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass

Run: `cargo clippy 2>&1 | tail -10`
Expected: No new warnings from our changes

**Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "chore: update CLAUDE.md for P2P messaging hardening"
```

---

## Important Notes for the Implementer

### Architectural Decisions

1. **The pool holds the Mutex lock during send.** This is intentional — it prevents two tasks from racing to create duplicate connections to the same peer. The lock is held briefly (just the send, not the circuit build for the retry-on-stale case). For the fresh-connect path, the lock is dropped before connecting and re-acquired to cache.

   Wait — actually, re-read the `send()` implementation carefully. The lock IS dropped before creating a fresh connection (`drop(conns)` after the `if let Some(pooled)` block). This is correct: we don't hold the lock during circuit build.

2. **The semaphore (10) limits concurrent Tor circuit builds**, not concurrent sends. This prevents overwhelming Tor with 100 simultaneous circuit requests if the queue is full.

3. **Break-on-first-failure in per-peer task** is an optimization: if one message to a peer fails, remaining messages to that peer will almost certainly fail too (peer is offline). We skip them and let the next queue processing cycle retry.

### What NOT to Change

- `src/net/framing.rs` — already generic, works perfectly
- `src/net/listener.rs` — incoming side is independent from outbound sending
- `src/crypto/signal.rs` — encryption layer is orthogonal
- `src/protocol/message.rs` — wire format unchanged
- Database schema — no migration needed (created_at column already exists)

### Testing Notes

- The `ConnectionPool` can't be unit-tested with mock streams easily because `TorConnection::connect()` requires a real `TorClient`. Integration tests with real Tor should use `#[ignore]`.
- The `compute_next_retry()` function IS unit-testable (pure math) — that's where most test coverage goes.
- Existing tests remain unchanged since they don't use the pool.
