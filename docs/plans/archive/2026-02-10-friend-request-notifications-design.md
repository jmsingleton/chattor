# Friend Request Notifications Design

**Goal:** Let users discover and act on incoming friend requests in the TUI.

**Approach:** Sidebar badge + hotkey to open a scrollable list, drill into existing modal.

## Sidebar Badge

The sidebar header shows pending count when > 0: `"Friends (2 new)"` in yellow.
Count comes from `get_pending_request_count()` DB query each render cycle.
When count is 0, header shows just `"Friends"`.

## Friend Request List View

New `AppState::ViewingFriendRequests` state, triggered by `F` in Normal state (only when pending > 0).

- Full-screen modal overlay, yellow border
- Title: `"Friend Requests (N pending)"`
- Each item: friend code + truncated onion + relative time
- Arrow key navigation with highlighted selection
- Footer: `[Enter] View  [Esc] Back`

## State Transitions

```
Normal → [F] → ViewingFriendRequests (list)
  List → [Enter] → ViewingFriendRequest (existing modal)
    Modal → [A] Accept → back to list
    Modal → [R] Reject → back to list
    Modal → [Esc] → back to list
  List → [Esc] → Normal
  List auto-closes when last request handled
```

## DB Queries

- `get_pending_request_count(db) -> i64`
- `get_pending_friend_requests(db) -> Vec<PendingFriendRequest>`

## Files

- `src/ui/state.rs` — new state + key handling
- `src/ui/modals.rs` — new list renderer
- `src/ui/sidebar.rs` — badge in header
- `src/ui/app_ui.rs` — wire up new state rendering
- `src/db/queries.rs` — two new query functions

## Not Building

- Auto-popup notifications
- Sound/bell alerts
- Request expiration
- Changes to accept/reject logic (already works)
