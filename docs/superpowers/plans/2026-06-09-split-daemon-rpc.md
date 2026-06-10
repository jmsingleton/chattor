# Split `src/daemon/rpc.rs` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose the 1,006-line `src/daemon/rpc.rs` into a `rpc/` directory module — shared JSON-RPC types and `dispatch()` in `mod.rs`, the 15 `handle_*` functions split across four per-domain submodules — with no behavior change and no call-site churn.

**Architecture:** Convert `rpc.rs` to `rpc/mod.rs` (keeps `RpcRequest`/`RpcResponse`/`RpcError`, `impl RpcResponse`, and `pub async fn dispatch()`). Each `handle_*` moves verbatim into a per-domain submodule as `pub(super) async fn`; the corresponding `dispatch()` arm gains a `domain::` prefix. Handlers extract one domain at a time with all 17 tests staying in `mod.rs` (they call the public `dispatch()`, so they validate every step), then tests distribute to the submodules last. The public API (`rpc::dispatch`, `rpc::RpcRequest`, `rpc::RpcResponse`) stays in `mod.rs`, so `socket.rs` is untouched.

**Tech Stack:** Rust, tokio (async), serde_json, cargo (build/test/clippy/fmt).

**Spec:** `docs/superpowers/specs/2026-06-09-split-daemon-rpc-design.md`

---

## Reference: handler → domain → line range

(In the pre-split `rpc.rs`. Move each handler body verbatim; change its signature from `async fn` to `pub(super) async fn`.)

| Handler | Domain file | Lines |
|---------|-------------|-------|
| `handle_friends_list` | friends.rs | 117–146 |
| `handle_friends_add` | friends.rs | 147–224 |
| `handle_friends_requests` | friends.rs | 225–241 |
| `handle_friends_accept` | friends.rs | 242–257 |
| `handle_friends_reject` | friends.rs | 258–273 |
| `handle_channels_list` | channels.rs | 476–503 |
| `handle_channels_publish` | channels.rs | 504–586 |
| `handle_channels_subscribe` | channels.rs | 587–669 |
| `handle_channels_feed` | channels.rs | 670–696 |
| `handle_send_message` | messaging.rs | 274–426 |
| `handle_recv_messages` | messaging.rs | 427–475 |
| `handle_ephemeral_set` | messaging.rs | 697–742 |
| `handle_status` | system.rs | 88–99 |
| `handle_identity` | system.rs | 100–116 |
| `handle_notifications_toggle` | system.rs | 743–753 |

`dispatch()` is at lines 58–87; the `#[cfg(test)] mod tests` starts at line 754.

## Reference: submodule import header

Each submodule starts with these imports (the compiler trims/extends under
`clippy -D warnings`; bodies are verbatim and self-contained re: `serde_json::json!`,
which they qualify):

```rust
use super::RpcResponse;
use crate::app::App;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
```

**friends.rs additionally needs** `use crate::presence::PresenceMap;` (the
`handle_friends_list` param type; its body already calls
`crate::presence::get_presence_snapshot(...)` by full path).

## Reference: the `dispatch()` arms after all four extractions

After Tasks 2–5, `dispatch()`'s match reads (only the call targets changed — keys,
args, and the `_ =>` fallback are original):

```rust
match req.method.as_str() {
    "status"               => system::handle_status(id, app).await,
    "identity"             => system::handle_identity(id, app).await,
    "friends_list"         => friends::handle_friends_list(id, app, presence).await,
    "friends_add"          => friends::handle_friends_add(id, app, &req.params).await,
    "friends_requests"     => friends::handle_friends_requests(id, app).await,
    "friends_accept"       => friends::handle_friends_accept(id, app, &req.params).await,
    "friends_reject"       => friends::handle_friends_reject(id, app, &req.params).await,
    "send_message"         => messaging::handle_send_message(id, app, &req.params).await,
    "recv_messages"        => messaging::handle_recv_messages(id, app, &req.params).await,
    "channels_list"        => channels::handle_channels_list(id, app).await,
    "channels_publish"     => channels::handle_channels_publish(id, app, &req.params).await,
    "channels_subscribe"   => channels::handle_channels_subscribe(id, app, &req.params).await,
    "channels_feed"        => channels::handle_channels_feed(id, app, &req.params).await,
    "ephemeral_set"        => messaging::handle_ephemeral_set(id, app, &req.params).await,
    "notifications_toggle" => system::handle_notifications_toggle(id, app).await,
    _ => RpcResponse::error(id, -32601, format!("Method not found: {}", req.method)),
}
```

---

### Task 1: Relocate `rpc.rs` to a directory module (no content change)

