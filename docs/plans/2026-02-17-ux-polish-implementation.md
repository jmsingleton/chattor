# UX Polish Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add typing indicators, online status, and desktop notifications via a unified presence system.

**Architecture:** A single `Presence` protocol message handles heartbeats and typing. In-memory `HashMap<String, PeerPresence>` tracks per-friend state. Presence is shared between the main loop and background tasks via `Arc<Mutex<>>`. Desktop notifications use `notify-rust` with a global toggle in `app_settings`.

**Tech Stack:** Rust, ratatui, tokio, serde, notify-rust (new dependency)

---

### Task 1: Add Presence Protocol Types

**Files:**
- Modify: `src/protocol/message.rs`

**Step 1: Add PresenceType enum and PresenceMessage struct after the existing types**

Add before the `#[cfg(test)]` block at the bottom of the file:

```rust
/// Type of presence update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PresenceType {
    Heartbeat,
    TypingStarted,
    TypingStopped,
}

/// Lightweight presence message (not encrypted — Tor provides transport security)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresenceMessage {
    pub from_onion: String,
    pub presence_type: PresenceType,
    pub timestamp: i64,
}
```

**Step 2: Add Presence variant to Message enum**

Add after the `ChannelPostReceipt` variant in the `Message` enum:

```rust
    #[serde(rename = "presence")]
    Presence(PresenceMessage),
```

**Step 3: Write serialization tests**

Add to the existing `#[cfg(test)]` block in `src/protocol/message.rs`:

```rust
    #[test]
    fn test_presence_message_serialization() {
        let msg = Message::Presence(PresenceMessage {
            from_onion: "test.onion".to_string(),
            presence_type: PresenceType::Heartbeat,
            timestamp: 1000,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_presence_typing_roundtrip() {
        for pt in [PresenceType::Heartbeat, PresenceType::TypingStarted, PresenceType::TypingStopped] {
            let msg = Message::Presence(PresenceMessage {
                from_onion: "peer.onion".to_string(),
                presence_type: pt.clone(),
                timestamp: 42,
            });
            let bytes = serde_json::to_vec(&msg).unwrap();
            let decoded: Message = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(msg, decoded);
        }
    }
```

**Step 4: Run tests to verify**

Run: `cargo test protocol::message`
Expected: All tests pass including the two new ones.

**Step 5: Commit**

```bash
git add src/protocol/message.rs
git commit -m "feat: add Presence protocol message type for typing and online status"
```

---

### Task 2: Add PeerPresence State and PresenceTracker

**Files:**
- Create: `src/presence.rs`
- Modify: `src/lib.rs` (add `pub mod presence;`)
- Modify: `src/main.rs` (add `mod presence;`)

**Step 1: Write tests for PeerPresence timeout logic**

Create `src/presence.rs` with tests first:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// How often to send heartbeats to peers with active connections
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);

/// Mark peer offline after this duration without a heartbeat
pub const OFFLINE_THRESHOLD: Duration = Duration::from_secs(120);

/// Typing indicator expires after this duration
pub const TYPING_TIMEOUT: Duration = Duration::from_secs(5);

/// Minimum interval between outgoing TypingStarted messages
pub const TYPING_DEBOUNCE: Duration = Duration::from_secs(4);

/// Per-peer presence state (in-memory only, never persisted)
#[derive(Debug, Clone)]
pub struct PeerPresence {
    pub last_seen: Instant,
    pub typing_started: Option<Instant>,
}

impl PeerPresence {
    pub fn new() -> Self {
        PeerPresence {
            last_seen: Instant::now(),
            typing_started: None,
        }
    }

    /// Whether this peer should be considered online
    pub fn is_online(&self) -> bool {
        self.last_seen.elapsed() < OFFLINE_THRESHOLD
    }

    /// Whether this peer is currently typing
    pub fn is_typing(&self) -> bool {
        self.typing_started
            .map(|t| t.elapsed() < TYPING_TIMEOUT)
            .unwrap_or(false)
    }
}

/// Thread-safe presence tracker shared between main loop and background tasks
pub type PresenceMap = Arc<Mutex<HashMap<String, PeerPresence>>>;

