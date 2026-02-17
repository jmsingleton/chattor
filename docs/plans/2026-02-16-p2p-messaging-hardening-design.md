# P2P Messaging Hardening — Design

**Goal:** Make the existing peer-to-peer messaging pipeline robust for real-world Tor conditions. The outbound send flow, inbound listener, message queue, framing, and Signal encryption all exist and use real code — this design hardens timeouts, retry policy, connection reuse, and queue processing concurrency.

**Approach:** Connection pool module (Approach A from brainstorming).

---

## 1. Connection Pool

New `ConnectionPool` struct in `src/net/pool.rs`.

### Data Model

```rust
pub struct ConnectionPool {
    connections: Arc<Mutex<HashMap<String, PooledConnection>>>,
    tor_client: Arc<TorClient>,
}

struct PooledConnection {
    conn: TorConnection,
    last_used: Instant,
}
```

### API

- `pool.send(peer_onion, message)` — reuses cached connection or creates a new one. On send failure, evicts the stale connection and retries once with a fresh circuit.
- `pool.evict(peer_onion)` — explicitly drop a connection.
- Background cleanup task: connections unused for 5 minutes are dropped via a periodic sweep spawned at pool creation time.

### Ownership

Created in `init_tor()`, stored as `Arc<ConnectionPool>` on the `App` struct. Both `try_send_direct()` and the queue processor use it.

### Port

Define `const CHATTOR_PORT: u16 = 9051` in `src/tor/connection.rs` and use it in `connect()` instead of the current hardcoded literal.

### Timeouts

- Circuit build (connect): **30 seconds** (Tor circuits need this)
- Message send: **10 seconds** (once connected, should be fast)

---

## 2. Retry Policy

Replace the current flat "10 retries x 30s" with exponential backoff over a 24-hour window.

### Backoff Schedule

| Retry # | Delay | Cumulative |
|---------|-------|------------|
| 1 | 30s | 30s |
| 2 | 1min | 1.5min |
| 3 | 2min | 3.5min |
| 4 | 4min | 7.5min |
| 5 | 8min | 15.5min |
| 6+ | 15min (cap) | ... |

**Formula:** `min(30 * 2^retry_count, 900)` seconds — doubles each time, capped at 15 minutes.

**Expiry:** Messages older than 24 hours (based on `created_at`) are marked `failed` regardless of retry count.

### Implementation

New helper function `compute_next_retry(retry_count, created_at, now) -> Option<i64>` in `src/net/queue.rs`. Returns `None` if the message has expired (created_at + 24h < now), otherwise returns the next retry timestamp.

Queue wakeup interval stays at 30 seconds — the backoff lives in `next_retry_at`, so the timer just checks "are any messages due?" each cycle.

---

## 3. Concurrent Queue Processing

Replace the current sequential loop with per-peer parallelism.

### New Flow

1. Get all pending messages
2. Group by `peer_onion` into `HashMap<String, Vec<QueuedMessage>>`
3. For each peer, spawn a tokio task:
   - Messages to same peer sent sequentially (preserves ordering)
   - Different peers processed concurrently
4. Collect results via `JoinSet`

### Concurrency Limit

Cap at 10 concurrent peer tasks via `tokio::sync::Semaphore`. Prevents spawning many Tor circuits simultaneously if the queue is large.

### Ordering Guarantee

Messages to the *same* peer: FIFO order (sequential within per-peer task). Messages to *different* peers: no ordering guarantee (correct — independent conversations).

### try_send_direct

Simplified to call `pool.send()` instead of creating its own `TorConnection`.

---

## 4. File Changes

### New file
- `src/net/pool.rs` — ConnectionPool, PooledConnection, background cleanup, send-with-retry-on-stale

### Modified files
- `src/net/mod.rs` — add `pub mod pool;` and re-export
- `src/tor/connection.rs` — add `CHATTOR_PORT` constant, use in `connect()`, remove dead `receive()` method, clean unused imports
- `src/app.rs` — add `connection_pool: Option<Arc<ConnectionPool>>` field, create pool in `init_tor()`
- `src/main.rs` — simplify `try_send_direct()` to use pool, rewrite `process_message_queue()` with per-peer concurrency and JoinSet, use `compute_next_retry()` for backoff
- `src/net/queue.rs` — add `compute_next_retry()` helper

### Not changed
- `src/net/framing.rs` — already generic
- `src/net/listener.rs` — incoming side is independent
- `src/crypto/signal.rs` — encryption layer is orthogonal
- `src/protocol/message.rs` — wire format unchanged
- Database schema — no migration needed

### Testing
- `pool.rs`: unit tests with `tokio::io::duplex` mock streams
- `queue.rs`: unit test for `compute_next_retry()` backoff curve and 24h expiry
- Existing integration tests continue to work unchanged
- Real Tor tests: `#[ignore]`