**Files:**
- Move: `src/daemon/rpc.rs` → `src/daemon/rpc/mod.rs`

- [ ] **Step 1: Move the file**

`src/daemon/mod.rs`'s `pub mod rpc;` resolves `rpc/mod.rs` identically.

Run:
```bash
mkdir -p src/daemon/rpc
git mv src/daemon/rpc.rs src/daemon/rpc/mod.rs
```

- [ ] **Step 2: Build** — `cargo build`. Expected `Finished`, no errors.

- [ ] **Step 3: Test** — `cargo test --lib -- --test-threads=1`. Expected `361 passed; 0 failed; 4 ignored` (unchanged). `--test-threads=1` avoids a pre-existing `$HOME`-race flake in `app.rs` tests unrelated to this work.

- [ ] **Step 4: Commit**
```bash
git add -A
git commit -m "refactor(daemon): relocate rpc.rs to rpc/ directory module"
```

---

### Task 2: Extract the friends handlers

**Files:**
- Create: `src/daemon/rpc/friends.rs`
- Modify: `src/daemon/rpc/mod.rs`

- [ ] **Step 1: Create `src/daemon/rpc/friends.rs`**

Start with the import header plus `use crate::presence::PresenceMap;`. Then **cut**
(move verbatim) the 5 friends handlers from `mod.rs` into this file, changing each
signature from `async fn handle_friends_*` to `pub(super) async fn handle_friends_*`
(everything else — params, body, return — unchanged):

```rust
use super::RpcResponse;
use crate::app::App;
use crate::presence::PresenceMap;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

// pub(super) async fn handle_friends_list  (orig lines 117-146)
// pub(super) async fn handle_friends_add   (orig lines 147-224)
// pub(super) async fn handle_friends_requests (orig 225-241)
// pub(super) async fn handle_friends_accept   (orig 242-257)
// pub(super) async fn handle_friends_reject   (orig 258-273)
```

- [ ] **Step 2: Wire `mod.rs`**

Add `mod friends;` (keep `mod` declarations alphabetical). Remove the 5 moved
handlers. Update the 5 friends arms in `dispatch()` to qualify the call target:
```rust
"friends_list"     => friends::handle_friends_list(id, app, presence).await,
"friends_add"      => friends::handle_friends_add(id, app, &req.params).await,
"friends_requests" => friends::handle_friends_requests(id, app).await,
"friends_accept"   => friends::handle_friends_accept(id, app, &req.params).await,
"friends_reject"   => friends::handle_friends_reject(id, app, &req.params).await,
```

- [ ] **Step 3: Build** — `cargo build`. Expected `Finished`. If `mod.rs` now warns about an unused import (e.g. `PresenceMap` no longer referenced there because only `handle_friends_list` used it), leave it for now unless clippy in Step 5 errors — later extractions may still need it; reconcile when clippy flags it.

- [ ] **Step 4: Test** — `cargo test --lib -- --test-threads=1`. Expected `361 passed; 0 failed; 4 ignored` (all 17 rpc tests still in mod.rs, all green).

- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected clean. Trim any unused import clippy flags in `friends.rs` or `mod.rs`.

- [ ] **Step 6: Commit**
```bash
git add -A
git commit -m "refactor(daemon): extract friends RPC handlers into rpc/friends.rs"
```

---

### Task 3: Extract the channels handlers

**Files:**
- Create: `src/daemon/rpc/channels.rs`
- Modify: `src/daemon/rpc/mod.rs`

- [ ] **Step 1: Create `src/daemon/rpc/channels.rs`**

Import header (no `PresenceMap`). Cut the 4 channels handlers verbatim, each
`async fn` → `pub(super) async fn`:

```rust
use super::RpcResponse;
use crate::app::App;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

// pub(super) async fn handle_channels_list      (orig 476-503)
// pub(super) async fn handle_channels_publish   (orig 504-586)
// pub(super) async fn handle_channels_subscribe (orig 587-669)
// pub(super) async fn handle_channels_feed      (orig 670-696)
```

- [ ] **Step 2: Wire `mod.rs`** — add `mod channels;` (alphabetical); remove the 4 moved handlers; update the 4 channels arms:
```rust
"channels_list"      => channels::handle_channels_list(id, app).await,
"channels_publish"   => channels::handle_channels_publish(id, app, &req.params).await,
"channels_subscribe" => channels::handle_channels_subscribe(id, app, &req.params).await,
"channels_feed"      => channels::handle_channels_feed(id, app, &req.params).await,
```

