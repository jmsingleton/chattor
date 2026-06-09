# Split `src/ui/state.rs` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose the 1,871-line `src/ui/state.rs` — whose production code is a single 525-line `handle_key` method matching over 8 `AppState` variants — into a `state/` directory module: a thin dispatcher plus one handler file per state, with zero behavior change and zero call-site churn.

**Architecture:** Convert `state.rs` to `state/mod.rs` (holds the `AppState`/`AppAction` enums, `Default`, and the `handle_key` dispatcher). Each `match self` arm body moves verbatim into a `pub(super) fn handle_<state>_key` method on `impl AppState`, defined in a per-state submodule file; the dispatcher arm becomes a one-line call. Production handlers are extracted one state at a time — and the full 59-test suite stays in `mod.rs` validating every step — then the tests are distributed to the submodules as a final mechanical pass. Because every test drives the public `handle_key`, the build stays green at each commit.

**Tech Stack:** Rust, crossterm (key events), cargo (build/test/clippy/fmt).

**Spec:** `docs/superpowers/specs/2026-06-09-split-ui-state-design.md`

---

## Reference: `AppState` variants and their fields

```
Normal               { selected_friend_idx, conversation_id, input, cursor, input_focused, scroll_offset }
AddingFriend         { input, cursor, error }
ViewingFriendRequests{ requests, selected_idx }
ViewingFriendRequest { request_id, from_onion, friend_code, timestamp, return_to_list }
ViewingMyIdentity    { friend_code, onion_address, copied_field }
SettingEphemeral     { conversation_id, selected_idx }
ViewingChannel       { publisher_onion, channel_type, is_own, input, cursor, scroll_offset }
SubscribingToChannel { input, cursor, error }
```

## Reference: match-arm line ranges in the original `handle_key`

