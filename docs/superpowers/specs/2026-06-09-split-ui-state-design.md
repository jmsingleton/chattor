# Design: Split `src/ui/state.rs` into Per-State Handler Modules

**Date:** 2026-06-09
**Status:** Approved
**Type:** Refactor (no behavior change)

## Motivation

`src/ui/state.rs` is the largest file in the repository at 1,871 lines. The graphify
knowledge-graph analysis flagged its `AppState` cluster as one of the codebase's
oversized units. Inspection shows the file is ~615 lines of production code plus
~1,256 lines of tests (59 tests), and the production code is essentially a single
525-line method — `handle_key` — implemented as one `match self` over the 8
`AppState` variants. That method is the actual god-object: every UI interaction for
every screen flows through one function.

Extracting each match arm into its own per-state handler turns a 525-line method into
a thin dispatcher plus eight focused handlers, and splits the monolithic file into
seven files each owning one screen's key handling and its tests.

## Goal & Non-Goals

**Goal:** Decompose `state.rs` into a `state/` directory module — a thin dispatcher
in `mod.rs` plus one handler file per `AppState` variant — with zero behavior change
and zero call-site churn.

**Non-Goals:**
- No change to key handling behavior, state transitions, or returned `AppAction`s.
- No change to the `AppState`/`AppAction` enum definitions.
- No change to rendering code (which matches `AppState` variants externally).
- No new functionality.

## Constraints (verified against source)

- `src/ui/mod.rs` declares `pub mod state;`. Converting `state.rs` to `state/mod.rs`
  keeps this resolving unchanged.
- Callers reference `state::AppState`, `state::AppAction`, and `.handle_key(...)`.
  `AppState`/`AppAction` stay defined in `mod.rs`, so all those paths are unchanged.
- `impl AppState` contains exactly one method: `handle_key`. (`Default::default` is a
  separate `impl Default for AppState`.) Extraction is therefore clean.
- `friend_count` (the second `handle_key` parameter) is used **only** in the `Normal`
  arm (two call sites). Only `handle_normal_key` needs that parameter; the other
  handlers take just `key`.
- Every arm reassigns `*self = AppState::…` to transition, so handlers must take
  `&mut self`.
- 59 tests live in one `#[cfg(test)] mod tests` block; they call the public
  `handle_key`, not the private arm logic.
- Baseline: `cargo test --lib` green (361 passed, 4 ignored before this change).

## Design

### Module structure

```
src/ui/state/
  mod.rs              // AppState enum, AppAction enum, impl Default for AppState,
                      // impl AppState { pub fn handle_key } dispatcher,
                      // and `mod normal; mod adding_friend; ...` declarations
  normal.rs           // handle_normal_key            (+ Normal tests)
  adding_friend.rs    // handle_adding_friend_key     (+ AddingFriend tests)
  friend_requests.rs  // handle_viewing_friend_requests_key
                      // + handle_viewing_friend_request_key  (+ both request tests)
  identity.rs         // handle_viewing_my_identity_key (+ ViewingMyIdentity tests)
  ephemeral.rs        // handle_setting_ephemeral_key  (+ SettingEphemeral tests)
  channel.rs          // handle_viewing_channel_key
                      // + handle_subscribing_to_channel_key  (+ channel tests)
```

### The dispatcher (`mod.rs`)

`handle_key` keeps the global Ctrl-C → `Quit` check, then routes to per-state methods:

```rust
pub fn handle_key(&mut self, key: KeyEvent, friend_count: usize) -> Result<Option<AppAction>> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(Some(AppAction::Quit));
    }
    match self {
        AppState::Normal { .. }                => self.handle_normal_key(key, friend_count),
        AppState::AddingFriend { .. }          => self.handle_adding_friend_key(key),
        AppState::ViewingFriendRequests { .. } => self.handle_viewing_friend_requests_key(key),
        AppState::ViewingFriendRequest { .. }  => self.handle_viewing_friend_request_key(key),
        AppState::ViewingMyIdentity { .. }     => self.handle_viewing_my_identity_key(key),
        AppState::SettingEphemeral { .. }      => self.handle_setting_ephemeral_key(key),
        AppState::ViewingChannel { .. }        => self.handle_viewing_channel_key(key),
        AppState::SubscribingToChannel { .. }  => self.handle_subscribing_to_channel_key(key),
    }
}
```

### Per-state handlers

Each handler is an `impl AppState` method defined in its submodule with `pub(super)`
visibility — callable by the dispatcher in the parent `state` module, invisible
outside it. It re-binds its own state via a `let-else` whose `unreachable!` documents
the dispatcher invariant, then runs the original arm body verbatim:

```rust
// normal.rs
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
            selected_friend_idx, conversation_id, input, cursor, input_focused, scroll_offset
        } = self
        else { unreachable!("handle_normal_key requires AppState::Normal") };

        // ...original Normal arm body, verbatim...
    }
}
```

The arm body — key matching, `*self = AppState::…` transitions, `AppAction` returns,
and calls into `crate::ui::input::*` helpers — moves unchanged. Handlers that don't
use `friend_count` omit the parameter; handlers that don't need `KeyModifiers` omit
that import (the compiler flags unused imports under `clippy -D warnings`).

### Tests

The 59 tests distribute to the submodule of the state they exercise (e.g.
`test_subscribing_to_channel_*` → `channel.rs`, `test_ephemeral_*` → `ephemeral.rs`).
Each submodule gains its own `#[cfg(test)] mod tests`. Tests call the public
`handle_key`, so each test module imports `AppState`/`AppAction` (via `use super::*`
plus any needed crate paths — compiler-guided) and `crossterm` key types. No test
assertion or logic changes.

## Error Handling & Data Flow

Unchanged. `handle_key` remains the sole public entry point with the same signature
and return type. The global key check, every per-state branch, every state
transition, and every returned `AppAction` are byte-identical to the original arm
bodies. Rendering code that matches `AppState` variants is unaffected.

## Testing & Verification

Done only when **all** pass:

1. `cargo build` — clean.
2. `cargo test` — same counts as baseline, all green (the 59 redistributed state
   tests plus the full suite). Run with `--test-threads=1` to avoid a known
   pre-existing `$HOME`-race flake in `app.rs` tests (unrelated to this change).
3. `cargo clippy --lib -- -D warnings` — no new lints (watch for unused imports per
   file, e.g. `KeyModifiers` only where used).
4. `cargo fmt --check` — clean.
5. Zero call-site churn: `git diff <base> --name-only -- '*.rs' | grep -v '^src/ui/state'`
   is empty.

## Risk Assessment

**Medium** — higher than the `db/queries.rs` split because the arms are control flow,
not data. Mitigated by: (a) verbatim arm-body moves; (b) the 59 existing tests form a
tight behavior net; (c) the `let-else` re-bind makes the dispatcher invariant
explicit. The single mechanical subtlety is the `let-else` re-bind replacing the
original inline destructure — behavior-identical because the dispatcher only routes a
state to its matching handler. Rollback is per-commit revert.
