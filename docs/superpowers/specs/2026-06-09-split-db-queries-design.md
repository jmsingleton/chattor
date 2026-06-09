# Design: Split `src/db/queries.rs` into Domain Modules

**Date:** 2026-06-09
**Status:** Approved
**Type:** Refactor (no behavior change)

## Motivation

A knowledge-graph analysis of the codebase (graphify) surfaced `src/db/queries.rs`
as the single largest structural cluster: 81 graph nodes, **1,397 lines**, and at
least four distinct domains crammed into one file (channels dominate, followed by
messaging, friends, and settings). It is the highest-value, lowest-risk cleanup
target the graph identified — a god module whose internal cohesion is high only
because everything database-related was dumped together.

Splitting it along the domain boundaries the graph already detected makes each unit
focused, independently testable, and small enough to reason about in one pass.

## Goal & Non-Goals

**Goal:** Decompose `queries.rs` into four domain submodules with zero behavior
change and zero call-site churn.

**Non-Goals:**
- No SQL changes, no signature changes, no new functionality.
- No `Database` trait seam, no repository abstraction (separate future work).
- No touching the other refactor candidates (dispatch(), ui/state.rs, etc.).

## Constraints (verified against source)

- **121 call sites** reference functions/types via `db::queries::*`. Caller imports
  are a mix of fully-qualified `db::queries::foo()` and
  `use crate::db::queries::{ChatMessage, FriendEntry, ...}`.
- **No shared private helpers** in production code — every non-test `fn` is already
  `pub fn`. Nothing needs to be threaded between the new modules.
- **29 tests** live in one `#[cfg(test)] mod tests` block sharing a `setup_test_db()`
  helper.
- `src/db/mod.rs` declares `pub mod queries;`.
- Baseline `cargo build` is green before work starts.

## Design

### Module structure

`src/db/queries.rs` becomes a directory module:

```
src/db/queries/
  mod.rs        // mod friends; mod messaging; mod channels; mod settings;
                // pub use friends::*;
                // pub use messaging::*;
                // pub use channels::*;
                // pub use settings::*;
  friends.rs
  messaging.rs
  channels.rs
  settings.rs
```

- Submodules are **private** (`mod friends;`, not `pub mod`), re-exported flat via
  `pub use friends::*`. This preserves the exact public API: `db::queries::foo()`
  resolves as before, and **no** second `db::queries::friends::foo()` path exists to
  create ambiguity or invite drift.
- `src/db/mod.rs` is unchanged — `pub mod queries;` resolves `queries/mod.rs`
  transparently.
- **Net effect on the 121 call sites: none.** They keep compiling verbatim.

### Item assignment

**friends.rs** — types `FriendEntry`, `PendingFriendRequest`; functions:
`get_friends_with_unread`, `find_friend_by_onion`, `get_pending_request_count`,
`get_pending_friend_requests`, `get_friend_display_name`, `store_friend_pubkey`.

**messaging.rs** — type `ChatMessage`; functions:
`get_or_create_conversation`, `get_messages`, `store_outgoing_message`,
`store_outgoing_message_with_ttl`, `store_incoming_message`,
`store_incoming_message_with_ttl`, `mark_conversation_read`, `update_message_status`,
`get_unreceipted_message_ids`, `get_conversation_ephemeral_ttl`,
`set_conversation_ephemeral_ttl`, `activate_ephemeral_timers`,
`cleanup_expired_messages`.

(Ephemeral-TTL functions live here rather than a separate module — they are message
lifecycle, tightly coupled to conversations/messages, and too few to justify their
own unit. YAGNI.)

**channels.rs** — types `ChannelPost`, `ChannelSubscription`; functions:
`initialize_channels`, `store_channel_post`, `get_channel_posts`,
`enforce_channel_retention`, `add_channel_subscriber`, `remove_channel_subscriber`,
`get_channel_subscribers`, `add_channel_subscription`, `remove_channel_subscription`,
`get_channel_subscriptions`, `update_subscription_sync_time`,
`store_channel_post_receipt`, `get_channel_post_read_count`,
`get_channel_post_read_counts_batch`, `get_channel_posts_since`.

**settings.rs** — functions: `get_app_setting`, `set_app_setting`,
`cleanup_stale_prekey_material` (operates on the `app_settings` table where prekey
private material is stored).

### Imports

Each domain file carries only the `use` lines it needs (e.g. `rusqlite::params`,
`crate::error::{ChattorError, Result}`, `crate::db::Database`, and `HashMap` for the
batch read-count function). The compiler flags any missing or unused import; resolve
per file.

### Tests

The 29 tests split to sit beside the code they exercise, each domain file gaining its
own `#[cfg(test)] mod tests`. Each test module declares a local `setup_test_db()`
helper. The ~5-line duplication is accepted deliberately: it keeps each module
independently testable and matches the existing convention elsewhere in the codebase
(e.g. `net/queue.rs`, `crypto/session_store.rs` each define their own setup helpers).

## Error Handling & Data Flow

Unchanged. This is a pure code-move: identical functions, identical SQL, identical
signatures, identical error propagation via `crate::error::Result`.

## Testing & Verification

The refactor is "done" only when **all** of the following pass:

1. `cargo build` — compiles clean.
2. `cargo test` — same test count as baseline, all green (the 29 moved tests plus the
   full suite, ~406 tests).
3. `cargo clippy -- -D warnings` — no new lints (watch for unused imports).
4. `cargo fmt --check` — formatting clean.

## Risk Assessment

**Low.** The only mechanical risk is per-file `use` lines and test-helper placement,
both compiler-caught. No runtime behavior, persistence, or wire-format surface is
touched. Rollback is trivial (revert the commit).