/// Create a new empty presence map
pub fn new_presence_map() -> PresenceMap {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Record a heartbeat from a peer
pub async fn record_heartbeat(map: &PresenceMap, onion: &str) {
    let mut m = map.lock().await;
    let entry = m.entry(onion.to_string()).or_insert_with(PeerPresence::new);
    entry.last_seen = Instant::now();
}

/// Record typing started from a peer
pub async fn record_typing_started(map: &PresenceMap, onion: &str) {
    let mut m = map.lock().await;
    let entry = m.entry(onion.to_string()).or_insert_with(PeerPresence::new);
    entry.last_seen = Instant::now();
    entry.typing_started = Some(Instant::now());
}

/// Record typing stopped from a peer
pub async fn record_typing_stopped(map: &PresenceMap, onion: &str) {
    let mut m = map.lock().await;
    if let Some(entry) = m.get_mut(onion) {
        entry.typing_started = None;
    }
}

/// Get a snapshot of online/typing status for all peers
pub async fn get_presence_snapshot(map: &PresenceMap) -> HashMap<String, (bool, bool)> {
    let m = map.lock().await;
    m.iter()
        .map(|(k, v)| (k.clone(), (v.is_online(), v.is_typing())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_presence_initially_online() {
        let p = PeerPresence::new();
        assert!(p.is_online());
        assert!(!p.is_typing());
    }

    #[test]
    fn test_peer_presence_not_typing_by_default() {
        let p = PeerPresence::new();
        assert!(!p.is_typing());
    }

    #[test]
    fn test_peer_presence_typing_active() {
        let mut p = PeerPresence::new();
        p.typing_started = Some(Instant::now());
        assert!(p.is_typing());
    }

    #[test]
    fn test_peer_presence_typing_expired() {
        let mut p = PeerPresence::new();
        p.typing_started = Some(Instant::now() - Duration::from_secs(6));
        assert!(!p.is_typing());
    }

    #[test]
    fn test_peer_presence_offline_after_threshold() {
        let mut p = PeerPresence::new();
        p.last_seen = Instant::now() - Duration::from_secs(121);
        assert!(!p.is_online());
    }

    #[test]
    fn test_peer_presence_online_within_threshold() {
        let mut p = PeerPresence::new();
        p.last_seen = Instant::now() - Duration::from_secs(60);
        assert!(p.is_online());
    }

    #[tokio::test]
    async fn test_record_heartbeat() {
        let map = new_presence_map();
        record_heartbeat(&map, "test.onion").await;
        let snap = get_presence_snapshot(&map).await;
        assert_eq!(snap.get("test.onion"), Some(&(true, false)));
    }

    #[tokio::test]
    async fn test_record_typing() {
        let map = new_presence_map();
        record_typing_started(&map, "test.onion").await;
        let snap = get_presence_snapshot(&map).await;
        assert_eq!(snap.get("test.onion"), Some(&(true, true)));

        record_typing_stopped(&map, "test.onion").await;
        let snap = get_presence_snapshot(&map).await;
        assert_eq!(snap.get("test.onion"), Some(&(true, false)));
    }
}
```

**Step 2: Add module declarations**

In `src/lib.rs`, add `pub mod presence;` after the existing module declarations.

In `src/main.rs`, add `mod presence;` after the existing module declarations (line 10).

**Step 3: Run tests**

Run: `cargo test presence`
Expected: All 8 presence tests pass.

**Step 4: Commit**

```bash
git add src/presence.rs src/lib.rs src/main.rs
git commit -m "feat: add PeerPresence state tracker with timeout logic"
```

---

### Task 3: Add `connected_peers()` to ConnectionPool

**Files:**
- Modify: `src/net/pool.rs`

**Step 1: Add connected_peers method to ConnectionPool**

Add after the `evict()` method (after line 116):

```rust
    /// Get a list of peer onion addresses with active (non-idle) connections.
    /// Used by the heartbeat task to know who to send presence updates to.
    pub async fn connected_peers(&self) -> Vec<String> {
        let conns = self.connections.lock().await;
        conns.iter()
            .filter(|(_, pc)| pc.last_used.elapsed() < IDLE_TIMEOUT)
            .map(|(k, _)| k.clone())
            .collect()
    }
```

**Step 2: Commit**

```bash
git add src/net/pool.rs
git commit -m "feat: add connected_peers() to ConnectionPool for heartbeat targeting"
```

---

### Task 4: Wire Presence into Main Loop — Incoming Messages

**Files:**
- Modify: `src/main.rs`

This task adds presence state to the main app and handles incoming Presence messages.

**Step 1: Create the presence map in main() and thread it through**

After the `let mut app_state = AppState::default();` line (line 221 in main.rs), add:

```rust
    // Initialize presence tracker (in-memory only)
    let presence_map = presence::new_presence_map();
```

**Step 2: Handle incoming Presence messages**

In the `handle_incoming_message` function, add a new match arm for `Message::Presence`. Add before the closing `}` of the match (before the current line 1118):

The function signature needs to change to accept the presence map. Change:

```rust
fn handle_incoming_message(app: &App, incoming: net::listener::IncomingMessage) -> Result<()> {
```

to:

```rust
async fn handle_incoming_message(app: &App, incoming: net::listener::IncomingMessage, presence: &presence::PresenceMap) -> Result<()> {
```

Add the match arm:

```rust
        protocol::message::Message::Presence(pres) => {
            match pres.presence_type {
                protocol::message::PresenceType::Heartbeat => {
                    presence::record_heartbeat(presence, &pres.from_onion).await;
                }
                protocol::message::PresenceType::TypingStarted => {
                    presence::record_typing_started(presence, &pres.from_onion).await;
                }
                protocol::message::PresenceType::TypingStopped => {
                    presence::record_typing_stopped(presence, &pres.from_onion).await;
                }
            }
        }
```

**Step 3: Update the call site**

The call to `handle_incoming_message` in the main loop (around line 647) needs to pass the presence map and become `.await`:

Change:
```rust
                if let Err(e) = handle_incoming_message(&*app_lock, incoming) {
```
to:
```rust
                if let Err(e) = handle_incoming_message(&*app_lock, incoming, &presence_map).await {
```

**Step 4: Run tests**

Run: `cargo build`
Expected: Compiles successfully.

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: handle incoming Presence messages and update presence tracker"
```

---

### Task 5: Heartbeat Background Task

**Files:**
- Modify: `src/main.rs`

**Step 1: Spawn heartbeat task after Tor init completes**

After the channel sync task spawn (after line 218 in main.rs), add a heartbeat task:

```rust
    // Spawn heartbeat task — sends presence updates to connected peers
    let app_heartbeat = Arc::clone(&app);
    let presence_heartbeat = Arc::clone(&presence_map);
    tokio::spawn(async move {
        // Wait for Tor to initialize before starting heartbeats
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        loop {
            {
                let app_lock = app_heartbeat.lock().await;
                if let Some(ref pool) = app_lock.connection_pool {
                    let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                    let peers = pool.connected_peers().await;
                    drop(app_lock); // release lock before sending

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    for peer in peers {
                        let msg = protocol::message::Message::Presence(
                            protocol::message::PresenceMessage {
                                from_onion: own_onion.clone(),
                                presence_type: protocol::message::PresenceType::Heartbeat,
                                timestamp: now,
                            }
                        );
                        // Best-effort: don't retry or queue heartbeats
                        let app_lock = app_heartbeat.lock().await;
                        if let Some(ref pool) = app_lock.connection_pool {
                            let _ = pool.send(&peer, &msg).await;
                        }
                    }
                }
            }
            tokio::time::sleep(presence::HEARTBEAT_INTERVAL).await;
        }
    });
```

**Step 2: Run build**

Run: `cargo build`
Expected: Compiles successfully.

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: spawn heartbeat background task for online status"
```

---

### Task 6: Outgoing Typing Detection

**Files:**
- Modify: `src/ui/state.rs`
- Modify: `src/main.rs`

**Step 1: Add TypingStarted and TypingStopped actions**

In `src/ui/state.rs`, add two new variants to the `AppAction` enum (after `Quit` at line 81):

```rust
    ToggleNotifications,
    SendPresence(crate::protocol::message::PresenceType),
```

**Step 2: Track last typing sent time in Normal state**

This requires tracking debounce state. The simplest approach: add a field to track when typing was last signaled. But since `AppState` is cloned and `Instant` isn't great for that, instead we'll track it in main.rs alongside the presence map.

In `src/main.rs`, after the `presence_map` creation, add:

```rust
    let mut last_typing_sent: Option<std::time::Instant> = None;
    let mut was_typing = false;
```

**Step 3: Detect typing changes in the main event loop**

After the `handle_key()` call processes an action (around line 300 in the match), add typing detection logic. Add this block right after the `Event::Key(key)` match arm (after the `None => {}` at line 625), still inside the key event handling:

```rust
                    // Typing indicator detection
                    if let AppState::Normal { input_focused: true, input, selected_friend_idx: Some(idx), .. } = &app_state {
                        let is_typing_now = !input.is_empty();
                        let should_send_started = is_typing_now && (!was_typing || last_typing_sent.map_or(true, |t| t.elapsed() >= presence::TYPING_DEBOUNCE));
                        let should_send_stopped = !is_typing_now && was_typing;

                        if should_send_started || should_send_stopped {
                            let app_lock = app.lock().await;
                            let friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
                            if let Some(friend) = friends.get(*idx) {
                                let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64;

                                let presence_type = if should_send_started {
                                    protocol::message::PresenceType::TypingStarted
                                } else {
                                    protocol::message::PresenceType::TypingStopped
                                };

                                let msg = protocol::message::Message::Presence(
                                    protocol::message::PresenceMessage {
                                        from_onion: own_onion,
                                        presence_type,
                                        timestamp: now,
                                    }
                                );

                                // Best-effort send (don't queue typing indicators)
                                if let Some(ref pool) = app_lock.connection_pool {
                                    let _ = pool.send(&friend.onion_address, &msg).await;
                                }
                            }
                            drop(app_lock);

                            if should_send_started {
                                last_typing_sent = Some(std::time::Instant::now());
                            }
                        }
                        was_typing = is_typing_now;
                    }
```

**Step 4: Send TypingStopped on message send**

In the `SendMessage` action handler (around line 434), after the message is sent successfully, add:

```rust
                            was_typing = false;
                            last_typing_sent = None;
```

**Step 5: Run build**

Run: `cargo build`
Expected: Compiles successfully.

**Step 6: Commit**

```bash
git add src/ui/state.rs src/main.rs
git commit -m "feat: detect typing and send presence indicators with debounce"
```

---

### Task 7: Sidebar Dynamic Status Icons

**Files:**
- Modify: `src/ui/sidebar.rs`
- Modify: `src/ui/app_ui.rs`

**Step 1: Add presence data to RenderContext**

In `src/ui/app_ui.rs`, add to the `RenderContext` struct (after the `theme` field at line 23):

```rust
    /// Per-onion presence: (is_online, is_typing)
    pub presence: std::collections::HashMap<String, (bool, bool)>,
```

**Step 2: Build presence snapshot in main loop**

In `src/main.rs`, where the `RenderContext` is constructed (around line 275), add the presence snapshot. Before the `let ctx = RenderContext {` line, add:

```rust
        let presence_snapshot = presence::get_presence_snapshot(&presence_map).await;
```

Then add to the `RenderContext` construction:

```rust
            presence: presence_snapshot,
```

**Step 3: Pass presence to sidebar rendering**

In `src/ui/sidebar.rs`, change the `render_friends_list` function signature to accept presence data. Add a parameter:

```rust
fn render_friends_list(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
    pending_request_count: i64,
    presence: &std::collections::HashMap<String, (bool, bool)>,
    theme: &Theme,
)
```

Update `render_sidebar_with_channels` to accept and pass presence too. Add the same parameter and thread it through to `render_friends_list`.

Update `render_sidebar` (the convenience wrapper) similarly.

**Step 4: Replace hardcoded status icon**

In `render_friends_list`, replace the hardcoded `○` (line 87) with dynamic lookup:

```rust
            let (status_icon, status_color) = match presence.get(&friend.onion_address) {
                Some((_, true)) => ("✎", theme.accent),       // typing
                Some((true, _)) => ("●", theme.sidebar_status_online), // online
                _ => ("○", theme.fg_dim),                      // offline
            };
```

And update the span that uses it (line 93):

```rust
                Span::styled(status_icon, Style::default().fg(status_color)),
```

**Step 5: Update all call sites**

Update the call in `app_ui.rs` `render_app()` (around line 90) to pass `&ctx.presence`.

**Step 6: Run build**

Run: `cargo build`
Expected: Compiles successfully.

**Step 7: Commit**

```bash
git add src/ui/sidebar.rs src/ui/app_ui.rs src/main.rs
git commit -m "feat: dynamic sidebar status icons for online/typing/offline"
```

---

### Task 8: Conversation Typing Indicator

**Files:**
- Modify: `src/ui/conversation.rs`
- Modify: `src/ui/app_ui.rs`

**Step 1: Add typing indicator parameter to render_conversation**

Change the signature of `render_conversation` to include whether the friend is typing:

```rust
pub fn render_conversation(
    f: &mut Frame,
    area: Rect,
    friend: Option<&FriendEntry>,
    messages: &[ChatMessage],
    own_onion: Option<&str>,
    scroll_offset: usize,
    ephemeral_ttl: Option<i64>,
    friend_is_typing: bool,
    theme: &Theme,
)
```

**Step 2: Render "is typing..." indicator**

In the `Some(friend_entry)` branch (line 71), when messages exist, after the `render_messages` call (line 86), add:

```rust
                if friend_is_typing {
                    let typing_text = format!("{} is typing\u{2026}", friend_entry.display());
                    let typing_line = Paragraph::new(typing_text)
                        .style(Style::default().fg(theme.fg_dim));
                    // Render in the last line of the messages area
                    if padded.height > 1 {
                        let typing_area = Rect {
                            x: padded.x,
                            y: padded.y + padded.height - 1,
                            width: padded.width,
                            height: 1,
                        };
                        f.render_widget(typing_line, typing_area);
                    }
                }
```

Also add a similar indicator for the empty messages case (after line 84):

```rust
                if friend_is_typing {
                    let typing_text = format!("{} is typing\u{2026}", friend_entry.display());
                    let typing_line = Paragraph::new(typing_text)
                        .style(Style::default().fg(theme.fg_dim));
                    let typing_area = Rect {
                        x: padded.x,
                        y: padded.y + padded.height.saturating_sub(1),
                        width: padded.width,
                        height: 1,
                    };
                    f.render_widget(typing_line, typing_area);
                }
```

**Step 3: Update the call site in app_ui.rs**

In `render_app()` (around line 110), compute whether the selected friend is typing and pass it:

```rust
        let friend_is_typing = selected_friend
            .map(|f| ctx.presence.get(&f.onion_address).map_or(false, |(_, typing)| *typing))
            .unwrap_or(false);
```

Pass `friend_is_typing` to `render_conversation`:

```rust
        crate::ui::conversation::render_conversation(
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
```

**Step 4: Run build**

Run: `cargo build`
Expected: Compiles successfully.

**Step 5: Commit**

```bash
git add src/ui/conversation.rs src/ui/app_ui.rs
git commit -m "feat: show typing indicator in conversation view"
```

---

### Task 9: Desktop Notifications with notify-rust

**Files:**
- Modify: `Cargo.toml`
- Create: `src/notifications.rs`
- Modify: `src/lib.rs`
- Modify: `src/main.rs`

**Step 1: Add notify-rust dependency**

In `Cargo.toml`, add to the `[dependencies]` section:

```toml
# Desktop notifications
notify-rust = "4"
```

**Step 2: Create notifications module**

Create `src/notifications.rs`:

```rust
use crate::db::Database;

/// Check if notifications are enabled (defaults to true)
pub fn is_enabled(db: &Database) -> bool {
    crate::db::queries::get_app_setting(db, "notifications_enabled")
        .unwrap_or(None)
        .map(|v| v != "false")
        .unwrap_or(true)
}

/// Toggle notifications on/off, returns new state
pub fn toggle(db: &Database) -> bool {
    let current = is_enabled(db);
    let new_state = !current;
    crate::db::queries::set_app_setting(
        db,
        "notifications_enabled",
        if new_state { "true" } else { "false" },
    ).ok();
    new_state
}

/// Send a desktop notification for an incoming message.
/// Does not include message content (privacy-first).
pub fn notify_message(sender_name: &str) {
    if let Err(e) = notify_rust::Notification::new()
        .summary("Chattor")
        .body(&format!("New message from {}", sender_name))
        .icon("mail-unread")
        .timeout(notify_rust::Timeout::Milliseconds(5000))
        .show()
    {
        // Best-effort: log but don't fail
        eprintln!("Desktop notification failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::NamedTempFile;

    fn test_db() -> Database {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap()
    }

    #[test]
    fn test_notifications_enabled_by_default() {
        let db = test_db();
        assert!(is_enabled(&db));
    }

    #[test]
    fn test_toggle_notifications() {
        let db = test_db();
        assert!(is_enabled(&db));
        let new_state = toggle(&db);
        assert!(!new_state);
        assert!(!is_enabled(&db));
        let new_state = toggle(&db);
        assert!(new_state);
        assert!(is_enabled(&db));
    }
}
```

**Step 3: Add module declarations**

In `src/lib.rs`, add `pub mod notifications;`
In `src/main.rs`, add `mod notifications;`

**Step 4: Run tests**

Run: `cargo test notifications`
Expected: Tests pass.

**Step 5: Commit**

```bash
git add Cargo.toml src/notifications.rs src/lib.rs src/main.rs
git commit -m "feat: add notifications module with notify-rust and global toggle"
```

---

### Task 10: Wire Notifications into Incoming Message Handler

**Files:**
- Modify: `src/main.rs`

**Step 1: Send notification on incoming TextMessage**

In `handle_incoming_message`, inside the `TextMessage` handler (around line 981 where the message is stored), after `store_incoming_message_with_ttl`, add notification logic:

```rust
                // Send desktop notification (best-effort)
                if notifications::is_enabled(&app.db) {
                    let sender_name = db::queries::get_friend_display_name(&app.db, from_onion)
                        .unwrap_or_else(|_| from_onion.to_string());
                    notifications::notify_message(&sender_name);
                }
```

**Step 2: Add helper to get friend display name**

In `src/db/queries.rs`, add a helper function:

```rust
/// Get display name for a friend by onion address
pub fn get_friend_display_name(db: &Database, onion: &str) -> Result<String> {
    let conn = db.connection();
    let result = conn.query_row(
        "SELECT COALESCE(display_name, onion_address) FROM friends WHERE onion_address = ?1",
        [onion],
        |row| row.get(0),
    );
    match result {
        Ok(name) => Ok(name),
        Err(_) => Ok(onion.to_string()),
    }
}
```

**Step 3: Run build**

Run: `cargo build`
Expected: Compiles.

**Step 4: Commit**

```bash
git add src/main.rs src/db/queries.rs
git commit -m "feat: send desktop notification on incoming messages"
```

---

### Task 11: Notification Toggle Keybinding

**Files:**
- Modify: `src/ui/state.rs`
- Modify: `src/main.rs`
- Modify: `src/ui/app_ui.rs`

**Step 1: Add `[n]` keybinding in Normal navigation mode**

In `src/ui/state.rs`, in the Normal navigation mode match (around line 145), add before the `Tab` handler:

```rust
                        KeyCode::Char('n') => Ok(Some(AppAction::ToggleNotifications)),
```

**Step 2: Handle the action in main.rs**

In the main event loop, add a handler for `ToggleNotifications` (alongside the other action handlers):

```rust
                        Some(AppAction::ToggleNotifications) => {
                            let app_lock = app.lock().await;
                            let new_state = notifications::toggle(&app_lock.db);
                            drop(app_lock);
                            notification_flash = Some((
                                std::time::Instant::now(),
                                if new_state { "Notifications: ON" } else { "Notifications: OFF" },
                            ));
                        }
```

Add the flash state variable near the other state variables (after `was_typing`):

```rust
    let mut notification_flash: Option<(std::time::Instant, &str)> = None;
```

**Step 3: Show flash message in footer**

In `src/ui/app_ui.rs`, add a `notification_flash` field to `RenderContext`:

```rust
    pub notification_flash: Option<String>,
```

In `render_app`, render the flash in the footer area when present. Replace the footer rendering (around line 129):

```rust
    // Footer
    let footer_spans = if let Some(ref flash) = ctx.notification_flash {
        vec![
            Span::raw("  "),
            Span::styled(flash.as_str(), Style::default().fg(ctx.theme.accent).add_modifier(Modifier::BOLD)),
        ]
    } else {
        format_footer_spans(app_state, &ctx.theme)
    };
    let footer = Paragraph::new(Line::from(footer_spans));
    f.render_widget(footer, chunks[2]);
```

In `src/main.rs`, populate the field when building the RenderContext:

```rust
            notification_flash: notification_flash
                .filter(|(t, _)| t.elapsed() < std::time::Duration::from_secs(2))
                .map(|(_, msg)| msg.to_string()),
```

Clear expired flash:

```rust
        if notification_flash.as_ref().map_or(false, |(t, _)| t.elapsed() >= std::time::Duration::from_secs(2)) {
            notification_flash = None;
        }
```

**Step 4: Add `[n]` to footer hints**

In `src/ui/app_ui.rs`, in the Normal navigation mode footer pairs (line 160), add `("n", "Notif")`:

```rust
        AppState::Normal { .. } => vec![("Tab/↑↓", "Select"), ("Enter", "Open"), ("a", "Add"), ("n", "Notif"), ("s", "Subscribe"), ("p", "Channel"), ("i", "Identity"), ("f", "Requests"), ("q", "Quit")],
```

**Step 5: Run build**

Run: `cargo build`
Expected: Compiles.

**Step 6: Commit**

```bash
git add src/ui/state.rs src/ui/app_ui.rs src/main.rs
git commit -m "feat: add [n] keybinding to toggle desktop notifications"
```

---

### Task 12: Clean Up Compiler Warnings

**Files:**
- Modify: `src/ui/state.rs` (unused import, dead code)
- Modify: `src/crypto/signal.rs` (dead code)
- Modify: `src/tor/address.rs` (unused variable)
- Modify: `src/tor/mod.rs` (unused imports)
- Modify: `src/db/mod.rs` (unused imports)
- Modify: `src/protocol/mod.rs` (unused imports)

**Step 1: Fix all warnings identified in diagnostics**

Fix each warning:
- `state.rs:792`: Remove unused `use crate::db::queries::PendingFriendRequest`
- `state.rs:27`: Field `timestamp` is never read — prefix with `_` if in struct variant
- `state.rs:79`: Variant `SelectChannel` is never constructed — remove if unused, or prefix with `_`
- `signal.rs:151`: Field `session_data` never read — prefix with `_`
- `signal.rs:26-27`: Fields `signed_prekey_secret` and `prekey_secret` — prefix with `_`
- `address.rs:26`: Unused variable `pubkey` — prefix with `_`
- `address.rs:15`: Function `friend_code_to_onion` never used — add `#[allow(dead_code)]` or remove
- `tor/mod.rs:6-9`: Unused imports — remove
- `db/mod.rs:5`: Unused imports `CREATE_TABLES`, `SCHEMA_VERSION` — remove
- `protocol/mod.rs:5`: Unused imports — remove

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings.

**Step 3: Commit**

```bash
git add -A
git commit -m "chore: clean up compiler warnings (unused imports, dead code)"
```

---

### Task 13: Integration Tests

**Files:**
- Modify: `tests/integration/messaging_test.rs` or create `tests/integration/presence_test.rs`

**Step 1: Write presence integration test**

Create `tests/integration/presence_test.rs`:

```rust
use chattor::presence::{self, PeerPresence, OFFLINE_THRESHOLD, TYPING_TIMEOUT};
use chattor::protocol::message::{Message, PresenceMessage, PresenceType};
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_presence_map_concurrent_access() {
    let map = presence::new_presence_map();

    // Simulate multiple peers sending heartbeats concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let map = map.clone();
        handles.push(tokio::spawn(async move {
            presence::record_heartbeat(&map, &format!("peer{}.onion", i)).await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.len(), 10);
    for i in 0..10 {
        let key = format!("peer{}.onion", i);
        assert_eq!(snap.get(&key), Some(&(true, false)));
    }
}

#[tokio::test]
async fn test_typing_then_message_clears_typing() {
    let map = presence::new_presence_map();

    // Peer starts typing
    presence::record_typing_started(&map, "alice.onion").await;
    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.get("alice.onion"), Some(&(true, true)));

    // Peer sends message (stops typing)
    presence::record_typing_stopped(&map, "alice.onion").await;
    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.get("alice.onion"), Some(&(true, false)));
}

#[test]
fn test_presence_message_serialization_via_framing() {
    let msg = Message::Presence(PresenceMessage {
        from_onion: "abc123.onion".to_string(),
        presence_type: PresenceType::TypingStarted,
        timestamp: 1700000000,
    });

    let json = serde_json::to_vec(&msg).unwrap();
    let decoded: Message = serde_json::from_slice(&json).unwrap();
    assert_eq!(msg, decoded);

    // Verify the JSON structure
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["type"], "presence");
    assert_eq!(value["from_onion"], "abc123.onion");
}
```

**Step 2: Update integration test mod.rs if needed**

If `tests/integration/mod.rs` exists, add `mod presence_test;`.

**Step 3: Run tests**

Run: `cargo test --test integration`
Expected: All integration tests pass.

**Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass (should be ~220+ now).

**Step 5: Commit**

```bash
git add tests/
git commit -m "test: add presence integration tests"
```

---

### Task 14: Update CLAUDE.md and Documentation

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update CLAUDE.md**

Add presence and notifications to the Key Components section. Update test count. Add the new files to Key Files. Update the Architecture Overview flow diagram. Update Future Work to remove the completed items.

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for UX polish features"
```