- [ ] **Step 3: Build** — `cargo build`. Expected `Finished`.
- [ ] **Step 4: Test** — `cargo test --lib -- --test-threads=1`. Expected `361 passed; 4 ignored`.
- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected clean (trim unused imports).
- [ ] **Step 6: Commit**
```bash
git add -A
git commit -m "refactor(daemon): extract channels RPC handlers into rpc/channels.rs"
```

---

### Task 4: Extract the messaging handlers

**Files:**
- Create: `src/daemon/rpc/messaging.rs`
- Modify: `src/daemon/rpc/mod.rs`

- [ ] **Step 1: Create `src/daemon/rpc/messaging.rs`**

Import header (no `PresenceMap`). Cut the 3 messaging handlers verbatim, each
`async fn` → `pub(super) async fn`:

```rust
use super::RpcResponse;
use crate::app::App;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

// pub(super) async fn handle_send_message  (orig 274-426)
// pub(super) async fn handle_recv_messages (orig 427-475)
// pub(super) async fn handle_ephemeral_set (orig 697-742)
```

- [ ] **Step 2: Wire `mod.rs`** — add `mod messaging;` (alphabetical); remove the 3 moved handlers; update the 3 arms:
```rust
"send_message"  => messaging::handle_send_message(id, app, &req.params).await,
"recv_messages" => messaging::handle_recv_messages(id, app, &req.params).await,
"ephemeral_set" => messaging::handle_ephemeral_set(id, app, &req.params).await,
```

- [ ] **Step 3: Build** — `cargo build`. Expected `Finished`.
- [ ] **Step 4: Test** — `cargo test --lib -- --test-threads=1`. Expected `361 passed; 4 ignored`.
- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected clean (trim unused imports).
- [ ] **Step 6: Commit**
```bash
git add -A
git commit -m "refactor(daemon): extract messaging RPC handlers into rpc/messaging.rs"
```

---

### Task 5: Extract the system handlers

After this task, every `dispatch()` arm is `domain::handle_*` and `mod.rs` holds no
`handle_*` functions.

**Files:**
- Create: `src/daemon/rpc/system.rs`
- Modify: `src/daemon/rpc/mod.rs`

- [ ] **Step 1: Create `src/daemon/rpc/system.rs`**

Import header (no `PresenceMap`). Cut the 3 system handlers verbatim, each
`async fn` → `pub(super) async fn`:

```rust
use super::RpcResponse;
use crate::app::App;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

// pub(super) async fn handle_status              (orig 88-99)
// pub(super) async fn handle_identity            (orig 100-116)
// pub(super) async fn handle_notifications_toggle (orig 743-753)
```

- [ ] **Step 2: Wire `mod.rs`** — add `mod system;` (alphabetical); remove the 3 moved handlers; update the 3 arms:
```rust
"status"               => system::handle_status(id, app).await,
"identity"             => system::handle_identity(id, app).await,
"notifications_toggle" => system::handle_notifications_toggle(id, app).await,
```

- [ ] **Step 3: Build** — `cargo build`. Expected `Finished`.

- [ ] **Step 4: Verify mod.rs has no handlers left and dispatch is fully qualified**

Run: `rg -c '^\s*(pub\(super\) )?async fn handle_' src/daemon/rpc/mod.rs`
Expected: `0` (no handler defined in mod.rs).
Run: `rg -c '=> (friends|channels|messaging|system)::handle_' src/daemon/rpc/mod.rs`
Expected: `15` (every dispatch arm qualified).

- [ ] **Step 5: Test** — `cargo test --lib -- --test-threads=1`. Expected `361 passed; 4 ignored`.
- [ ] **Step 6: Clippy** — `cargo clippy --lib -- -D warnings`. Expected clean. In particular, confirm `mod.rs` no longer imports anything only the handlers used (e.g. if `PresenceMap`/`App`/`Arc`/`Value` are now unused in mod.rs because only `dispatch`'s signature uses them — `dispatch` still uses `RpcRequest`, `Arc<Mutex<App>>`, `PresenceMap`, `RpcResponse`, so those stay; trim only what clippy flags).
- [ ] **Step 7: Commit**
```bash
git add -A
git commit -m "refactor(daemon): extract system RPC handlers into rpc/system.rs"
```

---

### Task 6: Distribute the tests and finalize

All 17 tests are still in `mod.rs`. The shared-type/router tests stay; the
per-method `test_dispatch_*` tests move to their domain submodule. **Every test calls
the public `dispatch()`, so placement is organizational** — a test passes wherever it
sits.

**Test → module assignment:**

- **KEEP in `mod.rs`** (7): `test_rpc_response_success`, `test_rpc_response_error`,
  `test_parse_rpc_request`, `test_parse_rpc_request_no_params`, `test_rpc_response_null_id`,
  `test_rpc_error_codes`, `test_dispatch_unknown_method`
- **`friends.rs`** (2): `test_dispatch_friends_list_empty`, `test_dispatch_friends_requests_empty`
- **`channels.rs`** (2): `test_dispatch_channels_list`, `test_dispatch_channels_feed_empty`
- **`messaging.rs`** (3): `test_dispatch_recv_messages_empty`, `test_dispatch_send_message_missing_params`, `test_dispatch_ephemeral_set_missing_params`
- **`system.rs`** (3): `test_dispatch_status`, `test_dispatch_identity`, `test_dispatch_notifications_toggle`

Total: 7 + 2 + 2 + 3 + 3 = 17.

The shared `test_app()` helper is needed by every async dispatch test, so it is
duplicated into each submodule's test module (and stays in `mod.rs` for
`test_dispatch_unknown_method`) — deliberate ~15-line duplication matching the
codebase convention (e.g. `db/queries/*` `setup_test_db`).

