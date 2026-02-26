# Performance & UX Hardening Design

**Date:** 2026-02-25
**Status:** Approved
**Ordering:** Stability first, then performance, then UX polish

## Overview

Deep review of chattor identified 4 critical performance issues, 4 critical UX issues (crash/silent-failure), and ~30 medium/low improvements. This design organizes all findings into 4 sequential work tracts ordered by risk: crash prevention first, then architectural performance, then input/navigation, then visual polish.

---

## Tract 1: Crash Prevention & Error Surfacing

**Goal:** Eliminate every panic path and make all errors visible to the user.

### Crash Bugs

1. **UTF-8 cursor tracking in text input** (`src/ui/state.rs:123-124`)
   - `cursor` tracks byte offset; `String::insert(*cursor, c)` panics on non-char-boundary after multi-byte input
   - Fix: track char index, convert to byte offset via `char_indices()` only when slicing

2. **UTF-8 byte slicing in sidebar name truncation** (`src/ui/sidebar.rs:87-91`)
   - `&name[..14]` panics if byte 14 is mid-character
   - Fix: use `name.chars().take(14).collect::<String>()` or `floor_char_boundary()`
   - Same pattern in: `modals.rs:79,180`, `app_ui.rs:44`, `channel_feed.rs:28`, `db/queries.rs:23`

3. **Friend list Down arrow unbounded** (`src/ui/state.rs:195-199`)
   - `*idx += 1` with no upper bound check; selection goes past end of list
   - Fix: pass friend count to `handle_key()` or check in caller

4. **`.expect()` on PreKey OPK decode** (`src/main.rs:1137-1138`)
   - Panics on corrupted database data
   - Fix: replace with `?` propagation, surface error to user

### Error Surfacing

5. **Wire up `format_error_for_user()`** (`src/ui/error.rs`)
   - Currently `#[allow(dead_code)]`, only used in tests
   - Add a status bar / flash message system for transient errors

6. **Replace 18 `eprintln!` calls** in `src/main.rs`
   - All invisible in TUI raw mode
   - Route through the flash message system from item 5

7. **Encryption/send failure feedback** (`src/main.rs:578-580`)
   - User's message disappears, silently marked "failed" in DB
   - Show flash: "Message failed to send — no encryption session"

8. **Friend request accept/reject feedback**
   - Silent success; show "Friend request accepted" / "rejected" flash

9. **Identity modal copy guard** (`src/main.rs:444-451`)
   - Disable `[o]`/`[c]` copy keybindings when displaying "(Waiting for Tor...)"

---

## Tract 2: Render Loop & Mutex Architecture

**Goal:** Fix the critical performance bottlenecks. The app currently runs ~1000 unnecessary DB queries/sec when idle.

### Mutex Restructuring

1. **Extract `ConnectionPool` from `App`**
   - Make it `Arc<ConnectionPool>` at top level, passed independently to background tasks
   - Heartbeat, queue processor, and incoming message handler no longer need the app mutex for sends

2. **Heartbeat: concurrent sends** (`src/main.rs:228-257`)
   - Currently sequential, holds app lock across `pool.send().await` (up to 30s/peer)
   - Fix: collect peers + build messages under lock, drop lock, send concurrently via JoinSet

3. **Queue processor: same pattern** (`src/main.rs:770-785`)
   - Lock to read queue state, unlock before sending

### Render Loop Efficiency

4. **Dirty flag** (`src/main.rs:273-321`)
   - Only re-query DB when an event triggers a change (key press, incoming message, timer tick)
   - Use a `tokio::sync::Notify` or channel from background tasks to signal changes

5. **Batch channel post read counts** (`src/main.rs:308-313`)
   - Replace N+1 `get_channel_post_read_count()` calls with single `GROUP BY post_id` query
   - Add `get_channel_post_read_counts_batch(db, &[post_id])` to `db/queries.rs`

6. **Throttle `cleanup_expired_messages`** (`src/main.rs:284`)
   - Currently runs every 100ms; move to once per 30s via simple `Instant` check

7. **Cache Theme** (`src/main.rs:335`)
   - Wrap in `Arc<Theme>`, pass by reference to `RenderContext` instead of cloning per frame

8. **Consider `tokio::select!`** for event-driven wakeup instead of `event::poll(100ms)`