(In the pre-split `state.rs`. Each range is the arm body to move verbatim — the code between that arm's `=> {` and its matching closing `}`.)

| State | Arm starts (line) | Handler name | Extra param |
|-------|------|--------------|-------------|
| Normal | 97 | `handle_normal_key` | `friend_count: usize` |
| AddingFriend | 275 | `handle_adding_friend_key` | — |
| ViewingFriendRequests | 339 | `handle_viewing_friend_requests_key` | — |
| ViewingFriendRequest | 374 | `handle_viewing_friend_request_key` | — |
| ViewingMyIdentity | 422 | `handle_viewing_my_identity_key` | — |
| SettingEphemeral | 448 | `handle_setting_ephemeral_key` | — |
| ViewingChannel | 484 | `handle_viewing_channel_key` | — |
| SubscribingToChannel | 557 | `handle_subscribing_to_channel_key` | — |

The match ends at line 612, `handle_key` at 613, `impl AppState` at 614. The global
Ctrl-C check (lines 92–94) stays at the top of `handle_key` in `mod.rs`.

## Reference: the extraction transformation (applies to every handler task)

For state `X` with handler `handle_x_key`:

1. **In the new submodule file**, define the handler. Its `let-else` re-bind uses the
   **exact destructuring pattern the original arm used** (copy that arm's `AppState::X { ... }` pattern verbatim, including any `..`). Then paste the arm body verbatim.

   ```rust
   use super::{AppAction, AppState};
   use crate::error::Result;
   use crossterm::event::{KeyCode, KeyEvent, KeyModifiers}; // trim unused per compiler

   impl AppState {
       pub(super) fn handle_x_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
           let AppState::X { /* same fields the arm bound */ } = self
           else { unreachable!("handle_x_key requires AppState::X") };

           // ...original arm body, verbatim...
       }
   }
   ```

2. **In `mod.rs`**: add `mod x;` (keep declarations alphabetical), and replace the
   original `match` arm with a one-line delegation:

   ```rust
   AppState::X { .. } => self.handle_x_key(key),
   ```

**Borrow-check fallback:** the `let-else` binds `&mut` references to the state's
fields for the rest of the function, while the body later does `*self = AppState::…`.
This compiles under NLL because the field bindings are not used after the
reassignment (same as the original arm). If a specific handler nonetheless fails to
borrow-check, replace the `let-else` with an inner match that mirrors the original arm
exactly:

```rust
pub(super) fn handle_x_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
    match self {
        AppState::X { /* same fields */ } => { /* ...arm body verbatim... */ }
        _ => unreachable!("handle_x_key requires AppState::X"),
    }
}
```

Both forms are behavior-identical; the dispatcher only routes a state to its matching
handler.

---

### Task 1: Relocate `state.rs` to a directory module (no content change)

**Files:**
- Move: `src/ui/state.rs` → `src/ui/state/mod.rs`

- [ ] **Step 1: Move the file**

`src/ui/mod.rs`'s `pub mod state;` resolves `state/mod.rs` identically. `git mv`
creates the directory.

Run:
```bash
mkdir -p src/ui/state
git mv src/ui/state.rs src/ui/state/mod.rs
```

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: `Finished`, no errors (only the file location changed).

- [ ] **Step 3: Test**

Run: `cargo test --lib -- --test-threads=1`
Expected: `361 passed; 0 failed; 4 ignored` (unchanged). `--test-threads=1` avoids a
pre-existing `$HOME`-race flake in `app.rs` tests unrelated to this work.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(ui): relocate state.rs to state/ directory module"
```

---

### Task 2: Extract the `Normal` handler

Largest arm (~170 lines, the only one needing `friend_count`).

**Files:**
- Create: `src/ui/state/normal.rs`
- Modify: `src/ui/state/mod.rs`

- [ ] **Step 1: Create `src/ui/state/normal.rs`**

Apply the extraction transformation for `Normal`. Signature includes `friend_count`:

```rust
use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_normal_key(
        &mut self,
        key: KeyEvent,
        friend_count: usize,
    ) -> Result<Option<AppAction>> {
        let AppState::Normal {
            selected_friend_idx,
            conversation_id,
            input,
            cursor,
            input_focused,
            scroll_offset,
        } = self
        else { unreachable!("handle_normal_key requires AppState::Normal") };

        // ...the Normal arm body (original lines ~104-274), verbatim...
    }
}
```

- [ ] **Step 2: Wire `mod.rs`**

Add `mod normal;` (alphabetical) near the top of `mod.rs`. Replace the entire `Normal`
match arm with:
```rust
AppState::Normal { .. } => self.handle_normal_key(key, friend_count),
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors. (If a borrow-check error mentions the field bindings
vs `*self = …`, apply the inner-match fallback from the transformation reference.)

- [ ] **Step 4: Test**

Run: `cargo test --lib -- --test-threads=1`
Expected: `361 passed; 0 failed; 4 ignored` (all 59 state tests still in `mod.rs`,
all green).

- [ ] **Step 5: Clippy**

Run: `cargo clippy --lib -- -D warnings`
Expected: clean. If it flags an unused import in `normal.rs` (e.g. `KeyModifiers` if
the Normal body doesn't use modifiers), remove that specific import and re-run.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ui): extract Normal key handler into state/normal.rs"
```

---

### Task 3: Extract the `AddingFriend` handler

**Files:**
- Create: `src/ui/state/adding_friend.rs`
- Modify: `src/ui/state/mod.rs`

- [ ] **Step 1: Create `src/ui/state/adding_friend.rs`**

Apply the extraction transformation for `AddingFriend` (no `friend_count`):

```rust
use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_adding_friend_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        let AppState::AddingFriend { /* same fields the original arm bound */ } = self
        else { unreachable!("handle_adding_friend_key requires AppState::AddingFriend") };

        // ...the AddingFriend arm body (original lines ~275-338), verbatim...
    }
}
```

- [ ] **Step 2: Wire `mod.rs`**

Add `mod adding_friend;` (alphabetical). Replace the `AddingFriend` arm with:
```rust
AppState::AddingFriend { .. } => self.handle_adding_friend_key(key),
```

- [ ] **Step 3: Build** — Run `cargo build`. Expected: `Finished`. (Borrow-check fallback available.)
- [ ] **Step 4: Test** — Run `cargo test --lib -- --test-threads=1`. Expected: `361 passed; 4 ignored`.
- [ ] **Step 5: Clippy** — Run `cargo clippy --lib -- -D warnings`. Expected: clean (trim unused imports).
- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ui): extract AddingFriend key handler into state/adding_friend.rs"
```

