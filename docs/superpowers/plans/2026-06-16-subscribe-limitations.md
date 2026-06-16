# Subscribe-Flow Limitations & Polish — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the chattor TUI subscribe to friends-only channels and resolve friend codes, fix a flaky parallel test at its root cause, and clear two review nits.

**Architecture:** The subscribe modal (`AppState::SubscribingToChannel`) gains a `channel_type` field toggled with Tab; `AppAction::SubscribeToChannel` carries the chosen type; the main-loop handler resolves a `.onion`-or-friend-code target (new pure helper in `protocol::friend_code`) and re-opens the modal with an inline error on failure. The flaky App tests stop mutating the process-global `HOME` env var and use `App::new_with_settings` with isolated temp dirs.

**Tech Stack:** Rust, ratatui 0.27, crossterm 0.27, rusqlite/SQLCipher, tempfile.

**Spec:** `docs/superpowers/specs/2026-06-16-subscribe-limitations-design.md`

**Conventions:** Run all commands from repo root. After each task: `cargo fmt`, then `cargo clippy -- -D warnings` must be clean, then commit. KNOWN-FLAKY note: `test_app_has_phase2_components` races under parallel test runs until Task 1 lands — after Task 1 it must be deterministic.

---

### Task 1: Fix the flaky App tests (root cause: shared global HOME)

`test_app_creation_with_temp_dirs` and `test_app_has_phase2_components` both call `std::env::set_var("HOME", ...)` then `App::new(None)`, which derives the DB path from process-global `HOME`. Parallel test threads stomp each other's `HOME` and open the same DB file → lock race. Fix: construct explicit `Settings` with isolated temp dirs and call `App::new_with_settings`, exactly like `test_app_init_tor_real` already does.

**Files:**
- Modify: `src/app.rs` (the two `#[test]` fns near lines 172–196)

- [ ] **Step 1: Rewrite both tests to use isolated Settings**

Replace `test_app_creation_with_temp_dirs` and `test_app_has_phase2_components` (the two non-`#[ignore]`, non-async tests that call `set_var("HOME", ...)`) with:

```rust
    fn temp_settings(temp_dir: &TempDir) -> crate::config::Settings {
        crate::config::Settings {
            config_dir: temp_dir.path().to_path_buf(),
            data_dir: temp_dir.path().to_path_buf(),
            db_path: temp_dir.path().join("messages.db"),
            debug: false,
            tor_socks_port: 9050,
        }
    }

    #[test]
    fn test_app_creation_with_temp_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let app = App::new_with_settings(temp_settings(&temp_dir), None);
        assert!(app.is_ok());
    }

    #[test]
    fn test_app_has_phase2_components() {
        let temp_dir = TempDir::new().unwrap();
        let app = App::new_with_settings(temp_settings(&temp_dir), None).unwrap();

        // Verify Phase 2 components exist
        assert!(app.tor_client.is_none()); // Not initialized by default
        assert!(app.hidden_service.is_none());
        assert!(app.onion_address.is_none());
        assert!(app.connection_pool.is_none());
    }
```

