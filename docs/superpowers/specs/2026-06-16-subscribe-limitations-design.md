# Subscribe-Flow Limitations & Polish — Design Spec

**Date:** 2026-06-16
**Branch context:** follow-up to `feat/tui-polish-tier1` (PR #12). Addresses limitations surfaced by the Tier-1 final review.

## Goal

Close four gaps left by the Tier-1 TUI polish work:

1. The TUI "Subscribe to Channel" modal can only subscribe to a publisher's **public** channel — friends-only channels are unreachable from the TUI.
2. The TUI subscribe modal only accepts a raw `.onion`; the daemon also resolves 32-word **friend codes**. Add parity.
3. `test_app_has_phase2_components` (and its sibling `test_app_creation_with_temp_dirs`) intermittently fail under parallel `cargo test`.
4. Two minor code-review nits: an unused `TextInput::is_empty` method, and modal-layout inconsistency.

These are independent fixes bundled into one plan; only #1 and #2 are user-facing behavior changes.

## Non-goals

- No changes to the publisher side (how channels accept/serve subscribers).
- No changes to the daemon RPC (`handle_channels_subscribe` already supports both channel types + friend codes).
- No migration of the channel composer to `TextInput` (deferred to a later tier).

---

## 1. Friends-only subscribe in the TUI

### Current behavior
`AppState::SubscribingToChannel { input: Box<TextInput>, error: Option<String> }`. The `SubscribeToChannel(publisher)` action handler in `src/main.rs` hardcodes `"public"` in both the local `add_channel_subscription` call and the wire `ChannelSubscribeMessage { channel_type: ChannelType::Public }`.

### Design
Add a channel-type selector to the modal, toggled with **Tab** (chosen over ←/→, which move the cursor inside the single-line input).

- **State:** `SubscribingToChannel { input: Box<TextInput>, channel_type: String, error: Option<String> }`, default `channel_type = "public"`. (`String` "public"/"friends_only" matches `ViewingChannel.channel_type` and the DB column convention.)
- **Handler (`handle_subscribing_to_channel_key`):**
  - `Tab` → toggle `channel_type` between `"public"` and `"friends_only"` (consume; return `Ok(None)`).
  - `Enter` → if input trimmed empty, set `error`; else emit `AppAction::SubscribeToChannel(text, channel_type.clone())`.
  - `Esc` → `AppState::default()`.
  - else → `input.handle_key(key)`.
- **Action:** `AppAction::SubscribeToChannel(String)` → `SubscribeToChannel(String, String)` = `(publisher, channel_type)`.
- **Render (`render_subscribe_channel_modal`):** add a selector line under the input showing the two options with the active one marked (`▸ Public   Friends-only`), and a `[Tab] switch type` hint. The render fn gains a `channel_type: &str` param.

### Modal mockup
```
╭─ Subscribe to Channel ─────────────────────╮
│  Enter publisher's .onion or friend code:  │
│  ╭──────────────────────────────────────╮  │
│  │ abcd…wxyz.onion                      │  │
│  ╰──────────────────────────────────────╯  │
│  Channel:  ▸ Public      Friends-only      │
│  [Tab] switch  [Enter] Subscribe  [Esc] Cancel │
╰────────────────────────────────────────────╯
```

---

## 2. Friend-code subscribe in the TUI

### Design
Resolve the input in the `SubscribeToChannel` action handler (`src/main.rs`), mirroring the daemon's `handle_channels_subscribe`:

- If the input ends with `.onion`, use it directly.
- Otherwise call `crate::protocol::friend_code::friend_code_to_onion(input)`.
- On resolution failure: **re-open the modal** with the typed text + selected channel type preserved and `error = Some("Invalid .onion address or friend code")` (per user decision — best UX, matches the Add-Friend modal). Today the handler cannot surface errors at all; this adds that path.
- On success: `add_channel_subscription(&db, &onion, &channel_type)` and enqueue a `ChannelSubscribe` message with `ChannelType::Public`/`FriendsOnly` derived from `channel_type`, then `AppState::default()`.

The modal prompt/hint text updates from ".onion address" to ".onion or friend code".

---

## 3. Flaky test fix (root cause)

### Root cause
`test_app_creation_with_temp_dirs` and `test_app_has_phase2_components` both do `std::env::set_var("HOME", temp_dir.path())` then `App::new(None)`. `App::new` derives `Settings` (and thus `db_path`) from process-global `HOME`. `set_var` mutates process-global state, so two of these tests running on different threads stomp each other's `HOME` and can open/create the **same** DB file concurrently → SQLite lock contention → intermittent failure.

### Design
Stop mutating the global. Rewrite both tests to construct an explicit `Settings` with temp `config_dir` + `data_dir` (+ `db_path`) and call `App::new_with_settings(settings, None)` — the pattern `test_app_init_tor_real` already uses. Each test gets a fully isolated DB; no shared global; deterministic under default parallel `cargo test`.

(`test_init_tor` is `#[ignore]`d and network-gated; leave it, or apply the same pattern opportunistically — not required.)

---

## 4. Review nits

- **`TextInput::is_empty`:** unused in production (handlers use `input.text().trim().is_empty()`, which `is_empty` can't replace because trimming matters). Drop the method and its unit test; removes the `#[allow(dead_code)]`. (YAGNI.)
- **Modal layout consistency:** `render_add_friend_modal` and `render_subscribe_channel_modal` currently split their chunk layout from `area` with `margin(2)` then draw the borderless block last. Derive chunks from `block.inner(area)` instead, consistent with the identity/ephemeral/request-list modals. Cosmetic; no behavior change. (Low priority — include only if it stays a clean, small diff.)

---

## Testing

- **State unit tests (`src/ui/state/channel.rs`):**
  - Tab toggles `channel_type` public→friends_only→public.
  - Enter emits `SubscribeToChannel(text, "public")` and `SubscribeToChannel(text, "friends_only")` per current toggle.
  - Enter on empty input sets `error`, no action.
  - Esc returns to Normal.
- **Friend-code resolution:** a unit test for the resolve helper path — valid `.onion` passes through, a valid friend code resolves to its onion, an invalid string yields an error. (If resolution lives inline in the async main-loop handler, extract a small pure helper `resolve_subscribe_target(&str) -> Result<String>` so it is unit-testable.)
- **App tests:** rewritten `test_app_has_phase2_components` + `test_app_creation_with_temp_dirs` pass under default `cargo test` (parallel) repeatedly.
- **Gate:** `cargo fmt --check`, `cargo clippy -- -D warnings`, full `cargo test` (lib + daemon + e2e + integration) green.

## Affected files

- `src/ui/state/mod.rs` — `SubscribingToChannel` variant + `AppAction::SubscribeToChannel` arity.
- `src/ui/state/channel.rs` — handler + tests.
- `src/ui/state/normal.rs` — `'s'` construction (add `channel_type: "public"`).
- `src/ui/modals.rs` — `render_subscribe_channel_modal` (+ optional `block.inner` for add-friend too).
- `src/ui/app_ui.rs` — subscribe modal dispatch (pass `channel_type`).
- `src/main.rs` — `SubscribeToChannel` handler: resolve friend code, use channel_type, re-open modal on error; `resolve_subscribe_target` helper.
- `src/ui/widgets/text_input.rs` — drop `is_empty`.
- `src/app.rs` — rewrite the two HOME-mutating tests.
- `src/protocol/friend_code.rs` — reuse existing `friend_code_to_onion` (no change expected).
