# Design: Split `src/daemon/rpc.rs` into Per-Domain Handler Modules

**Date:** 2026-06-09
**Status:** Approved
**Type:** Refactor (no behavior change)

## Motivation

`src/daemon/rpc.rs` is 1,006 lines: shared JSON-RPC types, a `dispatch()` router, and
15 `handle_*` async functions covering friends, channels, messaging, and a few
system methods. The graphify analysis flagged it as an oversized module (the third
target in the prioritized refactor backlog, after `db/queries.rs` and `ui/state.rs`).
`dispatch()` itself is already a clean 15-arm `match` — the issue is purely file size:
all 15 handlers live in one file.

Splitting the handlers into per-domain submodules makes each unit focused and keeps
`dispatch()` reading like a routing table that names each method's domain.

## Goal & Non-Goals

**Goal:** Decompose `rpc.rs` into a `rpc/` directory module — shared types and
`dispatch()` in `mod.rs`, the 15 handlers split across four per-domain submodules —
with no behavior change and no call-site churn.

**Non-Goals:**
- No change to the JSON-RPC wire protocol, method names, params, or responses.
- No change to `dispatch()`'s routing logic (only the call target of each arm gains a
  `domain::` prefix).
- No new RPC methods.

## Constraints (verified against source)

- `src/daemon/mod.rs` declares `pub mod rpc;`. Converting `rpc.rs` to `rpc/mod.rs`
  keeps this resolving.
- External callers (`src/daemon/socket.rs`) use only `rpc::RpcRequest`,
  `rpc::RpcResponse`, and `rpc::dispatch` — all of which stay in `mod.rs`. So the
  public API is unchanged and `socket.rs` is untouched.
- The 15 `handle_*` functions are private `async fn`, called only by `dispatch()`.
- `dispatch()` signature: `pub async fn dispatch(req: &RpcRequest, app: &Arc<Mutex<App>>, presence: &PresenceMap) -> RpcResponse`. Handlers take `(id: Option<Value>, app: &Arc<Mutex<App>>)` plus, variously, `presence: &PresenceMap` (only `handle_friends_list`) or `params: &<...>` (the `&req.params` handlers).
- 17 tests in one `#[cfg(test)] mod tests`; every test calls the public `dispatch()`.
- Baseline: `cargo test --lib` green (361 passed, 4 ignored before this change).

## Design

### Module structure

```
src/daemon/rpc/
  mod.rs        // RpcRequest, RpcResponse, RpcError, impl RpcResponse,
                // pub async fn dispatch(), `mod friends/channels/messaging/system;`,
                // and the shared/type/dispatch tests
  friends.rs    // handle_friends_list, handle_friends_add, handle_friends_requests,
                //   handle_friends_accept, handle_friends_reject            (5)
  channels.rs   // handle_channels_list, handle_channels_publish,
                //   handle_channels_subscribe, handle_channels_feed         (4)
  messaging.rs  // handle_send_message, handle_recv_messages,
                //   handle_ephemeral_set                                    (3)
  system.rs     // handle_status, handle_identity, handle_notifications_toggle (3)
```

### `mod.rs` retains the public surface

`RpcRequest`, `RpcResponse`, `RpcError`, `impl RpcResponse`, and `dispatch()` stay in
`mod.rs`. Because those are exactly what `socket.rs` references, the public API
(`rpc::dispatch`, `rpc::RpcRequest`, `rpc::RpcResponse`) is preserved and no caller
outside `src/daemon/rpc/` changes.

### Handlers become `pub(super)` in submodules

Each `handle_*` moves verbatim into its domain file as `pub(super) async fn` —
visible to the parent `rpc` module (where `dispatch()` lives), invisible outside.
Handler bodies (DB calls, JSON shaping, error handling) are unchanged.

### `dispatch()` arms gain a domain prefix

The one deliberate non-move change. Each arm's call target is qualified with its
submodule:

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

Match keys, argument lists, and the `_ =>` fallback are otherwise identical to the
original. (Alternative considered and rejected: `use friends::*;` glob imports to keep
the arms bare — less explicit about domain ownership.)

### Submodule imports

Each file imports only what its handlers use — typically `use super::RpcResponse;`
plus `std::sync::Arc`, `tokio::sync::Mutex`, `crate::app::App`, `serde_json::Value`,
and (friends only) `crate::presence::{PresenceMap, get_presence_snapshot}`. The
compiler flags any unused or missing import under `clippy -D warnings`.

### Tests

The shared-type and router tests stay in `mod.rs`: `test_rpc_response_success`,
`test_rpc_response_error`, `test_parse_rpc_request`, `test_parse_rpc_request_no_params`,
`test_rpc_response_null_id`, `test_rpc_error_codes`, `test_dispatch_unknown_method`.
The per-method `test_dispatch_*` tests distribute to their domain submodule
(friends/channels/messaging/system). Every test calls the public `dispatch()`, so
placement is organizational — a test passes wherever it sits. No test logic changes.

## Error Handling & Data Flow

Unchanged. `dispatch()` keeps the same signature and routing; handler bodies are
byte-identical; the JSON-RPC request/response/error types and wire format are
untouched. `socket.rs` calls `rpc::dispatch` exactly as before.

## Testing & Verification

Done only when **all** pass:

1. `cargo build` — clean.
2. `cargo test` — same counts as baseline, all green (17 rpc tests + full suite). Run
   with `--test-threads=1` to avoid a known pre-existing `$HOME`-race flake in
   `app.rs` tests, unrelated to this change.
3. `cargo clippy --lib -- -D warnings` — no new lints (watch for unused imports per
   submodule).
4. `cargo fmt --check` — clean.
5. Zero call-site churn: `git diff <base> --name-only -- '*.rs' | grep -v '^src/daemon/rpc'`
   is empty.

## Risk Assessment

**Low** — closer to the `db/queries.rs` split than `ui/state.rs`: the handlers are
independent standalone async functions, so each moves cleanly with no control-flow
entanglement. The only non-move change is the 15 `dispatch()` arms gaining a
`domain::` prefix, verified immediately by the build. Rollback is per-commit revert.
