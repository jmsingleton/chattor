# Split `src/db/queries.rs` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose the 1,397-line `src/db/queries.rs` into four focused domain submodules (friends, messaging, channels, settings) with zero behavior change and zero call-site churn.

**Architecture:** Convert `queries.rs` into a `queries/` directory module. A private submodule per domain, flat-re-exported from `queries/mod.rs` via `pub use <domain>::*`, so the existing `db::queries::foo()` public API — and all 121 call sites — stay byte-for-byte unchanged. Work proceeds incrementally: relocate the file first, then extract one domain at a time, keeping `cargo build` + `cargo test` green at every commit so any regression is trivially bisectable.

**Tech Stack:** Rust, rusqlite/SQLCipher, cargo (build/test/clippy/fmt).

**Spec:** `docs/superpowers/specs/2026-06-09-split-db-queries-design.md`

---

## Reference: exact item → domain assignment

These are the public items in `queries.rs` and the domain each moves to. "Move verbatim" means cut the item's full source (signature + body, including its doc comment) unchanged.

**friends.rs** — structs `FriendEntry` (incl. its `impl FriendEntry { fn display }`), `PendingFriendRequest`; fns `get_friends_with_unread`, `find_friend_by_onion`, `get_pending_request_count`, `get_pending_friend_requests`, `get_friend_display_name`, `store_friend_pubkey`.

**messaging.rs** — struct `ChatMessage`; fns `get_or_create_conversation`, `get_messages`, `store_outgoing_message`, `store_outgoing_message_with_ttl`, `store_incoming_message`, `store_incoming_message_with_ttl`, `mark_conversation_read`, `update_message_status`, `get_unreceipted_message_ids`, `get_conversation_ephemeral_ttl`, `set_conversation_ephemeral_ttl`, `activate_ephemeral_timers`, `cleanup_expired_messages`.

**channels.rs** — structs `ChannelPost`, `ChannelSubscription`; fns `initialize_channels`, `store_channel_post`, `get_channel_posts`, `enforce_channel_retention`, `add_channel_subscriber`, `remove_channel_subscriber`, `get_channel_subscribers`, `add_channel_subscription`, `remove_channel_subscription`, `get_channel_subscriptions`, `update_subscription_sync_time`, `store_channel_post_receipt`, `get_channel_post_read_count`, `get_channel_post_read_counts_batch`, `get_channel_posts_since`.

**settings.rs** — fns `get_app_setting`, `set_app_setting`, `cleanup_stale_prekey_material`.

## Reference: test → domain assignment

The single `#[cfg(test)] mod tests` block (29 tests) splits as follows:

- **friends.rs tests:** `test_get_friends_with_unread_empty`, `test_get_friends_with_unread`, `test_find_friend_by_onion`, `test_get_pending_request_count_empty`, `test_get_pending_request_count`, `test_get_pending_friend_requests`, `test_get_pending_friend_requests_excludes_accepted`, `test_friend_entry_display` (8)
- **messaging.rs tests:** `test_get_or_create_conversation`, `test_store_and_get_messages`, `test_mark_conversation_read`, `test_update_message_status`, `test_get_unreceipted_message_ids`, `test_ephemeral_ttl_set_and_get`, `test_cleanup_expired_messages`, `test_activate_ephemeral_timers` (8)
- **channels.rs tests:** `test_initialize_channels`, `test_store_and_get_channel_posts`, `test_channel_post_dedup`, `test_channel_retention_enforced`, `test_add_and_get_channel_subscribers`, `test_remove_channel_subscriber`, `test_add_and_get_channel_subscriptions`, `test_update_subscription_sync_time`, `test_store_and_count_post_receipts`, `test_get_channel_posts_since`, `test_batch_channel_post_read_counts`, `test_batch_channel_post_read_counts_empty` (12)
- **settings.rs tests:** `test_get_set_app_setting` (1)

## Reference: shared test helper