---

### Task 4: Extract both friend-request handlers

`ViewingFriendRequests` and `ViewingFriendRequest` share `friend_requests.rs`.

**Files:**
- Create: `src/ui/state/friend_requests.rs`
- Modify: `src/ui/state/mod.rs`

- [ ] **Step 1: Create `src/ui/state/friend_requests.rs`**

Two handlers in one `impl AppState` block. Each applies the transformation:

```rust
use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_viewing_friend_requests_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        let AppState::ViewingFriendRequests { /* same fields */ } = self
        else { unreachable!("handle_viewing_friend_requests_key requires AppState::ViewingFriendRequests") };
        // ...ViewingFriendRequests arm body (original lines ~339-373), verbatim...
    }

    pub(super) fn handle_viewing_friend_request_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        let AppState::ViewingFriendRequest { /* same fields */ } = self
        else { unreachable!("handle_viewing_friend_request_key requires AppState::ViewingFriendRequest") };
        // ...ViewingFriendRequest arm body (original lines ~374-421), verbatim...
    }
}
```

- [ ] **Step 2: Wire `mod.rs`**

Add `mod friend_requests;` (alphabetical). Replace both arms:
```rust
AppState::ViewingFriendRequests { .. } => self.handle_viewing_friend_requests_key(key),
AppState::ViewingFriendRequest { .. }  => self.handle_viewing_friend_request_key(key),
```

- [ ] **Step 3: Build** — `cargo build`. Expected: `Finished`. (Fallback available.)
- [ ] **Step 4: Test** — `cargo test --lib -- --test-threads=1`. Expected: `361 passed; 4 ignored`.
- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected: clean.
- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ui): extract friend-request key handlers into state/friend_requests.rs"
```

---

### Task 5: Extract the `ViewingMyIdentity` handler

**Files:**
- Create: `src/ui/state/identity.rs`
- Modify: `src/ui/state/mod.rs`

- [ ] **Step 1: Create `src/ui/state/identity.rs`**

```rust
use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_viewing_my_identity_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        let AppState::ViewingMyIdentity { /* same fields the original arm bound */ } = self
        else { unreachable!("handle_viewing_my_identity_key requires AppState::ViewingMyIdentity") };
        // ...the ViewingMyIdentity arm body (original lines ~422-447), verbatim...
    }
}
```

- [ ] **Step 2: Wire `mod.rs`** — add `mod identity;` (alphabetical); replace the arm with:
```rust
AppState::ViewingMyIdentity { .. } => self.handle_viewing_my_identity_key(key),
```

- [ ] **Step 3: Build** — `cargo build`. Expected: `Finished`. (Fallback available.)
- [ ] **Step 4: Test** — `cargo test --lib -- --test-threads=1`. Expected: `361 passed; 4 ignored`.
- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected: clean.
- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ui): extract ViewingMyIdentity key handler into state/identity.rs"
```

---

### Task 6: Extract the `SettingEphemeral` handler

**Files:**
- Create: `src/ui/state/ephemeral.rs`
- Modify: `src/ui/state/mod.rs`

- [ ] **Step 1: Create `src/ui/state/ephemeral.rs`**

```rust
use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_setting_ephemeral_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        let AppState::SettingEphemeral { /* same fields the original arm bound */ } = self
        else { unreachable!("handle_setting_ephemeral_key requires AppState::SettingEphemeral") };
        // ...the SettingEphemeral arm body (original lines ~448-483), verbatim...
    }
}
```

- [ ] **Step 2: Wire `mod.rs`** — add `mod ephemeral;` (alphabetical); replace the arm with:
```rust
AppState::SettingEphemeral { .. } => self.handle_setting_ephemeral_key(key),
```

