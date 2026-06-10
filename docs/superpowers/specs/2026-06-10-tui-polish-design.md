# TUI Polish & UX Overhaul — Design

**Date:** 2026-06-10
**Status:** Approved
**Scope:** All three tiers (broken-UX fixes, core interactions, visual polish)

## Background

A UX review of the TUI (all of `src/ui/`, the `AppState` state machine, and key
handling) found a solid foundation — theming, rounded borders, presence icons,
date separators — but a rudimentary interaction layer with several outright
bugs:

1. **Subscribed channels are unreachable.** `AppAction::SelectChannel` is dead
   code (`src/ui/state/mod.rs`); the sidebar renders subscriptions but no
   keybinding opens them. Users can subscribe but never read posts.
2. **Scroll math ignores line wrapping.** `conversation.rs` and
   `channel_feed.rs` skip lines before `Wrap` is applied, so long messages make
   scrolling miscount and clip the newest content.
3. **Modal text inputs can't show long input.** Inputs render `{input}_` with
   no wrap and no cursor-position rendering; pasting a 32-word friend code (the
   primary onboarding flow) shows only a clipped prefix.
4. **Modals break on small terminals.** `centered_rect` is percentage-only with
   no minimum dimensions.
5. Missing standard UX: help overlay, message search (FTS5 backend is fully
   built and unused), friend rename/remove/block (`blocked_onions` table exists
   with no UI), unread divider, scrollbars, mouse wheel, visible async errors.

## Decisions (made with user)

| Decision | Choice |
|---|---|
| Scope | All three tiers |
| Conversation layout | Aligned bubbles: own messages right-aligned, peer left-aligned with accent bar |
| Mouse | Wheel scroll only (no click handling; capture trade-off accepted) |
| Search | Global, across all conversations, jump-to-message on Enter |
| Implementation approach | Hybrid: `tui-textarea` crate for text inputs; hand-rolled ScrollView + ModalFrame; ratatui built-in `Scrollbar` |

## 1. Foundation widgets (`src/ui/widgets/`)

Built first; everything else sits on them.

### `TextInput`
Themed wrapper around the `tui-textarea` crate (new dependency, MIT,
render-only — never touches network/crypto paths).
- Single-line mode (modals, search) and multi-line mode (conversation composer).
- Placeholder text, focus styling, proper cursor rendering, unicode-width
  correctness, undo/redo for free.
- Replaces all 5 hand-rolled input sites: conversation input, channel composer,
  add-friend modal, subscribe modal, and the new search box.
- Known limitation: tui-textarea does not soft-wrap; long single lines scroll
  horizontally. Accepted for inputs. Multi-line composer growth (Tier 3) uses
  its native multi-line support.

### `ScrollView`
Hand-rolled wrap-aware scrollback for chat logs and feeds.
- Pre-computes wrapped line heights for the actual pane width before applying
  scroll offset (fixes the clipping bug at the root).
- Anchors to bottom by default; scroll position tracked in display lines.
- Renders ratatui's built-in `Scrollbar` when content overflows.
- Shows a `↓ N new` jump-to-bottom pill when new messages arrive while the user
  is scrolled up; any jump key (`End`) or reaching bottom clears it.

### `ModalFrame`
Replaces `centered_rect` everywhere.
- Percentage sizing with min/max clamps in absolute cells.
- If the terminal can't fit the minimum, the modal takes the full screen
  instead of collapsing.
- Consistent title bar and bottom controls-hint row.
- Global guard: below 60×16, render a "Terminal too small (min 60×16)" screen
  instead of the app.

## 2. Tier 1 — fix broken UX

- **Unified sidebar navigation.** Sidebar selection becomes one enum:
  `SidebarSelection::{Friend(usize), OwnChannel(ChannelKind), Subscription(usize)}`
  replacing `selected_friend_idx: Option<usize>` in `AppState::Normal`.
  `↑↓`/`j`/`k`/`Tab` walk through friends and channels; `Enter` opens a
  conversation or channel feed. Wires up the dead `SelectChannel` action;
  subscribed channels become reachable for the first time.
