# TUI Polish Tier 1 — Broken UX Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the four broken interaction paths in the chattor TUI — unreachable subscribed channels, wrap-blind scrolling, unusable modal text inputs, and modal collapse on small terminals — by building three foundation widgets and wiring them in.

**Architecture:** Three new reusable widgets under `src/ui/widgets/` (ModalFrame = clamped centering fn, TextInput = tui-textarea wrapper, ScrollView = wrap-aware bottom-anchored scroller using `Paragraph::line_count`). Sidebar selection becomes a `SidebarSelection` enum spanning friends + channels. Remote channel posts gain publisher attribution (schema v11) so subscription feeds can filter. `render_app` changes to `&mut AppState` so widgets can hold render state and scroll offsets get clamped write-back.

**Tech Stack:** Rust, ratatui 0.27 (+ `unstable-rendered-line-info` feature), crossterm 0.27, tui-textarea 0.5, rusqlite/SQLCipher.

**Spec:** `docs/superpowers/specs/2026-06-10-tui-polish-design.md` (this plan covers Tier 1 only; Tiers 2–3 get their own plans).

**Conventions for every task:** run commands from repo root. After each task: `cargo fmt` before committing. Error handling follows `crate::error::Result` + `ChattorError::Database(format!(...))` patterns. Tier 1 does NOT migrate the conversation composer or channel composer to TextInput (that happens in Tier 3 with the growing composer) — only the two modal inputs.

---

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml:15` (ratatui), add tui-textarea after line 16

- [ ] **Step 1: Edit Cargo.toml**

Replace line 15 and add tui-textarea below crossterm:

```toml
ratatui = { version = "0.27", features = ["unstable-rendered-line-info"] }
crossterm = "0.27"
tui-textarea = "0.5"
```

`unstable-rendered-line-info` enables `Paragraph::line_count(width)`, which ScrollView needs for wrap-aware math. tui-textarea must stay at 0.5.x — 0.6+ requires ratatui ≥0.28 (verified against the crates index).

- [ ] **Step 2: Verify it resolves and builds**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished` with no errors. If cargo selects tui-textarea != 0.5.x, pin `tui-textarea = "=0.5.3"`.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: add tui-textarea 0.5 and ratatui line-count feature for TUI widgets"
```

---

### Task 2: ModalFrame widget + small-terminal safety

**Files:**
- Create: `src/ui/widgets/mod.rs`
- Create: `src/ui/widgets/modal_frame.rs`
- Modify: `src/ui/mod.rs:1-10` (register module)
- Modify: `src/ui/modals.rs` (replace `centered_rect`)
- Modify: `src/ui/app_ui.rs` (too-small guard)

- [ ] **Step 1: Write the failing test**

Create `src/ui/widgets/modal_frame.rs`:

```rust
use ratatui::layout::Rect;

/// Compute a centered modal area sized `pct_x`/`pct_y` percent of `screen`,
/// but never smaller than `min_w`×`min_h` cells. If the screen itself can't
/// fit the minimum, the modal takes the whole screen instead of collapsing.
pub fn modal_area(screen: Rect, pct_x: u16, pct_y: u16, min_w: u16, min_h: u16) -> Rect {
    let w = (((screen.width as u32) * (pct_x as u32)) / 100) as u16;
    let h = (((screen.height as u32) * (pct_y as u32)) / 100) as u16;
    let w = w.max(min_w).min(screen.width);
    let h = h.max(min_h).min(screen.height);
    Rect {
        x: screen.x + (screen.width - w) / 2,
        y: screen.y + (screen.height - h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_sizing_on_large_screen() {
        let area = modal_area(Rect::new(0, 0, 100, 40), 60, 50, 40, 10);
        assert_eq!(area.width, 60);
        assert_eq!(area.height, 20);
        assert_eq!(area.x, 20);
        assert_eq!(area.y, 10);
    }

    #[test]
    fn clamps_up_to_minimum_on_small_screen() {
        // 60% of 70 = 42 < min 50 -> use 50
        let area = modal_area(Rect::new(0, 0, 70, 24), 60, 40, 50, 12);
        assert_eq!(area.width, 50);
        assert_eq!(area.height, 12);
    }

    #[test]
    fn tiny_screen_falls_back_to_full_screen() {
        let area = modal_area(Rect::new(0, 0, 30, 8), 60, 40, 50, 12);
        assert_eq!(area, Rect::new(0, 0, 30, 8));
    }
}
```

Create `src/ui/widgets/mod.rs`:

```rust
pub mod modal_frame;
```

In `src/ui/mod.rs`, add `pub mod widgets;` after the existing `pub mod theme;` line.

- [ ] **Step 2: Run the tests**

Run: `cargo test ui::widgets::modal_frame -- --nocapture`
Expected: 3 passed. (Pure function + tests land together; the "failing" phase here is the compile error before mod.rs registration — verify the tests actually run.)

- [ ] **Step 3: Replace `centered_rect` in modals.rs**

In `src/ui/modals.rs`, delete the `centered_rect` helper (lines 438–457) and replace each call:

```rust
// render_add_friend_modal:
let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 60, 40, 50, 12);
// render_friend_request_modal:
let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 60, 50, 54, 14);
// render_friend_request_list:
let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 60, 50, 50, 12);
// render_identity_modal:
let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 70, 70, 58, 16);
// render_ephemeral_modal:
let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 50, 40, 30, 11);
// render_subscribe_channel_modal:
let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 60, 40, 50, 12);
```

- [ ] **Step 4: Add the too-small guard to render_app**

In `src/ui/app_ui.rs`, at the very top of `render_app` (before the layout split), add:

```rust
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 16;