(Confirm the `Settings` field list matches `test_app_init_tor_real`: `config_dir`, `data_dir`, `db_path`, `debug`, `tor_socks_port`. If `Settings` has more fields, copy them from that test's construction.)

- [ ] **Step 2: Verify deterministic under parallel runs**

Run: `cargo test --lib test_app_ 2>&1 | tail -5`
Expected: all pass. Then stress it: `for i in 1 2 3 4 5; do cargo test --lib app::tests 2>&1 | grep "test result" ; done`
Expected: `0 failed` every iteration.

- [ ] **Step 3: Commit**

```bash
cargo fmt
git add src/app.rs
git commit -m "test(app): isolate App tests with explicit Settings (fixes parallel HOME race)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Drop unused `TextInput::is_empty`

It's `#[allow(dead_code)]` and unused in production (handlers use `input.text().trim().is_empty()`, which `is_empty` can't replace because trimming matters). Remove it and its test.

**Files:**
- Modify: `src/ui/widgets/text_input.rs`

- [ ] **Step 1: Remove the method**

Delete this method (including the `#[allow(dead_code)]` line above it):

```rust
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }
```

- [ ] **Step 2: Remove its unit test**

Delete the `typing_appends_text` assertion line `assert!(!input.is_empty());` (keep the rest of that test), and delete any standalone test that calls `is_empty`. Grep first: `grep -n "is_empty" src/ui/widgets/text_input.rs` — remove every reference inside this file.

- [ ] **Step 3: Verify**

Run: `cargo test ui::widgets::text_input 2>&1 | tail -3` (expect all pass) and `cargo clippy -- -D warnings 2>&1 | tail -3` (expect clean).

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add src/ui/widgets/text_input.rs
git commit -m "refactor(ui): drop unused TextInput::is_empty

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Channel-type selection in the subscribe modal

Adds a `channel_type` to `SubscribingToChannel`, a Tab toggle, threads the chosen type through `AppAction` into the subscribe handler. After this task, friends-only subscribe works from the TUI (with a raw `.onion`; friend-code resolution is Task 4). The `AppAction` arity change forces same-task updates across state, render, dispatch, and main.rs so everything compiles.

**Files:**
- Modify: `src/ui/state/mod.rs` (variant + `AppAction::SubscribeToChannel` arity)
- Modify: `src/ui/state/channel.rs` (handler + tests)
- Modify: `src/ui/state/normal.rs` (`'s'` construction)
- Modify: `src/ui/modals.rs` (`render_subscribe_channel_modal`)
- Modify: `src/ui/app_ui.rs` (dispatch arm)
- Modify: `src/main.rs` (`SubscribeToChannel` handler uses channel_type)

- [ ] **Step 1: Change the state variant (src/ui/state/mod.rs)**

Replace the `SubscribingToChannel` variant:

```rust
    SubscribingToChannel {
        input: Box<crate::ui::widgets::text_input::TextInput>,
        channel_type: String, // "public" or "friends_only"
        error: Option<String>,
    },
```

And change the `AppAction` variant from:

```rust
    SubscribeToChannel(String),         // publisher .onion address
```
to:
```rust
    SubscribeToChannel(String, String), // (publisher .onion or friend code, channel_type)
```

- [ ] **Step 2: Update the handler with the Tab toggle (src/ui/state/channel.rs)**

Replace the `SubscribingToChannel` match body in `handle_subscribing_to_channel_key`:

```rust
            AppState::SubscribingToChannel {
                input,
                channel_type,
                error,
            } => match key.code {
                KeyCode::Tab => {
                    *channel_type = if channel_type == "public" {
                        "friends_only".to_string()
                    } else {
                        "public".to_string()
                    };
                    Ok(None)
                }
                KeyCode::Enter => {
                    let text = input.text().trim().to_string();
                    if text.is_empty() {
                        *error = Some("Please enter a channel address".to_string());
                        Ok(None)
                    } else {
                        Ok(Some(AppAction::SubscribeToChannel(
                            text,
                            channel_type.clone(),
                        )))
                    }
                }
                KeyCode::Esc => {
                    *self = AppState::default();
                    Ok(None)
                }
                _ => {
                    input.handle_key(key);
                    Ok(None)
                }
            },
```

- [ ] **Step 3: Update the test helper + tests (src/ui/state/channel.rs)**

Replace the `subscribing` helper and add a Tab-toggle test; update the submit test to expect the 2-arg action:

```rust
    fn subscribing(text: &str) -> AppState {
        AppState::SubscribingToChannel {
            input: Box::new(
                crate::ui::widgets::text_input::TextInput::single_line("").with_text(text),
            ),
            channel_type: "public".to_string(),
            error: None,
        }
    }

    #[test]
    fn test_subscribing_tab_toggles_channel_type() {
        let mut state = subscribing("peer.onion");
        state
            .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), 10, 0)
            .unwrap();
        match &state {
            AppState::SubscribingToChannel { channel_type, .. } => {
                assert_eq!(channel_type, "friends_only")
            }
            _ => panic!("Expected SubscribingToChannel state"),
        }
        // toggles back
        state
            .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), 10, 0)
            .unwrap();
        match &state {
            AppState::SubscribingToChannel { channel_type, .. } => {
                assert_eq!(channel_type, "public")
            }
            _ => panic!("Expected SubscribingToChannel state"),
        }
    }
```

Update `test_subscribing_to_channel_enter_submits` to expect the new action:

```rust
    #[test]
    fn test_subscribing_to_channel_enter_submits() {
        let mut state = subscribing("peer.onion");
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10, 0)
            .unwrap();
        assert_eq!(
            action,
            Some(AppAction::SubscribeToChannel(
                "peer.onion".to_string(),
                "public".to_string()
            ))
        );
    }
```

Add a friends-only submit test:

```rust
    #[test]
    fn test_subscribing_friends_only_submit() {
        let mut state = subscribing("peer.onion");
        state
            .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), 10, 0)
            .unwrap();
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10, 0)
            .unwrap();
        assert_eq!(
            action,
            Some(AppAction::SubscribeToChannel(
                "peer.onion".to_string(),
                "friends_only".to_string()
            ))
        );
    }
```

(The other two tests — `test_subscribing_to_channel_typing`, `_enter_empty_shows_error`, `_escape` — keep working with the new helper since they match `{ input, .. }` / `{ error, .. }`.)

- [ ] **Step 4: Update the `'s'` construction (src/ui/state/normal.rs)**

```rust
                        KeyCode::Char('s') => {
                            *self = AppState::SubscribingToChannel {
                                input: Box::new(
                                    crate::ui::widgets::text_input::TextInput::single_line(
                                        "Publisher's .onion or friend code",
                                    ),
                                ),
                                channel_type: "public".to_string(),
                                error: None,
                            };
                            Ok(None)
                        }
```

- [ ] **Step 5: Update the render fn (src/ui/modals.rs)**

Replace `render_subscribe_channel_modal` with (adds a `channel_type` param + selector line; restructures constraints to 5 rows):

```rust
/// Render "Subscribe to Channel" modal
pub fn render_subscribe_channel_modal(
    f: &mut Frame,
    input: &mut TextInput,
    channel_type: &str,
    error: Option<&str>,
    theme: &Theme,
) {
    let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 60, 40, 50, 13);
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Subscribe to Channel ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.channel_border));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // prompt
            Constraint::Length(3), // input
            Constraint::Length(1), // channel-type selector
            Constraint::Length(1), // help/error
            Constraint::Length(1), // controls
        ])
        .split(area);

    let prompt = Paragraph::new("Enter publisher's .onion or friend code:");
    f.render_widget(prompt, chunks[0]);

    input.render(f, chunks[1], true, theme);

    // Channel-type selector: active option bold + arrow, inactive dim.
    let public_active = channel_type == "public";
    let active_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(theme.fg_dim);
    let selector = Line::from(vec![
        Span::styled("Channel:  ", Style::default().fg(theme.fg_dim)),
        Span::styled(
            if public_active { "\u{25b8} Public" } else { "  Public" },
            if public_active { active_style } else { inactive_style },
        ),
        Span::raw("    "),
        Span::styled(
            if public_active { "  Friends-only" } else { "\u{25b8} Friends-only" },
            if public_active { inactive_style } else { active_style },
        ),
    ]);
    f.render_widget(Paragraph::new(selector), chunks[2]);

    let help = if let Some(err) = error {
        Paragraph::new(err).style(Style::default().fg(theme.error))
    } else {
        Paragraph::new("[Tab] switch channel type")
            .style(Style::default().fg(theme.fg_dim))
    };
    f.render_widget(help, chunks[3]);

    let controls = Paragraph::new("[Enter] Subscribe    [Esc] Cancel")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[4]);

    f.render_widget(block, area);
}
```

Confirm `Modifier`, `Line`, `Span` are already imported in `modals.rs` (the add-friend modal uses them); if `Modifier` is missing, add it to the ratatui `use`. Confirm `theme.accent` exists (used in footer rendering — `app_ui.rs` references `ctx.theme.accent`); if not, use `theme.channel_border`.

- [ ] **Step 6: Update the dispatch arm (src/ui/app_ui.rs)**

Replace the `SubscribingToChannel` dispatch arm (around line 228):

```rust
        AppState::SubscribingToChannel {
            input,
            channel_type,
            error,
        } => {
            let err = error.clone();
            crate::ui::modals::render_subscribe_channel_modal(
                f,
                input,
                channel_type,
                err.as_deref(),
                &ctx.theme,
            );
        }
```

(The footer-hints arm at ~line 355 matches `SubscribingToChannel { .. }` and needs no change. Optionally add `("Tab", "Type")` to its hint vec.)

- [ ] **Step 7: Update the main.rs handler to use channel_type**

Replace the `SubscribeToChannel` action arm in `src/main.rs` (currently `Some(AppAction::SubscribeToChannel(publisher_onion)) => { ... hardcoded "public" ... }`):

```rust
                        Some(AppAction::SubscribeToChannel(publisher_onion, channel_type)) => {
                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();

                            // Store subscription locally
                            db::queries::add_channel_subscription(
                                &app_lock.db,
                                &publisher_onion,
                                &channel_type,
                            )
                            .ok();

                            let wire_channel_type = if channel_type == "public" {
                                protocol::message::ChannelType::Public
                            } else {
                                protocol::message::ChannelType::FriendsOnly
                            };

                            // Send subscribe message to publisher
                            let sub_msg = protocol::message::Message::ChannelSubscribe(
                                protocol::message::ChannelSubscribeMessage {
                                    subscriber_onion: own_onion,
                                    channel_type: wire_channel_type,
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs()
                                        as i64,
                                },
                            );
                            app_lock
                                .message_queue
                                .enqueue(&app_lock.db, &publisher_onion, &sub_msg, "normal")
                                .ok();

                            drop(app_lock);
                            app_state = AppState::default();
                        }
```

- [ ] **Step 8: Build, test, clippy**

Run:
```bash
cargo build 2>&1 | tail -5
cargo test 2>&1 | tail -6
cargo clippy -- -D warnings 2>&1 | tail -3
```
Expected: clean build, all pass, clippy clean.

- [ ] **Step 9: Commit**

```bash
cargo fmt
git add src/ui/ src/main.rs
git commit -m "feat(ui): subscribe modal supports friends-only channels (Tab toggles type)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Resolve `.onion`-or-friend-code subscribe targets

Adds a reusable pure helper in `protocol::friend_code` and uses it in the subscribe handler; on an invalid target the modal re-opens with the typed text + selected channel type + an inline error.

**Files:**
- Modify: `src/protocol/friend_code.rs` (new `resolve_onion_or_friend_code` + tests)
- Modify: `src/main.rs` (subscribe handler resolves target, re-opens modal on error)

- [ ] **Step 1: Write the failing test for the helper**

In `src/protocol/friend_code.rs` tests module, add (match the module's existing test patterns — it already tests `friend_code_to_onion`):

```rust
    #[test]
    fn resolve_passes_through_onion() {
        // Valid v3 onion derived from a known key (same pattern as
        // roundtrip_onion_to_code_and_back).
        let onion = pubkey_to_onion(&[0u8; 32]).unwrap();
        assert_eq!(resolve_onion_or_friend_code(&onion).unwrap(), onion);
    }

    #[test]
    fn resolve_converts_friend_code() {
        let onion = pubkey_to_onion(&[0u8; 32]).unwrap();
        let code = onion_to_friend_code(&onion).unwrap();
        assert_eq!(resolve_onion_or_friend_code(&code).unwrap(), onion);
    }

    #[test]
    fn resolve_rejects_garbage() {
        assert!(resolve_onion_or_friend_code("not a real code").is_err());
    }
```

(Verified against the module: `pubkey_to_onion(&[u8; 32]) -> Result<String>`, `onion_to_friend_code`, and `friend_code_to_onion` all exist and round-trip — this mirrors the existing `roundtrip_onion_to_code_and_back` test. `pubkey_to_onion` is in scope via the test module's `use super::*;`.)

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test resolve_ 2>&1 | tail -5`
Expected: COMPILE FAIL — `resolve_onion_or_friend_code` not found.

- [ ] **Step 3: Implement the helper**

Add to `src/protocol/friend_code.rs` (near `friend_code_to_onion`):

```rust
/// Resolve a subscribe/add target that may be either a raw `.onion` address or
/// a 32-word friend code. `.onion` inputs pass through unchanged; anything else
/// is treated as a friend code and converted.
pub fn resolve_onion_or_friend_code(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.ends_with(".onion") {
        Ok(trimmed.to_string())
    } else {
        friend_code_to_onion(trimmed)
    }
}
```

(`Result` here is the module's existing `crate::error::Result` alias — confirm the file's existing `use`/return types and match them.)

- [ ] **Step 4: Run the helper tests**

Run: `cargo test resolve_ 2>&1 | tail -5`
Expected: 3 passed.

- [ ] **Step 5: Use it in the subscribe handler (src/main.rs)**

Change the start of the `SubscribeToChannel` arm to resolve the target first and re-open the modal on failure. Replace the handler body from Task 3 with:

```rust
                        Some(AppAction::SubscribeToChannel(target, channel_type)) => {
                            let publisher_onion = match protocol::friend_code::resolve_onion_or_friend_code(&target) {
                                Ok(onion) => onion,
                                Err(_) => {
                                    app_state = AppState::SubscribingToChannel {
                                        input: Box::new(
                                            crate::ui::widgets::text_input::TextInput::single_line(
                                                "Publisher's .onion or friend code",
                                            )
                                            .with_text(&target),
                                        ),
                                        channel_type,
                                        error: Some(
                                            "Invalid .onion address or friend code".to_string(),
                                        ),
                                    };
                                    continue;
                                }
                            };

                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();

                            db::queries::add_channel_subscription(
                                &app_lock.db,
                                &publisher_onion,
                                &channel_type,
                            )
                            .ok();

                            let wire_channel_type = if channel_type == "public" {
                                protocol::message::ChannelType::Public
                            } else {
                                protocol::message::ChannelType::FriendsOnly
                            };

                            let sub_msg = protocol::message::Message::ChannelSubscribe(
                                protocol::message::ChannelSubscribeMessage {
                                    subscriber_onion: own_onion,
                                    channel_type: wire_channel_type,
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs()
                                        as i64,
                                },
                            );
                            app_lock
                                .message_queue
                                .enqueue(&app_lock.db, &publisher_onion, &sub_msg, "normal")
                                .ok();

                            drop(app_lock);
                            app_state = AppState::default();
                        }
```

CRITICAL: verify the control-flow keyword. The action handling lives inside the main event `loop`. `continue` must continue that loop (so the re-opened modal renders next frame). Read the surrounding loop in `src/main.rs` to confirm `continue` targets the right loop and that no labeled loop / nested structure makes `continue` wrong. If the action match is NOT directly inside the render `loop` (e.g. it's in a helper returning a value), instead set `app_state` and fall through without running the subscribe block — restructure as `if let Ok(onion) = ... { <subscribe> } else { app_state = <modal-with-error> }`. Prefer the `if let Ok/else` form if there's any doubt about `continue`'s target.

- [ ] **Step 6: Build, test, clippy**

Run:
```bash
cargo build 2>&1 | tail -5
cargo test 2>&1 | tail -6
cargo clippy -- -D warnings 2>&1 | tail -3
```
Expected: clean build, all pass, clippy clean.

- [ ] **Step 7: Commit**

```bash
cargo fmt
git add src/protocol/friend_code.rs src/main.rs
git commit -m "feat(ui): subscribe accepts friend codes; re-open modal with error on bad target

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: (Optional nit) Align modal layout with `block.inner`

Cosmetic consistency: derive modal chunks from `block.inner(area)` instead of `area` + `margin(2)`, matching the identity/ephemeral/request-list modals. INCLUDE ONLY IF it stays a clean, small diff with NO visual regression (verify by eye via the TestBackend or a manual run). If it perturbs spacing, SKIP this task and note it.

**Files:**
- Modify: `src/ui/modals.rs` (`render_add_friend_modal`, `render_subscribe_channel_modal`)

- [ ] **Step 1: Switch both modals to block.inner**

For each of the two render fns, replace:
```rust
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([ ... ])
        .split(area);
```
with:
```rust
    let inner = block.inner(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([ ... ])
        .split(inner);
```
Keep the same constraint lists. `block.inner(area)` already removes the 1-cell border; `margin(1)` adds the inner padding the `margin(2)`-from-`area` previously provided. Render the block BEFORE splitting `inner` is unnecessary — `block.inner()` doesn't consume the block; keep the final `f.render_widget(block, area)`.

- [ ] **Step 2: Visual check**

Run: `cargo run` (or rely on existing modal render tests if any), open the Add-Friend (`a`) and Subscribe (`s`) modals, confirm spacing looks right and nothing is clipped. If spacing regressed, revert this task.

- [ ] **Step 3: clippy + commit**

```bash
cargo clippy -- -D warnings 2>&1 | tail -3
cargo fmt
git add src/ui/modals.rs
git commit -m "style(ui): derive add-friend + subscribe modal layout from block.inner

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Full gate**

Run, in order:
```bash
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
cargo test --test integration
cargo test --test e2e_messaging
```
Expected: all clean/passing.

- [ ] **Step 2: Manual smoke (no Tor needed for the modal)**

Run `cargo run`, press `s`, confirm: Tab toggles the Public/Friends-only selector; typing a bad code + Enter re-opens the modal with "Invalid .onion address or friend code" and preserves the typed text + selected type; Esc cancels.

- [ ] **Step 3: Confirm flaky test is fixed**

Run: `for i in 1 2 3; do cargo test --lib app::tests 2>&1 | grep "test result"; done`
Expected: `0 failed` each time.

---

## Self-review checklist (run after writing, before execution)

- Spec coverage: friends-only subscribe → Task 3; friend-code subscribe + error UX → Task 4; flaky test → Task 1; nits (is_empty, modal layout) → Tasks 2 + 5. ✓
- Type consistency: `SubscribeToChannel(String, String)` used identically in Tasks 3 (state, handler, tests, main.rs) and 4 (main.rs). `channel_type: String` "public"/"friends_only" consistent across state, render, handler, main.rs. `resolve_onion_or_friend_code(&str) -> Result<String>` defined Task 4 Step 3, used Task 4 Step 5. ✓
- Placeholders: none — every code step shows full code. ✓
- Risk flagged: `continue` control-flow caveat (Task 4 Step 5) and `theme.accent` fallback (Task 3 Step 5) and the friend-code round-trip helper name (Task 4 Step 1) each carry a verify-the-real-API note. ✓