- Conversation and channel feed move onto `ScrollView`.
- All modal inputs move onto `TextInput` (friend codes wrap and edit correctly).
- All modals move onto `ModalFrame`.

## 3. Tier 2 — core interactions

- **Help overlay.** `[?]` opens a keymap modal grouped by context
  (Navigate / Conversation / Modals). Footer keeps only the most common hints
  plus `[?] help`.
- **Global search.** `[/]` opens a search modal: live FTS5 results as you type,
  each showing friend name, snippet, and time. `↑↓` + `Enter` jumps to that
  conversation with the matched message centered and highlighted.
  New queries: FTS search across conversations; load messages around a
  timestamp (for jump-to-message).
- **Friend management.** With a friend selected in nav mode:
  - `r` — rename (set alias; stored on the `friends` row).
  - `d` — remove friend.
  - `b` — block (insert into existing `blocked_onions`, remove friendship).
  - Remove/block go through a new shared **confirm modal** ("Block Alice? [y/N]").
- **Unread divider.** `── new messages ──` line above the first unread message
  when opening a conversation; cleared on next open.
- **Mouse wheel.** Enable crossterm mouse capture; wheel scrolls the pane under
  the cursor (conversation, feed, sidebar, search results). No click handling.
  Trade-off accepted: terminal-native text selection requires Shift+drag.
- **Toasts.** Replace the single 2s `status_flash` slot with a stack of up to 3
  toasts (bottom-right overlay, ~4s each, success/info/error styling). Async
  failures — send retries exhausted, subscribe failed, Tor connection dropped —
  route here via the existing event channel to the main loop.

## 4. Tier 3 — visual polish

- **Bubble conversation layout.** Peer messages left-aligned with a `▌` accent
  bar; own messages right-aligned; max bubble width ~70% of the pane; absolute
  `HH:MM` timestamps; status glyphs (⏳ queued, ✓ sent, ✓✓ delivered, ✓✓ read
  in accent color, ✗ failed) on own messages. No background fills — gutter bars
  and alignment only (bg colors render badly across terminals). Date separators
  show real dates: "Today", "Yesterday", else "May 21" (with year if not
  current).
- **Sidebar upgrade.** Width 24 → 28. Friends sorted by last activity.
  Two-line entries: line 1 = status icon + name + last-activity time; line 2 =
  dimmed last-message preview + unread badge.
- **Slim header.** 3 bordered rows → 1 borderless row:
  `chattor  @abc123…  ◉ Connected                    [?] help`.
- **Growing composer.** Conversation input grows 1 → 5 lines as newlines are
  added (`Alt+Enter` inserts a newline; `Enter` sends). Long single lines
  scroll horizontally (tui-textarea has no soft wrap).
- **Selection highlight.** Full-row background highlight for selected list
  items. New `Theme` fields: `selection_bg`, toast colors, scrollbar color —
  added to all 7 presets and the TOML override schema.
- Typing indicator stays on a dedicated line directly above the composer.

## 5. State machine & architecture

- New `AppState` variants: `Searching`, `HelpOverlay`,
  `ConfirmingAction { action, target }`, `RenamingFriend`.
- Existing pattern unchanged: state handles keys → emits `AppAction` → main
  loop executes. New actions: `OpenSearch`, `JumpToMessage`, `RenameFriend`,
  `RemoveFriend`, `BlockFriend`, `ToggleHelp`.
- DB changes are additive and small: alias update on `friends`, two new
  queries (FTS search, load-around-timestamp). `blocked_onions` already exists.
- `RenderContext` gains: toast stack, unread-divider position, search results,
  sidebar preview data (last message + time per friend).

## 6. Error handling & testing

- UI render paths stay panic-free; no unwraps on data.
- Async task failures route to toasts; nothing fails silently.
- Tests follow the existing pattern:
  - State-machine unit tests for every new state and keybinding.
  - `TestBackend` render tests for ScrollView line math (long wrapped
    messages), bubble layout, and modals at tiny terminal sizes — the
    previously broken cases.
  - Query tests for FTS search and load-around-timestamp.

## Delivery

Three PRs, one per tier; foundation widgets land inside the Tier 1 PR. Each PR
keeps `cargo test` green and `cargo clippy -- -D warnings` clean.