- [ ] **Step 3: Build** — `cargo build`. Expected: `Finished`. (Fallback available.)
- [ ] **Step 4: Test** — `cargo test --lib -- --test-threads=1`. Expected: `361 passed; 4 ignored`.
- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected: clean.
- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ui): extract SettingEphemeral key handler into state/ephemeral.rs"
```

---

### Task 7: Extract both channel handlers

`ViewingChannel` and `SubscribingToChannel` share `channel.rs`. After this task the
`mod.rs` `match` has no inline arm bodies left — every arm is a one-line delegation.

**Files:**
- Create: `src/ui/state/channel.rs`
- Modify: `src/ui/state/mod.rs`

- [ ] **Step 1: Create `src/ui/state/channel.rs`**

```rust
use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_viewing_channel_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        let AppState::ViewingChannel { /* same fields */ } = self
        else { unreachable!("handle_viewing_channel_key requires AppState::ViewingChannel") };
        // ...ViewingChannel arm body (original lines ~484-556), verbatim...
    }

    pub(super) fn handle_subscribing_to_channel_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        let AppState::SubscribingToChannel { /* same fields */ } = self
        else { unreachable!("handle_subscribing_to_channel_key requires AppState::SubscribingToChannel") };
        // ...SubscribingToChannel arm body (original lines ~557-611), verbatim...
    }
}
```

- [ ] **Step 2: Wire `mod.rs`** — add `mod channel;` (alphabetical); replace both arms:
```rust
AppState::ViewingChannel { .. }       => self.handle_viewing_channel_key(key),
AppState::SubscribingToChannel { .. } => self.handle_subscribing_to_channel_key(key),
```

- [ ] **Step 3: Build** — `cargo build`. Expected: `Finished`. (Fallback available.)
- [ ] **Step 4: Test** — `cargo test --lib -- --test-threads=1`. Expected: `361 passed; 4 ignored`.
- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected: clean.
- [ ] **Step 6: Verify the dispatcher is now thin**

Run: `rg -n 'self.handle_' src/ui/state/mod.rs | wc -l`
Expected: `8` (all eight arms delegate). The `match self` in `handle_key` should
contain only one-line delegations plus the preceding global Ctrl-C check.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(ui): extract channel key handlers into state/channel.rs"
```

---

### Task 8: Distribute the tests and finalize

All 59 tests are still in `mod.rs`. Move each to the submodule whose state it
exercises. **Test placement is organizational only** — every test drives the public
`handle_key`, so a test passes regardless of which module it sits in; place by the
`AppState` the test constructs as its starting state.

**Test → module assignment:**

- **`mod.rs`** (dispatcher/global — keep here): `default_state_is_normal`,
  `ctrl_c_quits_from_any_state`
- **`normal.rs`**: `normal_nav_mode_quit`, `normal_nav_mode_add_friend`,
  `normal_nav_mode_view_identity`, `normal_nav_mode_arrow_selects_friend`,
  `normal_enter_selects_friend_and_focuses_input`, `input_focused_typing`,
  `input_focused_enter_sends_message`, `input_focused_escape_unfocuses`,
  `input_focused_backspace`, `tab_initializes_friend_selection`,
  `normal_nav_mode_view_friend_requests`, `empty_enter_in_input_does_nothing`,
  `input_focused_emoji_typing`, `down_arrow_bounded_by_friend_count`,
  `page_up_increases_scroll_offset`, `page_down_decreases_scroll_offset`,
  `page_down_does_not_underflow`, `page_up_works_while_input_focused`,
  `scroll_resets_on_friend_change`, `ctrl_a_moves_to_start`, `ctrl_e_moves_to_end`,
  `ctrl_w_deletes_word_backward`, `ctrl_u_deletes_to_start`,
  `delete_key_forward_deletes`, `home_key_moves_to_start`, `end_key_moves_to_end`,
  `vim_j_navigates_down`, `vim_k_navigates_up`, `vim_j_bounded_by_friend_count`,
  `vim_k_bounded_at_zero`, `vim_j_resets_scroll_offset`,
  `vim_jk_not_active_when_input_focused`, `test_ephemeral_hotkey_with_conversation`,
  `test_ephemeral_hotkey_without_conversation`, `test_view_own_channel_hotkey`,
  `test_subscribe_channel_hotkey` (36 tests — the four `*_hotkey` tests start in
  `Normal` and exercise `handle_normal_key`)