let size = f.size();
if size.width < MIN_WIDTH || size.height < MIN_HEIGHT {
    let lines = vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::default()
                .fg(ctx.theme.warning)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "Need at least {}x{} (current {}x{})",
                MIN_WIDTH, MIN_HEIGHT, size.width, size.height
            ),
            Style::default().fg(ctx.theme.fg_dim),
        )),
    ];
    let para = Paragraph::new(lines)
        .alignment(ratatui::layout::Alignment::Center);
    let y = size.height.saturating_sub(2) / 2;
    let centered = ratatui::layout::Rect {
        x: size.x,
        y: size.y + y,
        width: size.width,
        height: 2.min(size.height),
    };
    f.render_widget(para, centered);
    return;
}
```

Move the two `const`s to module level (above `RenderContext`).

- [ ] **Step 5: Add TestBackend test for the guard**

At the bottom of `src/ui/app_ui.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::AppState;

    pub(crate) fn test_ctx() -> RenderContext {
        RenderContext {
            friends: vec![],
            messages: vec![],
            own_onion: None,
            friend_code: None,
            tor_connected: false,
            pending_request_count: 0,
            conversation_ephemeral_ttl: None,
            channel_subscriptions: vec![],
            channel_posts: vec![],
            channel_post_read_counts: Default::default(),
            theme: Theme::preset("dark"),
            presence: Default::default(),
            status_flash: None,
            continued_offline: false,
        }
    }

    fn buffer_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn tiny_terminal_renders_guard_instead_of_app() {
        let backend = ratatui::backend::TestBackend::new(40, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app_state = AppState::default();
        terminal
            .draw(|f| render_app(f, &app_state, &test_ctx()))
            .unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("Terminal too small"));
        assert!(!text.contains("chattor")); // header must not render
    }

    #[test]
    fn normal_terminal_renders_app() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app_state = AppState::default();
        terminal
            .draw(|f| render_app(f, &app_state, &test_ctx()))
            .unwrap();
        assert!(buffer_text(&terminal).contains("chattor"));
    }
}
```

Note: `Theme::preset("dark")` is the existing constructor (`src/ui/theme.rs:72`). `Line`, `Span`, `Style`, `Modifier`, `Paragraph` are already imported at the top of app_ui.rs.

- [ ] **Step 6: Run the full UI test module**

Run: `cargo test ui:: 2>&1 | tail -5`
Expected: all pass, including the 2 new app_ui tests and 3 modal_frame tests.

- [ ] **Step 7: Commit**

```bash
git add src/ui/widgets/ src/ui/mod.rs src/ui/modals.rs src/ui/app_ui.rs
git commit -m "feat(ui): ModalFrame min-size clamping + small-terminal guard"
```

---

### Task 3: Change render_app to take &mut AppState

Mechanical signature change that unblocks Tasks 5–8 (TextInput render needs `&mut`, ScrollView writes back clamped offsets).

**Files:**
- Modify: `src/ui/app_ui.rs:32` (signature), test module from Task 2
- Modify: `src/main.rs:651-653` (draw call)

- [ ] **Step 1: Change the signature**

```rust
pub fn render_app(f: &mut Frame, app_state: &mut AppState, ctx: &RenderContext) {
```

The body compiles unchanged (a `&mut` matches everywhere a `&` did; the `format_footer_spans(app_state, ...)` call reborrows immutably).

- [ ] **Step 2: Update the call site in main.rs**

At `src/main.rs:651`:

```rust
if let Err(e) = terminal.draw(|f| {
    ui::render_app(f, &mut app_state, &ctx);
}) {
```

- [ ] **Step 3: Update the Task 2 tests**

In the app_ui tests, change `let app_state = AppState::default();` to `let mut app_state = ...` and pass `&mut app_state`.

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test ui:: 2>&1 | tail -3`
Expected: clean build, tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/app_ui.rs src/main.rs
git commit -m "refactor(ui): render_app takes &mut AppState for stateful widgets"
```

---

### Task 4: TextInput widget

**Files:**
- Create: `src/ui/widgets/text_input.rs`
- Modify: `src/ui/widgets/mod.rs`

- [ ] **Step 1: Write the widget with tests**

Create `src/ui/widgets/text_input.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Borders};
use ratatui::Frame;
use tui_textarea::TextArea;

use crate::ui::theme::Theme;

/// Text input backed by tui-textarea: proper cursor rendering, unicode-width
/// handling, horizontal viewport scrolling for long input (e.g. 32-word
/// friend codes), and emacs-style editing keys for free.
pub struct TextInput {
    textarea: TextArea<'static>,
    single_line: bool,
}

impl TextInput {
    pub fn single_line(placeholder: &str) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text(placeholder);
        // The default cursor-line underline reads as a misrender in a
        // one-line input box.
        textarea.set_cursor_line_style(Style::default());
        Self {
            textarea,
            single_line: true,
        }
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.textarea.insert_str(text);
        self
    }

    /// Feed a key event to the editor. Returns false for keys the caller
    /// must handle itself: Esc always, and Enter in single-line mode.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => false,
            KeyCode::Enter if self.single_line => false,
            _ => {
                self.textarea.input(key);
                true
            }
        }
    }

    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    /// Render with a rounded border. `&mut self` because tui-textarea
    /// stores block/style on the widget itself.
    pub fn render(&mut self, f: &mut Frame, area: Rect, focused: bool, theme: &Theme) {
        let border = if focused {
            theme.border_focused
        } else {
            theme.border
        };
        self.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border)),
        );
        self.textarea.set_style(Style::default().fg(theme.input_fg));
        self.textarea
            .set_placeholder_style(Style::default().fg(theme.input_placeholder));
        f.render_widget(self.textarea.widget(), area);
    }
}

impl Clone for TextInput {
    fn clone(&self) -> Self {
        Self {
            textarea: self.textarea.clone(),
            single_line: self.single_line,
        }
    }
}

impl std::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInput")
            .field("text", &self.text())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_appends_text() {
        let mut input = TextInput::single_line("hint");
        assert!(input.handle_key(key(KeyCode::Char('h'))));
        assert!(input.handle_key(key(KeyCode::Char('i'))));
        assert_eq!(input.text(), "hi");
        assert!(!input.is_empty());
    }

    #[test]
    fn enter_not_consumed_in_single_line() {
        let mut input = TextInput::single_line("");
        assert!(!input.handle_key(key(KeyCode::Enter)));
        assert_eq!(input.text(), ""); // no newline inserted
    }

    #[test]
    fn esc_not_consumed() {
        let mut input = TextInput::single_line("");
        assert!(!input.handle_key(key(KeyCode::Esc)));
    }

    #[test]
    fn backspace_and_cursor_movement() {
        let mut input = TextInput::single_line("").with_text("abc");
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.text(), "ab");
        input.handle_key(key(KeyCode::Left));
        input.handle_key(key(KeyCode::Char('x')));
        assert_eq!(input.text(), "axb");
    }

    #[test]
    fn unicode_input_no_panic() {
        let mut input = TextInput::single_line("");
        input.handle_key(key(KeyCode::Char('🦀')));
        input.handle_key(key(KeyCode::Char('字')));
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.text(), "🦀");
    }

    #[test]
    fn with_text_starts_with_cursor_at_end() {
        let mut input = TextInput::single_line("").with_text("ab");
        input.handle_key(key(KeyCode::Char('c')));
        assert_eq!(input.text(), "abc");
    }
}
```

Add to `src/ui/widgets/mod.rs`:

```rust
pub mod text_input;
```

API note for the implementer: in tui-textarea 0.5 the render call is `f.render_widget(self.textarea.widget(), area)`. If `set_placeholder_style` doesn't exist on 0.5, drop that single line (placeholder will use the crate default dim style) — do not block on it.

- [ ] **Step 2: Run the tests**

Run: `cargo test ui::widgets::text_input 2>&1 | tail -3`
Expected: 6 passed.

- [ ] **Step 3: Commit**

```bash
git add src/ui/widgets/
git commit -m "feat(ui): TextInput widget wrapping tui-textarea"
```

---

### Task 5: Migrate Add Friend modal to TextInput

**Files:**
- Modify: `src/ui/state/mod.rs:21-25` (AddingFriend variant)
- Modify: `src/ui/state/adding_friend.rs` (handler + tests)
- Modify: `src/ui/state/normal.rs:95-102` ('a' key)
- Modify: `src/ui/modals.rs:11-64` (render)
- Modify: `src/ui/app_ui.rs` (modal dispatch)
- Modify: `src/main.rs:676-682` (error retry path)

- [ ] **Step 1: Change the state variant**

In `src/ui/state/mod.rs`:

```rust
AddingFriend {
    input: crate::ui::widgets::text_input::TextInput,
    error: Option<String>,
},
```

(`cursor` field is gone — TextInput owns it. AppState's `#[derive(Debug, Clone)]` still works via TextInput's manual impls.)