### Database Tuning

9. **Enable WAL mode + pragmas** (`src/db/connection.rs`)
   - `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA cache_size=-8000;`
   - Reduces write contention, improves read concurrency

10. **Fix ConnectionPool DashMap lock duration** (`src/net/pool.rs:70-87`)
    - `get_mut()` holds shard write lock during entire send (up to 10s)
    - Fix: clone/extract the connection, release the DashMap lock, then send

---

## Tract 3: Input & Navigation Polish

**Goal:** Bring text editing and navigation up to terminal-native expectations.

### Text Input

1. **Standard editing keybindings** (`src/ui/state.rs:107-147`)
   - `Home` / `Ctrl+A` — move cursor to start
   - `End` / `Ctrl+E` — move cursor to end
   - `Delete` — forward delete
   - `Ctrl+W` — delete word backward
   - `Ctrl+U` — delete to start of line
   - `Ctrl+V` — paste from clipboard

2. **Consistent cursor rendering**
   - Modals use `_` cursor (`modals.rs:44`); conversation uses block char `\u{2588}`
   - Standardize on block char everywhere

### Navigation

3. **Message scrolling** — wire up `scroll_offset` with PageUp/PageDown (`state.rs`)

4. **Vim-style j/k** — alongside arrow keys for up/down navigation

5. **Channel sidebar navigation** — make channels section selectable, navigate with arrows
   - Wire up `SelectChannel` action (currently `#[allow(dead_code)]`)

6. **Sidebar focus return** — Esc from input cleanly returns focus to sidebar

7. **Empty state feedback** — `[f]` with no pending requests shows "No pending requests" flash

8. **Help screen** — `[?]` opens a full keybinding reference overlay

---

## Tract 4: Visual & Feedback Polish

**Goal:** Improve status feedback, message display, and accessibility.

### Status Feedback

1. **Tor bootstrap progress** (`src/ui/bootstrap.rs:219`)
   - `_progress: u8` parameter exists but is unused; display as percentage bar

2. **"Offline" header state** (`src/ui/app_ui.rs:49-53`)
   - After "Continue Offline", show "Offline" instead of "Connecting..."

3. **Bootstrap skip option** — allow skipping during active connecting, not just after failure

4. **Notification state indicator** — persistent icon in header showing notifications on/off

5. **Ephemeral modal current setting** (`src/ui/modals.rs:303-360`)
   - Highlight the currently active TTL option instead of always starting at index 0

### Message Display

6. **Date separators** (`src/ui/conversation.rs:131-185`)
   - Insert "--- January 15, 2026 ---" lines between messages from different days

7. **Absolute timestamps** for messages older than 24h (replace "45d ago" with date+time)

8. **Typing indicator spacing** (`src/ui/conversation.rs:103-116`)
   - Reserve a line for the typing indicator instead of overlapping last message

9. **Input horizontal scrolling** (`src/ui/conversation.rs:188-224`)
   - When input exceeds visible width, scroll so cursor stays visible

10. **Sanitize control characters** in message content before rendering

### Accessibility

11. **Fix cyberpunk theme contrast** (`src/ui/theme.rs:161-194`)
    - `fg_dim: #005500` on `#000000` is ~1.7:1 contrast ratio; minimum should be 4.5:1

12. **Apply `theme.bg`** to widget backgrounds (currently unused)

13. **Delivered vs Read receipt differentiation** — add text or icon difference, not just color

14. **Quit confirmation** — warn if input field has unsent text

---

## Dependencies Between Tracts

- Tract 1 is fully independent — pure bug fixes
- Tract 2 is independent of Tract 1 (different files/concerns)
- Tract 3 depends on Tract 1's UTF-8 cursor fix (builds on the same code)
- Tract 4 is independent of all others

Tracts 1 and 2 can run in parallel. Tract 3 should follow Tract 1. Tract 4 can run anytime.

## Testing Strategy

- **Tract 1:** Add unit tests for UTF-8 input (emoji, CJK), bounds checking, error display
- **Tract 2:** Benchmark render loop before/after; test dirty flag triggers
- **Tract 3:** Manual TUI testing for keybindings; unit tests for cursor movement logic
- **Tract 4:** Visual inspection across all 7 themes; contrast ratio verification