- **`adding_friend.rs`**: `adding_friend_enter_sends`,
  `adding_friend_escape_returns_to_normal`
- **`friend_requests.rs`**: `friend_request_accept`, `friend_requests_list_navigation`,
  `friend_requests_list_enter_opens_modal`, `friend_requests_list_esc_returns_to_normal`,
  `friend_request_accept_returns_to_list`, `vim_jk_in_friend_requests`
- **`identity.rs`**: `identity_escape`
- **`ephemeral.rs`**: `test_ephemeral_selection`, `test_ephemeral_escape`,
  `vim_jk_in_ephemeral_settings`
- **`channel.rs`**: `test_subscribing_to_channel_typing`,
  `test_subscribing_to_channel_enter_submits`,
  `test_subscribing_to_channel_enter_empty_shows_error`,
  `test_subscribing_to_channel_escape`, `test_viewing_own_channel_publish`,
  `test_viewing_own_channel_empty_enter`, `test_viewing_own_channel_escape`,
  `test_viewing_remote_channel_escape`, `test_viewing_remote_channel_typing_ignored`

Total: 2 + 36 + 2 + 6 + 1 + 3 + 9 = 59.

- [ ] **Step 1: Add a test module to each submodule**

In each of `normal.rs`, `adding_friend.rs`, `friend_requests.rs`, `identity.rs`,
`ephemeral.rs`, `channel.rs`, append a test module and move the assigned tests
verbatim into it:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    // ...assigned tests, moved verbatim...
}
```

If the compiler reports `AppState`/`AppAction` unresolved in a moved test, add
`use crate::ui::state::{AppAction, AppState};` to that test module (the `super::*`
glob normally surfaces them via the submodule's own imports, but make it explicit if
not).

- [ ] **Step 2: Reduce the `mod.rs` test module**

Remove every moved test from `mod.rs`'s `#[cfg(test)] mod tests`, leaving only
`default_state_is_normal` and `ctrl_c_quits_from_any_state`.

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors.

- [ ] **Step 4: Verify no test was lost**

Run: `rg -c '#\[test\]' src/ui/state/*.rs | awk -F: '{s+=$2} END{print s}'`
Expected: `59`.

- [ ] **Step 5: Full verification gate**

Run: `cargo test -- --test-threads=1`
Expected: lib `361 passed; 4 ignored`, plus daemon `32`, e2e `6`, integration `16` —
all green, same totals as baseline.
Run: `cargo clippy --lib -- -D warnings`
Expected: clean.
Run: `cargo fmt` then `cargo fmt --check`
Expected: clean (no diff).

- [ ] **Step 6: Confirm zero call-site churn**

Run:
```bash
git diff main --name-only -- '*.rs' | grep -v '^src/ui/state' || echo "no non-state source files changed"
```
Expected: prints `no non-state source files changed` (the public API
`ui::state::{AppState, AppAction, handle_key}` was preserved, so nothing outside
`src/ui/state/` changed).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(ui): distribute state tests into per-state submodules"
```

---

## Final state

```
src/ui/state/
  mod.rs              // AppState, AppAction, Default, handle_key dispatcher, 2 global tests
  normal.rs           // handle_normal_key + 36 tests
  adding_friend.rs    // handle_adding_friend_key + 2 tests
  friend_requests.rs  // 2 request handlers + 6 tests
  identity.rs         // handle_viewing_my_identity_key + 1 test
  ephemeral.rs        // handle_setting_ephemeral_key + 3 tests
  channel.rs          // 2 channel handlers + 9 tests
```

Public API (`ui::state::{AppState, AppAction, handle_key}`) unchanged. The 525-line
`handle_key` is now a thin dispatcher (global key check + 8 one-line delegations). Test
count unchanged (59). No behavior, key-handling, or state-transition changes.