- [ ] **Step 2: Rewrite the handler**

Replace the body of `handle_adding_friend_key` in `src/ui/state/adding_friend.rs`:

```rust
impl AppState {
    pub(super) fn handle_adding_friend_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        match self {
            AppState::AddingFriend { input, error } => match key.code {
                KeyCode::Enter => {
                    let text = input.text().trim().to_string();
                    if text.is_empty() {
                        *error =
                            Some("Please enter a .onion address or friend code".to_string());
                        Ok(None)
                    } else {
                        Ok(Some(AppAction::SendFriendRequest(text)))
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
            _ => unreachable!("handle_adding_friend_key requires AppState::AddingFriend"),
        }
    }
}
```

Imports: drop `KeyModifiers` from the use line if now unused in non-test code.

- [ ] **Step 3: Update construction sites**

`src/ui/state/normal.rs` ('a' key):

```rust
KeyCode::Char('a') => {
    *self = AppState::AddingFriend {
        input: crate::ui::widgets::text_input::TextInput::single_line(
            "Paste .onion or friend code",
        ),
        error: None,
    };
    Ok(None)
}
```

`src/main.rs` (SendFriendRequest error arm, ~line 677):

```rust
Err(e) => {
    app_state = AppState::AddingFriend {
        input: crate::ui::widgets::text_input::TextInput::single_line(
            "Paste .onion or friend code",
        )
        .with_text(&code),
        error: Some(format!("Failed: {}", e)),
    };
}
```

- [ ] **Step 4: Rewrite the render fn**

In `src/ui/modals.rs`:

```rust
use crate::ui::widgets::text_input::TextInput;

/// Render "Add Friend" modal
pub fn render_add_friend_modal(
    f: &mut Frame,
    input: &mut TextInput,
    error: Option<&str>,
    theme: &Theme,
) {
    let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 60, 40, 50, 12);
    f.render_widget(Clear, area);

    let block = Block::default()
        .title("Add New Friend")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.modal_border));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    let prompt = Paragraph::new("Enter their .onion address or friend code:");
    f.render_widget(prompt, chunks[0]);

    input.render(f, chunks[1], true, theme);

    let help = if let Some(err) = error {
        Paragraph::new(err).style(Style::default().fg(theme.error))
    } else {
        Paragraph::new("Paste .onion or friend code (from [i] Identity)")
            .style(Style::default().fg(theme.fg_dim))
    };
    f.render_widget(help, chunks[2]);

    let controls = Paragraph::new("[Enter] Send    [Esc] Cancel")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[3]);

    f.render_widget(block, area);
}
```

In `src/ui/app_ui.rs` modal dispatch (the match is on `&mut AppState`, so `input` and `error` are disjoint mutable borrows — passing both works directly):

```rust
AppState::AddingFriend { input, error } => {
    let err = error.as_deref();
    crate::ui::modals::render_add_friend_modal(f, input, err, &ctx.theme);
}
```