- [ ] **Step 1: Add a test module to each submodule**

In each of `friends.rs`, `channels.rs`, `messaging.rs`, `system.rs`, append a test
module: the `test_app()` helper (verbatim, below) plus the assigned tests moved
verbatim from `mod.rs`. The moved tests call `dispatch(...)` and construct
`RpcRequest { ... }`, which live in the parent module, so import them explicitly.
(`RpcRequest`/`RpcResponse`/`RpcError` all have `pub` fields, so cross-module
construction and `resp.error`/`resp.result` assertions work without any visibility
change.)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::rpc::{dispatch, RpcRequest};

    fn test_app() -> (Arc<Mutex<App>>, tempfile::TempDir, tempfile::TempDir) {
        let temp_config = tempfile::tempdir().unwrap();
        let temp_data = tempfile::tempdir().unwrap();
        let settings = crate::config::Settings {
            config_dir: temp_config.path().to_path_buf(),
            data_dir: temp_data.path().to_path_buf(),
            db_path: temp_data.path().join("test.db"),
            debug: false,
            tor_socks_port: 9050,
        };
        let app = App::new_with_settings(settings, None).unwrap();
        (Arc::new(Mutex::new(app)), temp_config, temp_data)
    }

    // ...assigned tests, moved verbatim...
}
```

If the compiler reports another unresolved name in a moved test (e.g. `RpcResponse`
is available via `super::*`; `new_presence_map` is called as
`crate::presence::new_presence_map()` by full path), add the specific `use` it asks
for — but change NO test logic.

- [ ] **Step 2: Reduce the `mod.rs` test module**

Remove the 10 moved tests from `mod.rs`'s `#[cfg(test)] mod tests`, leaving the 7
KEEP tests (and the `test_app()` helper, still used by `test_dispatch_unknown_method`).

- [ ] **Step 3: Build** — `cargo build`. Expected `Finished`.

- [ ] **Step 4: Verify no test was lost**

Run: `rg -c '#\[test\]|#\[tokio::test\]' src/daemon/rpc/*.rs | awk -F: '{s+=$2} END{print s}'`
Expected: `17`.

- [ ] **Step 5: Full verification gate**

Run: `cargo test -- --test-threads=1`
Expected: lib `361 passed; 4 ignored`, plus daemon `32`, e2e `6`, integration `16` — all green.
Run: `cargo clippy --lib -- -D warnings`
Expected: clean.
Run: `cargo fmt` then `cargo fmt --check`
Expected: clean (no diff).

- [ ] **Step 6: Confirm zero call-site churn**

Run:
```bash
git diff main --name-only -- '*.rs' | grep -v '^src/daemon/rpc' || echo "no non-rpc source files changed"
```
Expected: prints `no non-rpc source files changed` (the public API `rpc::{dispatch, RpcRequest, RpcResponse}` was preserved, so `socket.rs` and everything else is untouched).

- [ ] **Step 7: Commit**
```bash
git add -A
git commit -m "refactor(daemon): distribute RPC tests into per-domain submodules"
```

---

## Final state

```
src/daemon/rpc/
  mod.rs        // RpcRequest, RpcResponse, RpcError, impl RpcResponse, dispatch(), 7 tests
  friends.rs    // 5 handlers + 2 tests
  channels.rs   // 4 handlers + 2 tests
  messaging.rs  // 3 handlers + 3 tests
  system.rs     // 3 handlers + 3 tests
```

Public API (`rpc::dispatch`, `rpc::RpcRequest`, `rpc::RpcResponse`) unchanged;
`socket.rs` untouched. `dispatch()` reads as a routing table with each method's domain
named. Test count unchanged (17). No wire-protocol or behavior changes.