Every new domain test module uses this identical helper (copied verbatim into each — deliberate ~15-line duplication for module independence, matching the codebase convention in `net/queue.rs` and `crypto/session_store.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        // Add a test friend
        db.connection()
            .execute(
                "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
                [],
            )
            .unwrap();

        (db, temp)
    }

    // ... domain tests go here ...
}
```

## Reference: per-file import header

Each new domain file starts with these imports. The Rust compiler will warn on any unused import and error on any missing one — trim/add per file as the build directs. `channels.rs` additionally needs `HashMap` for `get_channel_post_read_counts_batch`.

```rust
use crate::db::Database;
use crate::error::{ChattorError, Result};
use rusqlite::params;
// channels.rs only:
use std::collections::HashMap;
```

---

### Task 1: Relocate `queries.rs` to a directory module (no content change)

This is a pure file move. `src/db/mod.rs`'s `pub mod queries;` resolves `queries/mod.rs` identically to `queries.rs`, so nothing else changes.

**Files:**
- Move: `src/db/queries.rs` → `src/db/queries/mod.rs`

- [ ] **Step 1: Move the file**

Run:
```bash
git mv src/db/queries.rs src/db/queries/mod.rs
```

- [ ] **Step 2: Verify the build is unchanged**

Run: `cargo build`
Expected: `Finished` with no errors (identical to baseline — only the file location changed).

- [ ] **Step 3: Verify the full test suite passes**

Run: `cargo test`
Expected: all tests pass, same count as baseline (~406, including the 29 in `db::queries`).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(db): relocate queries.rs to queries/ directory module"
```

---

### Task 2: Extract the channels domain

Largest domain (15 fns, 2 structs, 12 tests). Doing it first shrinks `mod.rs` the most and exercises the extraction pattern on the hardest case.

**Files:**
- Create: `src/db/queries/channels.rs`
- Modify: `src/db/queries/mod.rs`

- [ ] **Step 1: Create `src/db/queries/channels.rs`**

Create the file with: the import header (including `use std::collections::HashMap;`), then **cut** the structs `ChannelPost` and `ChannelSubscription` and all 15 channel functions (listed in "item → domain assignment" above) **verbatim** from `mod.rs` into this file, then a `#[cfg(test)] mod tests` block using the shared test helper containing the 12 channel tests (listed in "test → domain assignment") moved verbatim.

File skeleton:
```rust
use crate::db::Database;
use crate::error::{ChattorError, Result};
use rusqlite::params;
use std::collections::HashMap;

// <-- ChannelPost, ChannelSubscription, and the 15 channel fns moved here verbatim

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        db.connection()
            .execute(
                "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
                [],
            )
            .unwrap();
        (db, temp)
    }

    // <-- the 12 channel tests moved here verbatim
}
```

- [ ] **Step 2: Wire the submodule in `mod.rs`**

In `src/db/queries/mod.rs`, delete the now-moved channel structs, functions, and channel tests. Add these two lines (keep declarations alphabetical):

```rust
mod channels;
pub use channels::*;
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors. If "unused import" warnings appear in `mod.rs`, leave them for now (they clear as more domains move out) unless clippy in Step 5 errors.

- [ ] **Step 4: Run the channel tests**

Run: `cargo test --lib db::queries::channels`
Expected: 12 tests pass.

- [ ] **Step 5: Full verification**

Run: `cargo test`
Expected: all pass, same total count as Task 1.
Run: `cargo clippy -- -D warnings`
Expected: no errors. If clippy flags an unused import in `channels.rs` (e.g. `ChattorError` not used there), remove that specific line and re-run.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(db): extract channels queries into queries/channels.rs"
```

---

### Task 3: Extract the messaging domain

**Files:**
- Create: `src/db/queries/messaging.rs`
- Modify: `src/db/queries/mod.rs`

- [ ] **Step 1: Create `src/db/queries/messaging.rs`**

Create the file with the import header (no `HashMap`), then cut the struct `ChatMessage` and the 13 messaging functions (listed in "item → domain assignment") **verbatim** from `mod.rs`, then a `#[cfg(test)] mod tests` block using the shared test helper containing the 8 messaging tests moved verbatim.

```rust
use crate::db::Database;
use crate::error::{ChattorError, Result};
use rusqlite::params;

// <-- ChatMessage and the 13 messaging fns moved here verbatim

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        db.connection()
            .execute(
                "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
                [],
            )
            .unwrap();
        (db, temp)
    }

    // <-- the 8 messaging tests moved here verbatim
}
```

- [ ] **Step 2: Wire the submodule in `mod.rs`**

Delete the moved `ChatMessage` struct, messaging functions, and messaging tests from `mod.rs`. Add (keeping alphabetical order):

```rust
mod messaging;
pub use messaging::*;
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors.

- [ ] **Step 4: Run the messaging tests**

Run: `cargo test --lib db::queries::messaging`
Expected: 8 tests pass.

- [ ] **Step 5: Full verification**

Run: `cargo test`
Expected: all pass, same total count.
Run: `cargo clippy -- -D warnings`
Expected: no errors (trim any unused import clippy flags in `messaging.rs`).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(db): extract messaging queries into queries/messaging.rs"
```

---

### Task 4: Extract the friends domain

**Files:**
- Create: `src/db/queries/friends.rs`
- Modify: `src/db/queries/mod.rs`

- [ ] **Step 1: Create `src/db/queries/friends.rs`**

Create the file with the import header, then cut the structs `FriendEntry` (with its `impl FriendEntry`), `PendingFriendRequest`, and the 6 friends functions (listed in "item → domain assignment") **verbatim** from `mod.rs`, then a `#[cfg(test)] mod tests` block using the shared test helper containing the 8 friends tests moved verbatim.

Note: `FriendEntry::display()` calls `crate::ui::input::truncate_display_dots(...)` via its fully-qualified path — that path is unchanged by the move, so nothing extra is needed.

```rust
use crate::db::Database;
use crate::error::{ChattorError, Result};
use rusqlite::params;

// <-- FriendEntry (+ impl), PendingFriendRequest, and the 6 friends fns moved here verbatim

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        db.connection()
            .execute(
                "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
                [],
            )
            .unwrap();
        (db, temp)
    }

    // <-- the 8 friends tests moved here verbatim
}
```

- [ ] **Step 2: Wire the submodule in `mod.rs`**

Delete the moved friends structs, functions, and tests from `mod.rs`. Add (alphabetical — `friends` goes before `messaging`):

```rust
mod friends;
pub use friends::*;
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors.

- [ ] **Step 4: Run the friends tests**

Run: `cargo test --lib db::queries::friends`
Expected: 8 tests pass.

- [ ] **Step 5: Full verification**

Run: `cargo test`
Expected: all pass, same total count.
Run: `cargo clippy -- -D warnings`
Expected: no errors (trim any unused import clippy flags in `friends.rs`).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(db): extract friends queries into queries/friends.rs"
```

---

### Task 5: Extract the settings domain and reduce `mod.rs` to wiring

After this task, `mod.rs` contains only module declarations and re-exports — no logic, no tests.

**Files:**
- Create: `src/db/queries/settings.rs`
- Modify: `src/db/queries/mod.rs`

- [ ] **Step 1: Create `src/db/queries/settings.rs`**

Create the file with the import header, then cut the 3 settings functions (`get_app_setting`, `set_app_setting`, `cleanup_stale_prekey_material`) **verbatim** from `mod.rs`, then a `#[cfg(test)] mod tests` block using the shared test helper containing `test_get_set_app_setting` moved verbatim.

```rust
use crate::db::Database;
use crate::error::{ChattorError, Result};
use rusqlite::params;

// <-- get_app_setting, set_app_setting, cleanup_stale_prekey_material moved here verbatim

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        db.connection()
            .execute(
                "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
                [],
            )
            .unwrap();
        (db, temp)
    }

    // <-- test_get_set_app_setting moved here verbatim
}
```

- [ ] **Step 2: Reduce `mod.rs` to wiring only**

After removing the settings functions and test, `mod.rs` should now contain nothing but the four module declarations and re-exports. Replace the entire remaining contents of `src/db/queries/mod.rs` with exactly:

```rust
//! Database query functions, organized by domain.
//!
//! Submodules are flat-re-exported so callers use `db::queries::<fn>` regardless
//! of which domain a function lives in.

mod channels;
mod friends;
mod messaging;
mod settings;

pub use channels::*;
pub use friends::*;
pub use messaging::*;
pub use settings::*;
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors. (If the build complains a re-exported name is now ambiguous or unused, that indicates an item was placed in two files or left behind — reconcile against the "item → domain assignment" reference.)

- [ ] **Step 4: Run the settings tests**

Run: `cargo test --lib db::queries::settings`
Expected: 1 test passes (`test_get_set_app_setting`).

- [ ] **Step 5: Full verification gate**

Run: `cargo test`
Expected: all tests pass, **same total count as the original baseline** (no test lost in the move).
Run: `cargo clippy -- -D warnings`
Expected: no errors.
Run: `cargo fmt`
Then: `cargo fmt --check`
Expected: clean (no diff).

- [ ] **Step 6: Confirm zero call-site churn**

Run:
```bash
git diff main --stat -- 'src/*.rs' ':!src/db/queries' | grep -v '^ src/db' || echo "no non-queries source files changed"
```
Expected: prints `no non-queries source files changed` (the only `.rs` changes outside `src/db/queries/` should be none — the flat re-export preserved every `db::queries::*` path).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(db): extract settings queries; queries/mod.rs is now wiring only"
```

---

## Final state

```
src/db/queries/
  mod.rs        // module declarations + flat re-exports (~15 lines)
  friends.rs    // FriendEntry, PendingFriendRequest, 6 fns, 8 tests
  messaging.rs  // ChatMessage, 13 fns, 8 tests
  channels.rs   // ChannelPost, ChannelSubscription, 15 fns, 12 tests
  settings.rs   // 3 fns, 1 test
```

Public API (`db::queries::*`) and all 121 call sites unchanged. Test count unchanged. No SQL, signature, or behavior changes.