(If the borrow checker objects to `err` + `input` together, clone the error string first — it's a tiny per-frame cost.)

- [ ] **Step 5: Update the state tests**

Replace the tests in `src/ui/state/adding_friend.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::text_input::TextInput;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn adding(text: &str) -> AppState {
        AppState::AddingFriend {
            input: TextInput::single_line("").with_text(text),
            error: None,
        }
    }

    #[test]
    fn adding_friend_typing_goes_to_input() {
        let mut state = adding("");
        state
            .handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::AddingFriend { input, .. } => assert_eq!(input.text(), "a"),
            _ => panic!("Expected AddingFriend"),
        }
    }

    #[test]
    fn adding_friend_enter_sends() {
        let mut state = adding("friend.onion");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(
            action,
            Some(AppAction::SendFriendRequest("friend.onion".to_string()))
        );
    }

    #[test]
    fn adding_friend_enter_empty_sets_error() {
        let mut state = adding("");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::AddingFriend { error, .. } => assert!(error.is_some()),
            _ => panic!("Expected AddingFriend"),
        }
    }

    #[test]
    fn adding_friend_escape_returns_to_normal() {
        let mut state = adding("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        state.handle_key(key, 10).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }
}
```

Also fix the `ctrl_c_quits_from_any_state` test in `src/ui/state/mod.rs` — its AddingFriend construction becomes:

```rust
let mut state = AppState::AddingFriend {
    input: crate::ui::widgets::text_input::TextInput::single_line(""),
    error: None,
};
```

(Note: `handle_key` still takes 2 args at this point; Task 10 changes it to 3.)

- [ ] **Step 6: Build and test**

Run: `cargo test ui:: 2>&1 | tail -3` then `cargo build 2>&1 | tail -3`
Expected: all pass; no remaining references to the old `cursor` field (compiler will catch any).

- [ ] **Step 7: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat(ui): Add Friend modal uses TextInput (long friend codes now editable)"
```

---

### Task 6: Migrate Subscribe Channel modal to TextInput

Same shape as Task 5, applied to `SubscribingToChannel`.

**Files:**
- Modify: `src/ui/state/mod.rs:55-59` (variant: `input: TextInput`, `error: Option<String>`, drop `cursor`)
- Modify: `src/ui/state/channel.rs:87-151` (handler) and its 4 `SubscribingToChannel` tests
- Modify: `src/ui/state/normal.rs:114-121` ('s' key construction)
- Modify: `src/ui/modals.rs:383-436` (render)
- Modify: `src/ui/app_ui.rs` (dispatch)

- [ ] **Step 1: Change variant + handler**

Variant:

```rust
SubscribingToChannel {
    input: crate::ui::widgets::text_input::TextInput,
    error: Option<String>,
},
```

Handler (`handle_subscribing_to_channel_key` body):

```rust
AppState::SubscribingToChannel { input, error } => match key.code {
    KeyCode::Enter => {
        let text = input.text().trim().to_string();
        if text.is_empty() {
            *error = Some("Please enter a channel address".to_string());
            Ok(None)
        } else {
            Ok(Some(AppAction::SubscribeToChannel(text)))
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

The `handle_viewing_channel_key` half of channel.rs is untouched (composer migrates in Tier 3).

- [ ] **Step 2: Update construction ('s' in normal.rs)**

```rust
KeyCode::Char('s') => {
    *self = AppState::SubscribingToChannel {
        input: crate::ui::widgets::text_input::TextInput::single_line(
            "Publisher's .onion address",
        ),
        error: None,
    };
    Ok(None)
}
```

- [ ] **Step 3: Update render + dispatch**

Replace `render_subscribe_channel_modal` in `src/ui/modals.rs`:

```rust
/// Render "Subscribe to Channel" modal
pub fn render_subscribe_channel_modal(
    f: &mut Frame,
    input: &mut TextInput,
    error: Option<&str>,
    theme: &Theme,
) {
    let area = crate::ui::widgets::modal_frame::modal_area(f.size(), 60, 40, 50, 12);
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
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    let prompt = Paragraph::new("Enter publisher's .onion address:");
    f.render_widget(prompt, chunks[0]);

    input.render(f, chunks[1], true, theme);

    let help = if let Some(err) = error {
        Paragraph::new(err).style(Style::default().fg(theme.error))
    } else {
        Paragraph::new("Subscribes to their public channel")
            .style(Style::default().fg(theme.fg_dim))
    };
    f.render_widget(help, chunks[2]);

    let controls = Paragraph::new("[Enter] Subscribe    [Esc] Cancel")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[3]);

    f.render_widget(block, area);
}
```

app_ui dispatch:

```rust
AppState::SubscribingToChannel { input, error } => {
    let err = error.as_deref();
    crate::ui::modals::render_subscribe_channel_modal(f, input, err, &ctx.theme);
}
```

- [ ] **Step 4: Update the 4 tests in channel.rs**

Replace the four `SubscribingToChannel` tests:

```rust
    fn subscribing(text: &str) -> AppState {
        AppState::SubscribingToChannel {
            input: crate::ui::widgets::text_input::TextInput::single_line("").with_text(text),
            error: None,
        }
    }

    #[test]
    fn test_subscribing_to_channel_typing() {
        let mut state = subscribing("");
        state
            .handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), 10)
            .unwrap();
        state
            .handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::SubscribingToChannel { input, .. } => assert_eq!(input.text(), "ab"),
            _ => panic!("Expected SubscribingToChannel state"),
        }
    }

    #[test]
    fn test_subscribing_to_channel_enter_submits() {
        let mut state = subscribing("peer.onion");
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
        assert_eq!(
            action,
            Some(AppAction::SubscribeToChannel("peer.onion".to_string()))
        );
    }

    #[test]
    fn test_subscribing_to_channel_enter_empty_shows_error() {
        let mut state = subscribing("");
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
        assert!(action.is_none());
        match &state {
            AppState::SubscribingToChannel { error, .. } => assert!(error.is_some()),
            _ => panic!("Expected SubscribingToChannel state"),
        }
    }

    #[test]
    fn test_subscribing_to_channel_escape() {
        let mut state = subscribing("draft");
        state
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 10)
            .unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }
```

(These still use 2-arg `handle_key`; Task 10 Step 4 mechanically adds the third arg.)

- [ ] **Step 5: Build, test, commit**

Run: `cargo test 2>&1 | tail -3`
Expected: all pass.

```bash
git add src/ui/
git commit -m "feat(ui): Subscribe Channel modal uses TextInput"
```

---

### Task 7: ScrollView widget (wrap-aware scrolling)

**Files:**
- Create: `src/ui/widgets/scroll_view.rs`
- Modify: `src/ui/widgets/mod.rs`

- [ ] **Step 1: Write the widget with the regression test for the wrap bug**

Create `src/ui/widgets/scroll_view.rs`:

```rust
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{
    Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::Frame;

use crate::ui::theme::Theme;

/// Render `lines` bottom-anchored in `area`, scrolled up by `scroll_offset`
/// display rows. Wrap-aware: long lines count for the rows they actually
/// occupy after wrapping (the old code skipped logical lines before wrap was
/// applied, clipping the newest messages).
///
/// Returns the clamped offset actually applied — callers must write it back
/// to their state so PageUp can't scroll past the top of history.
pub fn render_scrollable(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_offset: usize,
    theme: &Theme,
) -> usize {
    if area.width < 2 || area.height == 0 {
        return 0;
    }
    // Reserve the rightmost column as a scrollbar gutter so the bar never
    // overlaps text.
    let text_area = Rect {
        width: area.width - 1,
        ..area
    };

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let total = paragraph.line_count(text_area.width);
    let height = area.height as usize;
    let max_offset = total.saturating_sub(height);
    let offset = scroll_offset.min(max_offset);
    let scroll_y = (total.saturating_sub(height + offset)).min(u16::MAX as usize) as u16;

    f.render_widget(paragraph.scroll((scroll_y, 0)), text_area);

    if total > height {
        let mut sb_state = ScrollbarState::new(max_offset).position(max_offset - offset);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme.fg_dim)),
            area,
            &mut sb_state,
        );
    }

    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn draw(width: u16, height: u16, lines: Vec<String>, offset: usize) -> (String, usize) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::preset("dark");
        let mut clamped = 0;
        terminal
            .draw(|f| {
                let ls: Vec<Line> = lines.iter().map(|s| Line::from(s.as_str())).collect();
                clamped = render_scrollable(f, f.size(), ls, offset, &theme);
            })
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        (text, clamped)
    }

    #[test]
    fn bottom_line_visible_when_long_line_wraps() {
        // 100-char line wraps to ~6 rows at width 19 (20 minus gutter).
        // Old wrap-blind math would have clipped "LAST" off-screen.
        let (text, _) = draw(20, 5, vec!["x".repeat(100), "LAST".to_string()], 0);
        assert!(text.contains("LAST"));
    }

    #[test]
    fn offset_clamps_to_top_of_history() {
        let lines: Vec<String> = (0..10).map(|i| format!("line{}", i)).collect();
        let (text, clamped) = draw(20, 4, lines, 999);
        assert_eq!(clamped, 6); // 10 lines - 4 visible
        assert!(text.contains("line0")); // clamped to very top
    }

    #[test]
    fn no_scroll_needed_fits_entirely() {
        let (text, clamped) = draw(20, 10, vec!["a".to_string(), "b".to_string()], 0);
        assert_eq!(clamped, 0);
        assert!(text.contains('a') && text.contains('b'));
    }

    #[test]
    fn zero_size_area_is_safe() {
        let backend = TestBackend::new(5, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::preset("dark");
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 1, 0);
                assert_eq!(render_scrollable(f, area, vec![], 5, &theme), 0);
            })
            .unwrap();
    }
}
```

Add `pub mod scroll_view;` to `src/ui/widgets/mod.rs`.

- [ ] **Step 2: Run the tests**

Run: `cargo test ui::widgets::scroll_view 2>&1 | tail -3`
Expected: 4 passed. If `line_count` is missing, the ratatui feature flag from Task 1 isn't active — fix Cargo.toml, don't work around it.

- [ ] **Step 3: Commit**

```bash
git add src/ui/widgets/
git commit -m "feat(ui): wrap-aware ScrollView with scrollbar and offset clamping"
```

---

### Task 8: Wire ScrollView into conversation and channel feed

**Files:**
- Modify: `src/ui/conversation.rs` (`render_conversation`, `render_messages` return clamped offset; delete manual skip math at lines 231-243)
- Modify: `src/ui/channel_feed.rs` (`render_channel_feed`, `render_posts` likewise; delete lines 127-139)
- Modify: `src/ui/app_ui.rs` (plumb mutable offsets)

- [ ] **Step 1: Convert render_messages**

In `src/ui/conversation.rs`, change `render_messages` to return the clamped offset. Keep all the line-building logic (date separators, sender lines, status glyphs) exactly as is; replace everything from `// Apply scroll offset` (line 231) through the final `f.render_widget` with:

```rust
    crate::ui::widgets::scroll_view::render_scrollable(f, area, lines, scroll_offset, theme)
```

and change the fn signature to `-> usize`. `render_conversation` also returns `usize`: the `Some(friend_entry)` arm returns the value from `render_messages` (or `0` in the empty-messages branch); the `None` arm returns `0`. Both early paths need explicit `0` returns.

- [ ] **Step 2: Convert render_posts**

Same change in `src/ui/channel_feed.rs`: `render_posts` returns `crate::ui::widgets::scroll_view::render_scrollable(f, inner, lines, scroll_offset, theme)` (note: `inner`, the block's inner rect), empty-posts branch returns `0`. `render_channel_feed` returns the value from `render_posts` in both `is_own` branches.

- [ ] **Step 3: Plumb write-back in app_ui.rs**

Restructure the main-area section of `render_app` (currently lines 82-193). Replace it with:

```rust
    // Main area -- depends on state
    if let AppState::ViewingChannel {
        publisher_onion,
        channel_type,
        is_own,
        input,
        cursor,
        scroll_offset,
    } = app_state
    {
        let clamped = crate::ui::channel_feed::render_channel_feed(
            f,
            chunks[1],
            publisher_onion,
            channel_type,
            *is_own,
            input,
            *cursor,
            *scroll_offset,
            &ctx.channel_posts,
            &ctx.channel_post_read_counts,
            &ctx.theme,
        );
        *scroll_offset = clamped;
    } else {
        let (selected_idx, input_text, cursor, input_focused, scroll_offset) =
            if let AppState::Normal {
                selected_friend_idx,
                input,
                cursor,
                input_focused,
                scroll_offset,
                ..
            } = &*app_state
            {
                (
                    *selected_friend_idx,
                    input.clone(),
                    *cursor,
                    *input_focused,
                    *scroll_offset,
                )
            } else {
                (None, String::new(), 0, false, 0)
            };

        let clamped = render_main_area(
            f,
            chunks[1],
            selected_idx,
            &input_text,
            cursor,
            input_focused,
            scroll_offset,
            ctx,
        );

        if let AppState::Normal { scroll_offset, .. } = app_state {
            *scroll_offset = clamped;
        }
    }
```

And extract the existing sidebar/conversation/input rendering into a helper in the same file (body is the current code, with `render_conversation`'s return value captured):

```rust
/// Render sidebar + conversation + input. Returns the clamped scroll offset.
#[allow(clippy::too_many_arguments)]
fn render_main_area(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    selected_idx: Option<usize>,
    input: &str,
    cursor: usize,
    input_focused: bool,
    scroll_offset: usize,
    ctx: &RenderContext,
) -> usize {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(0)])
        .split(area);

    crate::ui::sidebar::render_sidebar_with_channels(
        f,
        main_chunks[0],
        &ctx.friends,
        selected_idx,
        !input_focused,
        ctx.pending_request_count,
        &ctx.channel_subscriptions,
        &ctx.presence,
        &ctx.theme,
    );

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(main_chunks[1]);

    let selected_friend = selected_idx.and_then(|i| ctx.friends.get(i));
    let friend_is_typing = selected_friend
        .map(|fr| {
            ctx.presence
                .get(&fr.onion_address)
                .is_some_and(|(_, typing)| *typing)
        })
        .unwrap_or(false);

    let clamped = crate::ui::conversation::render_conversation(
        f,
        right_chunks[0],
        selected_friend,
        &ctx.messages,
        ctx.own_onion.as_deref(),
        scroll_offset,
        ctx.conversation_ephemeral_ttl,
        friend_is_typing,
        &ctx.theme,
    );

    crate::ui::conversation::render_input(
        f,
        right_chunks[1],
        input,
        cursor,
        input_focused,
        &ctx.theme,
    );

    clamped
}
```

(Task 10 changes `selected_idx: Option<usize>` to the sidebar enum; here it keeps the current shape.)

- [ ] **Step 4: Add a render regression test**

In the app_ui tests module, add:

```rust
    #[test]
    fn scrolled_offset_gets_clamped_back_into_state() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app_state = AppState::Normal {
            selected_friend_idx: None,
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 9999,
        };
        terminal
            .draw(|f| render_app(f, &mut app_state, &test_ctx()))
            .unwrap();
        match app_state {
            AppState::Normal { scroll_offset, .. } => assert_eq!(scroll_offset, 0),
            _ => panic!("state changed unexpectedly"),
        }
    }
```

(No friend selected → no messages → max offset 0 → 9999 clamps to 0.)

- [ ] **Step 5: Build, test, commit**

Run: `cargo test 2>&1 | tail -3`
Expected: all pass.

```bash
git add src/ui/
git commit -m "fix(ui): wrap-aware scrolling in conversation and channel feed via ScrollView"
```

---

### Task 9: Publisher attribution for channel posts (schema v11)

Remote posts are all stored as `channel_id=0` with no publisher — a per-publisher subscription feed is impossible without this. Callers of `store_channel_post` (per the dependency graph): `src/handlers/messaging.rs` (2 sites), `src/main.rs:1032`, `src/daemon/rpc/channels.rs`, plus 5 tests in `src/db/queries/channels.rs`.

**Files:**
- Modify: `src/db/schema.rs` (SCHEMA_VERSION 10→11, channel_posts columns, version test at line ~191)
- Modify: `src/db/connection.rs` (migration chain + `migrate_to_v11`)
- Modify: `src/db/queries/channels.rs` (`store_channel_post` signature, new query, tests)
- Modify: `src/handlers/messaging.rs:223-247` and the `ChannelSyncResponse` arm (~line 291)
- Modify: `src/main.rs:1032` (publish)
- Modify: `src/daemon/rpc/channels.rs` (publish handler)

- [ ] **Step 1: Write the failing test for the new query**

In the tests module of `src/db/queries/channels.rs` (match the setup pattern used by `test_store_and_get_channel_posts` at line ~385 — same temp-DB helper):

```rust
    #[test]
    fn test_publisher_channel_posts_filtered() {
        let (db, _tmp) = test_db();
        store_channel_post(&db, 0, "from alice", "post-a1", 100, "sig", Some("alice.onion"), Some("public")).unwrap();
        store_channel_post(&db, 0, "from bob", "post-b1", 200, "sig", Some("bob.onion"), Some("public")).unwrap();
        store_channel_post(&db, 1, "my own", "post-me", 300, "sig", None, None).unwrap();

        let posts = get_publisher_channel_posts(&db, "alice.onion", 100).unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].content, "from alice");
    }
```

(If the existing tests construct the DB inline instead of a `test_db()` helper, copy that exact construction.)

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test test_publisher_channel_posts_filtered 2>&1 | tail -5`
Expected: COMPILE FAIL — `store_channel_post` takes 6 args, `get_publisher_channel_posts` not found.

- [ ] **Step 3: Schema + migration**

`src/db/schema.rs`:
- `pub const SCHEMA_VERSION: i32 = 11;`
- Update the version assertion test (line ~191) to 11.
- In `CREATE_TABLES`, replace the channel_posts table definition with:

```sql
CREATE TABLE IF NOT EXISTS channel_posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    post_id TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    signature TEXT NOT NULL,
    publisher_onion TEXT,
    channel_type TEXT,
    FOREIGN KEY (channel_id) REFERENCES channels(id)
);
```

- Next to the existing channel_posts indices add:

```sql
CREATE INDEX IF NOT EXISTS idx_channel_posts_publisher ON channel_posts(publisher_onion);
```

`src/db/connection.rs`:
- Add `self.migrate_to_v11()?;` after the `migrate_to_v10()` call in the chain (~line 63).
- Add, modeled exactly on `migrate_to_v10`:

```rust
    fn migrate_to_v11(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 11 {
            info!("Migrating database to schema v11 (channel post publisher attribution)");

            let conn = self.connection();

            let has_column: bool = conn
                .prepare("SELECT publisher_onion FROM channel_posts LIMIT 0")
                .is_ok();

            if !has_column {
                conn.execute_batch(
                    "ALTER TABLE channel_posts ADD COLUMN publisher_onion TEXT;
                     ALTER TABLE channel_posts ADD COLUMN channel_type TEXT;
                     CREATE INDEX IF NOT EXISTS idx_channel_posts_publisher ON channel_posts(publisher_onion);",
                )
                .map_err(|e| {
                    ChattorError::Database(format!("Failed to add publisher columns: {}", e))
                })?;
            }

            conn.execute("UPDATE schema_version SET version = 11", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v11 complete");
        }

        Ok(())
    }
```

Pre-existing remote posts keep NULL publisher and won't appear in per-publisher feeds — acceptable: they were unreachable before this change anyway.

- [ ] **Step 4: Update store_channel_post + add the new query**

`src/db/queries/channels.rs`:

```rust
pub fn store_channel_post(
    db: &Database,
    channel_id: i64,
    content: &str,
    post_id: &str,
    created_at: i64,
    signature: &str,
    publisher_onion: Option<&str>,
    channel_type: Option<&str>,
) -> Result<()> {
    db.connection().execute(
        "INSERT OR IGNORE INTO channel_posts
            (channel_id, content, post_id, created_at, signature, publisher_onion, channel_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![channel_id, content, post_id, created_at, signature, publisher_onion, channel_type],
    ).map_err(|e| ChattorError::Database(format!("Failed to store channel post: {}", e)))?;
    Ok(())
}

/// Posts from one remote publisher, newest first (subscription feed view).
pub fn get_publisher_channel_posts(
    db: &Database,
    publisher_onion: &str,
    limit: usize,
) -> Result<Vec<ChannelPost>> {
    let conn = db.connection();
    let mut stmt = conn
        .prepare(
            "SELECT id, channel_id, content, post_id, created_at, signature
         FROM channel_posts
         WHERE channel_id = 0 AND publisher_onion = ?1
         ORDER BY created_at DESC, id DESC
         LIMIT ?2",
        )
        .map_err(|e| {
            ChattorError::Database(format!("Failed to prepare publisher posts query: {}", e))
        })?;

    let posts = stmt
        .query_map(params![publisher_onion, limit as i64], |row| {
            Ok(ChannelPost {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                content: row.get(2)?,
                post_id: row.get(3)?,
                created_at: row.get(4)?,
                signature: row.get(5)?,
            })
        })
        .map_err(|e| ChattorError::Database(format!("Failed to query publisher posts: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect publisher posts: {}", e)))?;

    Ok(posts)
}
```

The `ChannelPost` struct is unchanged (we filter in SQL; nothing reads the new columns back yet).

- [ ] **Step 5: Update all callers**

`src/handlers/messaging.rs`, `Message::ChannelPost` arm (line ~223):

```rust
        protocol::message::Message::ChannelPost(post) => {
            let ct = match post.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };
            // Store remote post (channel_id 0 for remote posts)
            db::queries::store_channel_post(
                &app.db,
                0,
                &post.content,
                &post.post_id.to_string(),
                post.created_at,
                &post.signature,
                Some(&post.publisher_onion),
                Some(ct),
            )?;
```

`Message::ChannelSyncResponse` arm (line ~291) — each `post` in `resp.posts` is also a `ChannelPostMessage`, so the same fields apply:

```rust
        protocol::message::Message::ChannelSyncResponse(resp) => {
            for post in &resp.posts {
                let ct = match post.channel_type {
                    protocol::message::ChannelType::Public => "public",
                    protocol::message::ChannelType::FriendsOnly => "friends_only",
                };
                db::queries::store_channel_post(
                    &app.db,
                    0,
                    &post.content,
                    &post.post_id.to_string(),
                    post.created_at,
                    &post.signature,
                    Some(&post.publisher_onion),
                    Some(ct),
                )?;
```

`src/main.rs:1032` (own publish) and the `store_channel_post` call in `src/daemon/rpc/channels.rs`: append `, None, None` (own posts live in channel_id 1/2; publisher is implicit).

The 5 existing tests in `src/db/queries/channels.rs` that call `store_channel_post`: append `, None, None`.

- [ ] **Step 6: Run the new test and the full suite**

Run: `cargo test test_publisher_channel_posts_filtered 2>&1 | tail -3`
Expected: PASS.
Run: `cargo test 2>&1 | tail -3`
Expected: all pass (migration test asserts version 11).

- [ ] **Step 7: Commit**

```bash
git add src/db/ src/handlers/messaging.rs src/main.rs src/daemon/rpc/channels.rs
git commit -m "feat(db): schema v11 — publisher attribution on channel posts + per-publisher feed query"
```

---

### Task 10: Sidebar selection enum — make channels navigable

This wires up the dead `SelectChannel` path: ↑↓/j/k/Tab now walk friends *and* channels; Enter opens a conversation or a channel feed.

**Files:**
- Modify: `src/ui/state/mod.rs` (SidebarSelection enum, Normal variant, AppAction changes, handle_key signature, tests)
- Modify: `src/ui/state/normal.rs` (navigation rewrite + tests)
- Modify: `src/ui/sidebar.rs` (render selection across both sections)
- Modify: `src/ui/app_ui.rs` (selected mapping)
- Modify: `src/main.rs` (handle_key call, action handlers, typing-indicator pattern, subscription feed load)

- [ ] **Step 1: Write failing enum tests**

In `src/ui/state/mod.rs`, add the enum + tests:

```rust
/// What the sidebar cursor is on. Navigation order:
/// friends[0..n], OwnPublic, OwnFriends, subscriptions[0..m].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSelection {
    Friend(usize),
    OwnPublic,
    OwnFriends,
    Subscription(usize),
}

impl SidebarSelection {
    pub fn first(friend_count: usize) -> Self {
        if friend_count > 0 {
            SidebarSelection::Friend(0)
        } else {
            SidebarSelection::OwnPublic
        }
    }

    pub fn next(self, friend_count: usize, sub_count: usize) -> Self {
        use SidebarSelection::*;
        match self {
            Friend(i) if i + 1 < friend_count => Friend(i + 1),
            Friend(_) => OwnPublic,
            OwnPublic => OwnFriends,
            OwnFriends if sub_count > 0 => Subscription(0),
            OwnFriends => OwnFriends,
            Subscription(i) if i + 1 < sub_count => Subscription(i + 1),
            Subscription(i) => Subscription(i),
        }
    }

    pub fn prev(self, friend_count: usize, _sub_count: usize) -> Self {
        use SidebarSelection::*;
        match self {
            Friend(i) if i > 0 => Friend(i - 1),
            Friend(i) => Friend(i),
            OwnPublic if friend_count > 0 => Friend(friend_count - 1),
            OwnPublic => OwnPublic,
            OwnFriends => OwnPublic,
            Subscription(0) => OwnFriends,
            Subscription(i) => Subscription(i - 1),
        }
    }
}
```

Tests (same file's tests module):

```rust
    #[test]
    fn sidebar_selection_walks_friends_then_channels() {
        use SidebarSelection::*;
        assert_eq!(Friend(0).next(2, 1), Friend(1));
        assert_eq!(Friend(1).next(2, 1), OwnPublic);
        assert_eq!(OwnPublic.next(2, 1), OwnFriends);
        assert_eq!(OwnFriends.next(2, 1), Subscription(0));
        assert_eq!(Subscription(0).next(2, 1), Subscription(0)); // bottom stop
        assert_eq!(OwnFriends.next(2, 0), OwnFriends); // no subs
    }

    #[test]
    fn sidebar_selection_walks_back_up() {
        use SidebarSelection::*;
        assert_eq!(Subscription(0).prev(2, 1), OwnFriends);
        assert_eq!(OwnFriends.prev(2, 1), OwnPublic);
        assert_eq!(OwnPublic.prev(2, 1), Friend(1));
        assert_eq!(Friend(0).prev(2, 1), Friend(0)); // top stop
        assert_eq!(OwnPublic.prev(0, 1), OwnPublic); // no friends
    }

    #[test]
    fn sidebar_selection_first() {
        assert_eq!(SidebarSelection::first(3), SidebarSelection::Friend(0));
        assert_eq!(SidebarSelection::first(0), SidebarSelection::OwnPublic);
    }
```

Run: `cargo test sidebar_selection 2>&1 | tail -3` — expected: 3 passed (enum + tests land together).

- [ ] **Step 2: Change AppState::Normal and AppAction**

In `src/ui/state/mod.rs`:

```rust
    Normal {
        selected: Option<SidebarSelection>,
        conversation_id: Option<i64>,
        input: String,
        cursor: usize,
        input_focused: bool,
        scroll_offset: usize,
    },
```

`Default` impl: `selected: None`.

AppAction changes:
- `ViewOwnChannel` → `ViewOwnChannel(String)` (channel_type)
- Delete `SelectChannel(String, String, bool)` and its `#[allow(dead_code)]`
- Add `SelectSubscription(usize)`

`handle_key` gains a third parameter:

```rust
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        friend_count: usize,
        sub_count: usize,
    ) -> Result<Option<AppAction>> {
```

passed through to `self.handle_normal_key(key, friend_count, sub_count)` (other state handlers don't need it).

Export the enum from `src/ui/mod.rs`:

```rust
pub use state::{AppAction, AppState, SidebarSelection};
```

- [ ] **Step 3: Rewrite navigation in normal.rs**

`handle_normal_key` signature gains `sub_count: usize`. In the nav-mode match, replace the Tab/Up/Down/'k'/'j'/Enter arms (and 'p'):

```rust
                        KeyCode::Char('p') => {
                            Ok(Some(AppAction::ViewOwnChannel("public".to_string())))
                        }
                        KeyCode::Tab => {
                            if selected.is_none() {
                                *selected = Some(SidebarSelection::first(friend_count));
                            }
                            Ok(None)
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if let Some(s) = selected {
                                *s = s.prev(friend_count, sub_count);
                                *scroll_offset = 0;
                            }
                            Ok(None)
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            *selected = Some(match selected {
                                Some(s) => s.next(friend_count, sub_count),
                                None => SidebarSelection::first(friend_count),
                            });
                            *scroll_offset = 0;
                            Ok(None)
                        }
                        KeyCode::Enter => match *selected {
                            Some(SidebarSelection::Friend(idx)) => {
                                *input_focused = true;
                                *scroll_offset = 0;
                                Ok(Some(AppAction::SelectFriend(idx)))
                            }
                            Some(SidebarSelection::OwnPublic) => {
                                Ok(Some(AppAction::ViewOwnChannel("public".to_string())))
                            }
                            Some(SidebarSelection::OwnFriends) => Ok(Some(
                                AppAction::ViewOwnChannel("friends_only".to_string()),
                            )),
                            Some(SidebarSelection::Subscription(idx)) => {
                                Ok(Some(AppAction::SelectSubscription(idx)))
                            }
                            None => Ok(None),
                        },
```

(Bind `selected` in the destructure where `selected_friend_idx` was. Note Up/'k' and Down/'j' merge into single arms. The previous behavior of Down when `selected` is None — do nothing — becomes "select first", which is strictly more discoverable.)

Add `use super::SidebarSelection;` at the top.

- [ ] **Step 4: Update the test call sites mechanically**

Every `handle_key(<key>, <n>)` call in `src/ui/state/*.rs` tests becomes `handle_key(<key>, <n>, 0)`. Every test asserting `selected_friend_idx` asserts `selected` with `Some(SidebarSelection::Friend(i))`/`None` instead, and every `AppState::Normal { selected_friend_idx: ... }` construction uses `selected: ...`. Then add new behavior tests in `src/ui/state/normal.rs`:

```rust
    #[test]
    fn nav_continues_past_friends_into_channels() {
        let mut state = AppState::Normal {
            selected: Some(SidebarSelection::Friend(1)),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        // 2 friends, 1 subscription: Friend(1) -> OwnPublic
        state
            .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 2, 1)
            .unwrap();
        match &state {
            AppState::Normal { selected, .. } => {
                assert_eq!(*selected, Some(SidebarSelection::OwnPublic))
            }
            _ => panic!("Expected Normal"),
        }
    }

    #[test]
    fn enter_on_own_public_channel_opens_it() {
        let mut state = AppState::Normal {
            selected: Some(SidebarSelection::OwnPublic),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 0, 0)
            .unwrap();
        assert_eq!(action, Some(AppAction::ViewOwnChannel("public".to_string())));
    }

    #[test]
    fn enter_on_subscription_selects_it() {
        let mut state = AppState::Normal {
            selected: Some(SidebarSelection::Subscription(0)),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 0, 1)
            .unwrap();
        assert_eq!(action, Some(AppAction::SelectSubscription(0)));
    }
```

- [ ] **Step 5: Render the selection in the sidebar**

`src/ui/sidebar.rs`: change both render fns to take `selected: Option<crate::ui::SidebarSelection>` instead of `selected_idx: Option<usize>`.

- `render_friends_list`: `let is_selected = selected == Some(crate::ui::SidebarSelection::Friend(i));`
- `render_channels_section` gains the `selected` param and renders cursors. Replace the static items with:

```rust
fn render_channels_section(
    f: &mut Frame,
    area: Rect,
    subscriptions: &[ChannelSubscription],
    selected: Option<crate::ui::SidebarSelection>,
    theme: &Theme,
) {
    use crate::ui::SidebarSelection as Sel;

    let mut items: Vec<ListItem> = Vec::new();

    let entry = |label: String, is_selected: bool| {
        let arrow = if is_selected { "\u{25b8} " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(theme.sidebar_selected_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };
        ListItem::new(Line::from(vec![Span::styled(
            format!("{}{}", arrow, label),
            style,
        )]))
    };

    items.push(ListItem::new(Line::from(Span::styled(
        "  My Channels",
        Style::default()
            .fg(theme.sidebar_channel_header)
            .add_modifier(Modifier::BOLD),
    ))));
    items.push(entry("  Public".to_string(), selected == Some(Sel::OwnPublic)));
    items.push(entry("  Friends".to_string(), selected == Some(Sel::OwnFriends)));

    if !subscriptions.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "  Subscriptions",
            Style::default()
                .fg(theme.sidebar_channel_header)
                .add_modifier(Modifier::BOLD),
        ))));

        for (i, sub) in subscriptions.iter().enumerate() {
            let name = crate::ui::input::truncate_display_dots(&sub.publisher_onion, 8);
            let ch_label = if sub.channel_type == "public" { "pub" } else { "fri" };
            items.push(entry(
                format!("  {} [{}]", name, ch_label),
                selected == Some(Sel::Subscription(i)),
            ));
        }
    }

    let list = List::new(items).block(
        Block::default()
            .title(" Channels ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(list, area);
}
```

`render_sidebar_with_channels` passes `selected` to both children. The deprecated `render_sidebar` wrapper updates its param type the same way.

- [ ] **Step 6: Update app_ui.rs**

In the Task 8 `render_main_area`, the param becomes `selected: Option<crate::ui::SidebarSelection>`; sidebar gets `selected` directly; the conversation's friend lookup becomes:

```rust
    let selected_friend = match selected {
        Some(crate::ui::SidebarSelection::Friend(i)) => ctx.friends.get(i),
        _ => None,
    };
```

The Normal destructure in `render_app` binds `selected` instead of `selected_friend_idx`. Update the Task 8 test construction (`selected: None`).

- [ ] **Step 7: Update main.rs**

- `handle_key` call (line ~664): `app_state.handle_key(key, cached_friend_count, cached_channel_subs.len())?`
- `ViewOwnChannel` handler:

```rust
                        Some(AppAction::ViewOwnChannel(channel_type)) => {
                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                            drop(app_lock);
                            app_state = AppState::ViewingChannel {
                                publisher_onion: own_onion,
                                channel_type,
                                is_own: true,
                                input: String::new(),
                                cursor: 0,
                                scroll_offset: 0,
                            };
                        }
```

- Replace the `SelectChannel` arm with:

```rust
                        Some(AppAction::SelectSubscription(idx)) => {
                            if let Some(sub) = cached_channel_subs.get(idx) {
                                app_state = AppState::ViewingChannel {
                                    publisher_onion: sub.publisher_onion.clone(),
                                    channel_type: sub.channel_type.clone(),
                                    is_own: false,
                                    input: String::new(),
                                    cursor: 0,
                                    scroll_offset: 0,
                                };
                            }
                        }
```

- Subscription feed data load (line ~590): the `ViewingChannel` cache block binds `publisher_onion` too and uses the new query for remote channels:

```rust
            let (channel_posts, channel_post_read_counts) = if let AppState::ViewingChannel {
                ref publisher_onion,
                ref channel_type,
                is_own,
                ..
            } = &app_state
            {
                let (posts, mut counts);
                if *is_own {
                    let channel_id = if channel_type == "public" { 1 } else { 2 };
                    posts = db::queries::get_channel_posts(&app_lock.db, channel_id, 100)
                        .unwrap_or_default();
                    counts = std::collections::HashMap::new();
                    if !posts.is_empty() {
                        let post_ids: Vec<&str> =
                            posts.iter().map(|p| p.post_id.as_str()).collect();
                        counts = db::queries::get_channel_post_read_counts_batch(
                            &app_lock.db,
                            &post_ids,
                        )
                        .unwrap_or_default();
                    }
                } else {
                    posts = db::queries::get_publisher_channel_posts(
                        &app_lock.db,
                        publisher_onion,
                        100,
                    )
                    .unwrap_or_default();
                    counts = std::collections::HashMap::new();
                }
                (posts, counts)
            } else {
                (Vec::new(), std::collections::HashMap::new())
            };
```

- Typing-indicator pattern (line ~1173): `selected_friend_idx: Some(idx)` becomes `selected: Some(crate::ui::SidebarSelection::Friend(idx))` (and the variant field rename).

- [ ] **Step 8: Full build, test, clippy**

Run: `cargo test 2>&1 | tail -3 && cargo clippy -- -D warnings 2>&1 | tail -3`
Expected: all pass, no warnings. The compiler is the safety net here — chase every `selected_friend_idx` / `ViewOwnChannel` / `SelectChannel` error it reports.

- [ ] **Step 9: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat(ui): sidebar navigation spans channels — subscribed feeds finally reachable"
```

---

### Task 11: Final verification + docs

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Full gate**

Run, in order:
```bash
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
cargo test --test integration
cargo test --test e2e_messaging
```
Expected: all clean/passing. Fix anything that isn't before proceeding.

- [ ] **Step 2: Manual smoke (no Tor needed)**

Run: `cargo run` in a small terminal (resize below 60×16) → "Terminal too small" guard appears; resize up → app renders. Press `a` → type → cursor visible, arrow keys work; paste long text → input scrolls horizontally with cursor visible. `q` to quit.

- [ ] **Step 3: Update CLAUDE.md**

- Schema references: v9 → v11 (note v10 = TOFU pubkey column, v11 = channel post publisher attribution). Database inspect hint "should be 9" → "should be 11".
- Section 8 (UI Layer): add `src/ui/widgets/` — ModalFrame (min-size-clamped modals), TextInput (tui-textarea-backed inputs), ScrollView (wrap-aware scrolling + scrollbars); note sidebar navigation now spans friends + channels and subscribed channel feeds are opened with Enter.
- Key Files list: add `src/ui/widgets/scroll_view.rs`.
- "Future Work": remove "TOFU continuity checking" if v10 shipped it (verify with `git log --oneline | head -20` first; leave it if only the column exists).

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for schema v11 and TUI widget layer"
```

---

## Self-review checklist (run after writing, before execution)

- Spec coverage (Tier 1): subscribed-channel access → Tasks 9+10; wrap-aware scrolling → Tasks 7+8; modal input rendering → Tasks 4–6; small-terminal safety → Task 2. ✓
- Out of scope by design: composer migration (Tier 3), help overlay/search/toasts (Tier 2).
- Type consistency: `TextInput::single_line/with_text/handle_key/text/is_empty/render` used identically in Tasks 4, 5, 6; `render_scrollable(f, area, lines, offset, theme) -> usize` identical in Tasks 7, 8; `SidebarSelection::{first,next,prev}` identical in Task 10 steps; `store_channel_post(..., publisher_onion, channel_type)` identical in Task 9 steps 1, 4, 5.
