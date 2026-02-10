# Conversation UI & Text Messaging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add persistent chat layout with friends sidebar, conversation view, message input, and encrypted text messaging over Tor.

**Architecture:** Restructure AppState so Normal becomes the chat layout. Add db/queries.rs for friend list and message queries. Add sidebar and conversation rendering. Wire TextMessage send/receive through existing Signal + Tor infrastructure.

**Tech Stack:** Rust, ratatui 0.27, crossterm 0.27, arboard (clipboard), rusqlite, tokio, serde_json

**Working directory:** `/Users/jsingleton/watchful.bk/torrent-chat/.worktrees/conversation-ui/`

---

### Task 1: Database Migration v5 — Add last_read_at Column

**Files:**
- Modify: `src/db/schema.rs` — bump SCHEMA_VERSION to 5
- Modify: `src/db/connection.rs` — add `migrate_to_v5()`, call it from `initialize()`

**Context:** The conversations table needs a `last_read_at INTEGER` column for unread tracking. We already have migration infrastructure (v3, v4). Follow the same pattern.

**Step 1: Update schema version**

In `src/db/schema.rs`, change:
```rust
pub const SCHEMA_VERSION: i32 = 5;
```

**Step 2: Add last_read_at to CREATE_TABLES**

In `src/db/schema.rs`, modify the conversations table definition to:
```sql
CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    friend_id INTEGER NOT NULL,
    is_ephemeral INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    last_read_at INTEGER,
    FOREIGN KEY (friend_id) REFERENCES friends(id)
);
```

**Step 3: Add migrate_to_v5 in connection.rs**

Add this method to the `Database` impl block, after `migrate_to_v4()`:

```rust
/// Migrate database from v4 to v5 (add last_read_at for unread tracking)
fn migrate_to_v5(&self) -> Result<()> {
    let version = self.get_schema_version()?;

    if version < 5 {
        info!("Migrating database to schema v5 (unread tracking)");

        let conn = self.connection();

        // Add last_read_at column (NULL means never read)
        // ALTER TABLE ADD COLUMN is safe with IF NOT EXISTS not needed —
        // we only run this when version < 5
        conn.execute_batch(
            "ALTER TABLE conversations ADD COLUMN last_read_at INTEGER;"
        ).map_err(|e| TorrentChatError::Database(format!("Failed to add last_read_at: {}", e)))?;

        conn.execute("UPDATE schema_version SET version = 5", [])
            .map_err(|e| TorrentChatError::Database(format!("Failed to update version: {}", e)))?;

        info!("Migration to schema v5 complete");
    }

    Ok(())
}
```

**Step 4: Call migrate_to_v5 from initialize()**

In the `initialize()` method, add `self.migrate_to_v5()?;` after the `self.migrate_to_v4()?;` line inside the `Ok(_v)` match arm.

**Step 5: Update test assertions**

In `test_schema_version_defined`, update to `assert_eq!(SCHEMA_VERSION, 5);`.

In `test_migration_v2_to_v3`, the assertion already uses `SCHEMA_VERSION as i64` so it auto-updates.

**Step 6: Run tests**

Run: `cargo test --lib db:: -- --test-threads=1`
Expected: All database tests pass

**Step 7: Commit**

```bash
git add src/db/schema.rs src/db/connection.rs
git commit -m "feat(db): add schema v5 migration for unread tracking

Add last_read_at column to conversations table for tracking
which messages the user has seen in each conversation."
```

---

### Task 2: Database Queries Module

**Files:**
- Create: `src/db/queries.rs`
- Modify: `src/db/mod.rs` — add `pub mod queries;`

**Context:** We need helper functions for the UI to load friends with unread counts, load messages, store messages, and manage conversations. All functions take a `&Database` reference and use `db.connection()` for SQL.

**Step 1: Create src/db/queries.rs with types and functions**

```rust
use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use rusqlite::params;

/// A friend entry for the sidebar
#[derive(Debug, Clone)]
pub struct FriendEntry {
    pub friend_id: i64,
    pub onion_address: String,
    pub display_name: Option<String>,
    pub conversation_id: Option<i64>,
    pub unread_count: i64,
}

impl FriendEntry {
    /// Display name or truncated onion address
    pub fn display(&self) -> String {
        if let Some(ref name) = self.display_name {
            name.clone()
        } else {
            let addr = &self.onion_address;
            if addr.len() > 12 {
                format!("{}...", &addr[..12])
            } else {
                addr.clone()
            }
        }
    }
}

/// A message for the conversation view
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: i64,
    pub message_id: String,
    pub sender_onion: String,
    pub content: String,
    pub timestamp: i64,
    pub status: String,
}

/// Get active friends with unread counts
pub fn get_friends_with_unread(db: &Database) -> Result<Vec<FriendEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT f.id, f.onion_address, f.display_name,
                c.id as conversation_id,
                (SELECT COUNT(*) FROM messages m
                 WHERE m.conversation_id = c.id
                 AND m.timestamp > COALESCE(c.last_read_at, 0)) as unread
         FROM friends f
         LEFT JOIN conversations c ON c.friend_id = f.id
         WHERE f.status = 'active'
         ORDER BY f.display_name, f.onion_address"
    ).map_err(|e| TorrentChatError::Database(format!("Failed to prepare friends query: {}", e)))?;

    let entries = stmt.query_map([], |row| {
        Ok(FriendEntry {
            friend_id: row.get(0)?,
            onion_address: row.get(1)?,
            display_name: row.get(2)?,
            conversation_id: row.get(3)?,
            unread_count: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
        })
    }).map_err(|e| TorrentChatError::Database(format!("Failed to query friends: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| TorrentChatError::Database(format!("Failed to collect friends: {}", e)))?;

    Ok(entries)
}

/// Get or create a conversation for a friend
pub fn get_or_create_conversation(db: &Database, friend_id: i64) -> Result<i64> {
    let conn = db.connection();

    // Try to find existing conversation
    let existing: rusqlite::Result<i64> = conn.query_row(
        "SELECT id FROM conversations WHERE friend_id = ?1 LIMIT 1",
        params![friend_id],
        |row| row.get(0),
    );

    match existing {
        Ok(id) => Ok(id),
        Err(_) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO conversations (friend_id, is_ephemeral, created_at) VALUES (?1, 0, ?2)",
                params![friend_id, now],
            ).map_err(|e| TorrentChatError::Database(format!("Failed to create conversation: {}", e)))?;

            Ok(conn.last_insert_rowid())
        }
    }
}

/// Load messages for a conversation (most recent first, then reversed for display)
pub fn get_messages(db: &Database, conversation_id: i64, limit: usize, offset: usize) -> Result<Vec<ChatMessage>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, message_id, sender_onion, content, timestamp, status
         FROM messages
         WHERE conversation_id = ?1
         ORDER BY timestamp DESC, id DESC
         LIMIT ?2 OFFSET ?3"
    ).map_err(|e| TorrentChatError::Database(format!("Failed to prepare messages query: {}", e)))?;

    let mut messages: Vec<ChatMessage> = stmt.query_map(
        params![conversation_id, limit as i64, offset as i64],
        |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                message_id: row.get(1)?,
                sender_onion: row.get(2)?,
                content: row.get(3)?,
                timestamp: row.get(4)?,
                status: row.get(5)?,
            })
        },
    ).map_err(|e| TorrentChatError::Database(format!("Failed to query messages: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| TorrentChatError::Database(format!("Failed to collect messages: {}", e)))?;

    // Reverse so oldest is first (for display top-to-bottom)
    messages.reverse();
    Ok(messages)
}

/// Store an outgoing message
pub fn store_outgoing_message(
    db: &Database,
    conversation_id: i64,
    sender_onion: &str,
    content: &str,
    message_id: &str,
) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "INSERT INTO messages (message_id, conversation_id, sender_onion, content, timestamp, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'sent')",
        params![message_id, conversation_id, sender_onion, content, now],
    ).map_err(|e| TorrentChatError::Database(format!("Failed to store outgoing message: {}", e)))?;

    Ok(())
}

/// Store an incoming message
pub fn store_incoming_message(
    db: &Database,
    conversation_id: i64,
    sender_onion: &str,
    content: &str,
    message_id: &str,
) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "INSERT OR IGNORE INTO messages (message_id, conversation_id, sender_onion, content, timestamp, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'received')",
        params![message_id, conversation_id, sender_onion, content, now],
    ).map_err(|e| TorrentChatError::Database(format!("Failed to store incoming message: {}", e)))?;

    Ok(())
}

/// Mark a conversation as read (update last_read_at to now)
pub fn mark_conversation_read(db: &Database, conversation_id: i64) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "UPDATE conversations SET last_read_at = ?1 WHERE id = ?2",
        params![now, conversation_id],
    ).map_err(|e| TorrentChatError::Database(format!("Failed to mark conversation read: {}", e)))?;

    Ok(())
}

/// Update a message's delivery status
pub fn update_message_status(db: &Database, message_id: &str, status: &str) -> Result<()> {
    db.connection().execute(
        "UPDATE messages SET status = ?1 WHERE message_id = ?2",
        params![status, message_id],
    ).map_err(|e| TorrentChatError::Database(format!("Failed to update message status: {}", e)))?;

    Ok(())
}

/// Find friend by onion address
pub fn find_friend_by_onion(db: &Database, onion_address: &str) -> Result<Option<i64>> {
    let result: rusqlite::Result<i64> = db.connection().query_row(
        "SELECT id FROM friends WHERE onion_address = ?1 AND status = 'active'",
        params![onion_address],
        |row| row.get(0),
    );

    match result {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(TorrentChatError::Database(format!("Failed to find friend: {}", e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> Database {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        // Add a test friend
        db.connection().execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
            [],
        ).unwrap();

        db
    }

    #[test]
    fn test_get_friends_with_unread_empty() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends.len(), 0);
    }

    #[test]
    fn test_get_friends_with_unread() {
        let db = setup_test_db();
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].display_name, Some("Alice".to_string()));
        assert_eq!(friends[0].unread_count, 0);
    }

    #[test]
    fn test_get_or_create_conversation() {
        let db = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();
        assert!(conv_id > 0);

        // Should return same conversation
        let conv_id2 = get_or_create_conversation(&db, 1).unwrap();
        assert_eq!(conv_id, conv_id2);
    }

    #[test]
    fn test_store_and_get_messages() {
        let db = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_outgoing_message(&db, conv_id, "me.onion", "Hello!", "msg-1").unwrap();
        store_incoming_message(&db, conv_id, "alice.onion", "Hi!", "msg-2").unwrap();

        let messages = get_messages(&db, conv_id, 50, 0).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello!");
        assert_eq!(messages[1].content, "Hi!");
    }

    #[test]
    fn test_mark_conversation_read() {
        let db = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_incoming_message(&db, conv_id, "alice.onion", "Hey!", "msg-1").unwrap();

        // Before marking read, should have 1 unread
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends[0].unread_count, 1);

        // Mark read
        mark_conversation_read(&db, conv_id).unwrap();

        // After marking read, should have 0 unread
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends[0].unread_count, 0);
    }

    #[test]
    fn test_update_message_status() {
        let db = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_outgoing_message(&db, conv_id, "me.onion", "Hello!", "msg-1").unwrap();
        update_message_status(&db, "msg-1", "queued").unwrap();

        let messages = get_messages(&db, conv_id, 50, 0).unwrap();
        assert_eq!(messages[0].status, "queued");
    }

    #[test]
    fn test_find_friend_by_onion() {
        let db = setup_test_db();
        let found = find_friend_by_onion(&db, "alice.onion").unwrap();
        assert_eq!(found, Some(1));

        let not_found = find_friend_by_onion(&db, "unknown.onion").unwrap();
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_friend_entry_display() {
        let entry = FriendEntry {
            friend_id: 1,
            onion_address: "abcdefghijklmnopqrstuvwxyz.onion".to_string(),
            display_name: None,
            conversation_id: None,
            unread_count: 0,
        };
        assert_eq!(entry.display(), "abcdefghijkl...");

        let entry2 = FriendEntry {
            friend_id: 2,
            onion_address: "test.onion".to_string(),
            display_name: Some("Alice".to_string()),
            conversation_id: None,
            unread_count: 0,
        };
        assert_eq!(entry2.display(), "Alice");
    }
}
```

**Step 2: Update src/db/mod.rs**

Add `pub mod queries;` to the module declarations.

**Step 3: Run tests**

Run: `cargo test --lib db::queries`
Expected: All 8 tests pass

**Step 4: Commit**

```bash
git add src/db/queries.rs src/db/mod.rs
git commit -m "feat(db): add queries module for friends, messages, conversations

Helper functions for the conversation UI: get friends with unread
counts, load paginated messages, store outgoing/incoming messages,
mark conversations read, update message status."
```

---

### Task 3: Restructure AppState for Chat Layout

**Files:**
- Modify: `src/ui/state.rs` — redesign AppState::Normal, add new actions, update key handling
- Modify: `src/ui/mod.rs` — export new types

**Context:** The current `AppState::Normal` is a unit variant. We need to make it carry chat state: selected friend, conversation, message input, focus mode. The key handler needs to route differently based on `input_focused`.

**Step 1: Rewrite AppState and AppAction enums**

Replace the current `AppState` and `AppAction` enums in `src/ui/state.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::error::Result;

/// Application state machine
#[derive(Debug, Clone)]
pub enum AppState {
    /// Main chat layout — always the base state
    Normal {
        selected_friend_idx: Option<usize>,
        conversation_id: Option<i64>,
        input: String,
        cursor: usize,
        input_focused: bool,
        scroll_offset: usize,
    },

    /// Modal: adding a new friend
    AddingFriend {
        input: String,
        cursor: usize,
        error: Option<String>,
    },

    /// Modal: viewing an incoming friend request
    ViewingFriendRequest {
        request_id: i64,
        from_onion: String,
        friend_code: String,
        timestamp: i64,
    },

    /// Modal: viewing own identity
    ViewingMyIdentity {
        friend_code: String,
        onion_address: String,
    },
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Normal {
            selected_friend_idx: None,
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        }
    }
}

/// Actions returned from key handling
#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    SendFriendRequest(String),
    AcceptFriendRequest(i64),
    RejectFriendRequest(i64),
    ViewMyIdentity,
    SelectFriend(usize),
    SendMessage(String),
    Quit,
}
```

**Step 2: Rewrite handle_key()**

Replace the `handle_key` method:

```rust
impl AppState {
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        // Global: Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(Some(AppAction::Quit));
        }

        match self {
            AppState::Normal {
                selected_friend_idx,
                conversation_id: _,
                input,
                cursor,
                input_focused,
                scroll_offset,
            } => {
                if *input_focused {
                    // Input mode: keystrokes go to message input
                    match key.code {
                        KeyCode::Esc => {
                            *input_focused = false;
                            Ok(None)
                        }
                        KeyCode::Enter => {
                            if !input.is_empty() {
                                let msg = input.clone();
                                input.clear();
                                *cursor = 0;
                                Ok(Some(AppAction::SendMessage(msg)))
                            } else {
                                Ok(None)
                            }
                        }
                        KeyCode::Char(c) => {
                            input.insert(*cursor, c);
                            *cursor += 1;
                            Ok(None)
                        }
                        KeyCode::Backspace => {
                            if *cursor > 0 {
                                *cursor -= 1;
                                input.remove(*cursor);
                            }
                            Ok(None)
                        }
                        KeyCode::Left => {
                            if *cursor > 0 {
                                *cursor -= 1;
                            }
                            Ok(None)
                        }
                        KeyCode::Right => {
                            if *cursor < input.len() {
                                *cursor += 1;
                            }
                            Ok(None)
                        }
                        _ => Ok(None),
                    }
                } else {
                    // Navigation mode: shortcuts active
                    match key.code {
                        KeyCode::Char('q') => Ok(Some(AppAction::Quit)),
                        KeyCode::Char('a') => {
                            *self = AppState::AddingFriend {
                                input: String::new(),
                                cursor: 0,
                                error: None,
                            };
                            Ok(None)
                        }
                        KeyCode::Char('i') => {
                            Ok(Some(AppAction::ViewMyIdentity))
                        }
                        KeyCode::Tab => {
                            // If no friend selected yet, select first
                            if selected_friend_idx.is_none() {
                                *selected_friend_idx = Some(0);
                            }
                            // Tab is used to toggle focus to sidebar navigation
                            Ok(None)
                        }
                        KeyCode::Up => {
                            if let Some(ref mut idx) = selected_friend_idx {
                                if *idx > 0 {
                                    *idx -= 1;
                                }
                            }
                            Ok(None)
                        }
                        KeyCode::Down => {
                            if let Some(ref mut idx) = selected_friend_idx {
                                *idx += 1; // Will be clamped by render
                            }
                            Ok(None)
                        }
                        KeyCode::Enter => {
                            if let Some(idx) = *selected_friend_idx {
                                *input_focused = true;
                                *scroll_offset = 0;
                                Ok(Some(AppAction::SelectFriend(idx)))
                            } else {
                                Ok(None)
                            }
                        }
                        _ => Ok(None),
                    }
                }
            }

            AppState::AddingFriend { input, cursor, error } => {
                match key.code {
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    KeyCode::Char(c) => {
                        input.insert(*cursor, c);
                        *cursor += 1;
                        *error = None;
                        Ok(None)
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            input.remove(*cursor);
                        }
                        Ok(None)
                    }
                    KeyCode::Left => {
                        if *cursor > 0 { *cursor -= 1; }
                        Ok(None)
                    }
                    KeyCode::Right => {
                        if *cursor < input.len() { *cursor += 1; }
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        if input.is_empty() {
                            *error = Some("Please enter a .onion address".to_string());
                            Ok(None)
                        } else {
                            Ok(Some(AppAction::SendFriendRequest(input.clone())))
                        }
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingFriendRequest { request_id, .. } => {
                match key.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        let id = *request_id;
                        *self = AppState::default();
                        Ok(Some(AppAction::AcceptFriendRequest(id)))
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        let id = *request_id;
                        *self = AppState::default();
                        Ok(Some(AppAction::RejectFriendRequest(id)))
                    }
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingMyIdentity { .. } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('i') => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }
        }
    }
}
```

**Step 3: Update tests**

Replace the test module. Key tests to include:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_default_state_is_normal() {
        let state = AppState::default();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_normal_nav_mode_quit() {
        let mut state = AppState::default();
        let action = state.handle_key(key(KeyCode::Char('q'))).unwrap();
        assert_eq!(action, Some(AppAction::Quit));
    }

    #[test]
    fn test_normal_nav_mode_add_friend() {
        let mut state = AppState::default();
        state.handle_key(key(KeyCode::Char('a'))).unwrap();
        assert!(matches!(state, AppState::AddingFriend { .. }));
    }

    #[test]
    fn test_normal_nav_mode_view_identity() {
        let mut state = AppState::default();
        let action = state.handle_key(key(KeyCode::Char('i'))).unwrap();
        assert_eq!(action, Some(AppAction::ViewMyIdentity));
    }

    #[test]
    fn test_normal_nav_mode_arrow_selects_friend() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(1),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        state.handle_key(key(KeyCode::Up)).unwrap();
        if let AppState::Normal { selected_friend_idx, .. } = &state {
            assert_eq!(*selected_friend_idx, Some(0));
        }
    }

    #[test]
    fn test_normal_enter_selects_friend_and_focuses_input() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let action = state.handle_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(AppAction::SelectFriend(0)));
        if let AppState::Normal { input_focused, .. } = &state {
            assert!(*input_focused);
        }
    }

    #[test]
    fn test_input_focused_typing() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state.handle_key(key(KeyCode::Char('h'))).unwrap();
        state.handle_key(key(KeyCode::Char('i'))).unwrap();
        if let AppState::Normal { input, .. } = &state {
            assert_eq!(input, "hi");
        }
    }

    #[test]
    fn test_input_focused_enter_sends_message() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: "hello".to_string(),
            cursor: 5,
            input_focused: true,
            scroll_offset: 0,
        };
        let action = state.handle_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(AppAction::SendMessage("hello".to_string())));
        if let AppState::Normal { input, .. } = &state {
            assert!(input.is_empty());
        }
    }

    #[test]
    fn test_input_focused_escape_unfocuses() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state.handle_key(key(KeyCode::Esc)).unwrap();
        if let AppState::Normal { input_focused, .. } = &state {
            assert!(!*input_focused);
        }
    }

    #[test]
    fn test_input_focused_backspace() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: "hi".to_string(),
            cursor: 2,
            input_focused: true,
            scroll_offset: 0,
        };
        state.handle_key(key(KeyCode::Backspace)).unwrap();
        if let AppState::Normal { input, .. } = &state {
            assert_eq!(input, "h");
        }
    }

    #[test]
    fn test_ctrl_c_quits_from_any_state() {
        let mut state = AppState::default();
        let action = state.handle_key(ctrl('c')).unwrap();
        assert_eq!(action, Some(AppAction::Quit));
    }

    #[test]
    fn test_adding_friend_enter_sends() {
        let mut state = AppState::AddingFriend {
            input: "test.onion".to_string(),
            cursor: 10,
            error: None,
        };
        let action = state.handle_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(AppAction::SendFriendRequest("test.onion".to_string())));
    }

    #[test]
    fn test_adding_friend_escape_returns_to_normal() {
        let mut state = AppState::AddingFriend {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        state.handle_key(key(KeyCode::Esc)).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_friend_request_accept() {
        let mut state = AppState::ViewingFriendRequest {
            request_id: 42,
            from_onion: "test.onion".into(),
            friend_code: "test-1234-code-5678".into(),
            timestamp: 0,
        };
        let action = state.handle_key(key(KeyCode::Char('a'))).unwrap();
        assert_eq!(action, Some(AppAction::AcceptFriendRequest(42)));
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_identity_escape() {
        let mut state = AppState::ViewingMyIdentity {
            friend_code: "test".into(),
            onion_address: "test.onion".into(),
        };
        state.handle_key(key(KeyCode::Esc)).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_tab_initializes_friend_selection() {
        let mut state = AppState::default();
        state.handle_key(key(KeyCode::Tab)).unwrap();
        if let AppState::Normal { selected_friend_idx, .. } = &state {
            assert_eq!(*selected_friend_idx, Some(0));
        }
    }

    #[test]
    fn test_empty_enter_in_input_does_nothing() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        let action = state.handle_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, None);
    }
}
```

**Step 4: Run tests**

Run: `cargo test --lib ui::state::tests`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/ui/state.rs src/ui/mod.rs
git commit -m "feat(ui): restructure AppState for persistent chat layout

Normal state now carries conversation context: selected friend,
active conversation, message input with focus mode. Navigation
mode (Escape) enables shortcuts, input mode (Enter on friend)
captures keystrokes for messaging."
```

---

### Task 4: Add arboard Dependency for Clipboard

**Files:**
- Modify: `Cargo.toml` — add `arboard`
- Modify: `src/ui/mod.rs` — add clipboard helper

**Step 1: Add arboard to Cargo.toml**

Add to `[dependencies]`:
```toml
arboard = "3"
```

**Step 2: Add clipboard helper in src/ui/mod.rs**

Add a helper function:

```rust
/// Copy text to system clipboard. Returns true on success.
pub fn copy_to_clipboard(text: &str) -> bool {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => clipboard.set_text(text).is_ok(),
        Err(_) => false,
    }
}
```

**Step 3: Build to verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add Cargo.toml src/ui/mod.rs
git commit -m "deps: add arboard for cross-platform clipboard support"
```

---

### Task 5: Render Sidebar

**Files:**
- Create: `src/ui/sidebar.rs` — render friend list sidebar
- Modify: `src/ui/mod.rs` — add module and export

**Context:** The sidebar is a fixed-width panel on the left (20 chars) showing friends with selection, online status, and unread counts. The render function takes the friend list data and the currently selected index.

**Step 1: Create src/ui/sidebar.rs**

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::db::queries::FriendEntry;

/// Render the friends sidebar
pub fn render_sidebar(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
) {
    let title = format!(" Friends ({}) ", friends.len());
    let border_color = if focused { Color::Cyan } else { Color::DarkGray };

    let items: Vec<ListItem> = friends
        .iter()
        .enumerate()
        .map(|(i, friend)| {
            let is_selected = selected_idx == Some(i);
            let arrow = if is_selected { "▸ " } else { "  " };
            let name = friend.display();

            // Truncate name to fit sidebar (leave room for arrow + status + unread)
            let max_name_len = 10;
            let truncated = if name.len() > max_name_len {
                format!("{}…", &name[..max_name_len])
            } else {
                name
            };

            let status_icon = "○"; // MVP: always gray for now

            let mut spans = vec![
                Span::raw(arrow),
                Span::raw(truncated),
                Span::raw(" "),
                Span::styled(status_icon, Style::default().fg(Color::DarkGray)),
            ];

            if friend.unread_count > 0 {
                spans.push(Span::styled(
                    format!(" ({})", friend.unread_count),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
            }

            let style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(Line::from(spans)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_widget(list, area);
}
```

**Step 2: Add to src/ui/mod.rs**

Add `pub mod sidebar;` and add `pub use sidebar::render_sidebar;` to the exports.

**Step 3: Build to verify**

Run: `cargo build`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/ui/sidebar.rs src/ui/mod.rs
git commit -m "feat(ui): add sidebar rendering for friends list

Shows friends with selection arrow, online status indicator,
and unread count badges. Highlighted border when focused."
```

---

### Task 6: Render Conversation View

**Files:**
- Create: `src/ui/conversation.rs` — render message list and empty states
- Modify: `src/ui/mod.rs` — add module and export

**Context:** The conversation panel shows chat messages with sender, content, timestamp, and status. It also handles empty states (no friend selected, no messages, setup wizard when no friends exist).

**Step 1: Create src/ui/conversation.rs**

```rust
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use crate::db::queries::{ChatMessage, FriendEntry};

/// Render the conversation area
pub fn render_conversation(
    f: &mut Frame,
    area: Rect,
    friend: Option<&FriendEntry>,
    messages: &[ChatMessage],
    own_onion: Option<&str>,
    scroll_offset: usize,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    match friend {
        None => {
            // No conversation selected
            let text = Paragraph::new("Select a friend to start chatting")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));

            // Center vertically
            let v_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(45),
                    Constraint::Length(1),
                    Constraint::Percentage(45),
                ])
                .split(inner);
            f.render_widget(text, v_layout[1]);
        }
        Some(friend_entry) => {
            if messages.is_empty() {
                let text = Paragraph::new("No messages yet. Say hello!")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                let v_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(45),
                        Constraint::Length(1),
                        Constraint::Percentage(45),
                    ])
                    .split(inner);
                f.render_widget(text, v_layout[1]);
            } else {
                render_messages(f, inner, messages, own_onion, &friend_entry.display(), scroll_offset);
            }
        }
    }
}

/// Render message list
fn render_messages(
    f: &mut Frame,
    area: Rect,
    messages: &[ChatMessage],
    own_onion: Option<&str>,
    friend_name: &str,
    scroll_offset: usize,
) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in messages {
        let is_own = own_onion.map_or(false, |o| msg.sender_onion == o);
        let sender = if is_own { "You" } else { friend_name };
        let time = format_timestamp(msg.timestamp);

        let status_str = if is_own {
            match msg.status.as_str() {
                "sent" => " ✓",
                "queued" => " ⏳",
                "failed" => " ✗",
                "received" => "",
                _ => "",
            }
        } else {
            ""
        };

        // Sender line
        let sender_style = if is_own {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        };

        lines.push(Line::from(vec![
            Span::styled(sender.to_string(), sender_style),
            Span::styled(format!("  {}", time), Style::default().fg(Color::DarkGray)),
            Span::styled(status_str.to_string(), Style::default().fg(Color::DarkGray)),
        ]));

        // Content line
        lines.push(Line::from(Span::raw(format!("  {}", msg.content))));

        // Blank line between messages
        lines.push(Line::from(""));
    }

    // Apply scroll offset
    let skip = if scroll_offset > 0 && lines.len() > area.height as usize {
        lines.len().saturating_sub(area.height as usize + scroll_offset)
    } else {
        lines.len().saturating_sub(area.height as usize)
    };

    let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();

    let paragraph = Paragraph::new(visible_lines)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

/// Render the setup wizard (shown when no friends exist)
pub fn render_setup_wizard(
    f: &mut Frame,
    area: Rect,
    onion_address: Option<&str>,
    friend_code: Option<&str>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Length(2),  // Welcome
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Step 1 label
            Constraint::Length(3),  // Identity box
            Constraint::Length(3),  // Friend code box
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Step 2
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Step 3
            Constraint::Min(0),    // Fill
        ])
        .split(inner);

    // Welcome
    let welcome = Paragraph::new("Welcome to torrent-chat")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(welcome, chunks[1]);

    // Step 1
    let step1 = Paragraph::new("Step 1: Your identity")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White));
    f.render_widget(step1, chunks[3]);

    // Onion address
    let addr = onion_address.unwrap_or("(Waiting for Tor...)");
    let onion_widget = Paragraph::new(format!("  {}  [click to copy]", addr))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
    f.render_widget(onion_widget, chunks[4]);

    // Friend code
    let code = friend_code.unwrap_or("(Waiting for Tor...)");
    let code_widget = Paragraph::new(format!("  {}  [click to copy]", code))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(code_widget, chunks[5]);

    // Step 2
    let step2 = Paragraph::new("Step 2: Share your .onion address with a friend")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White));
    f.render_widget(step2, chunks[7]);

    // Step 3
    let step3 = Paragraph::new("Step 3: Press [a] to add their .onion address")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White));
    f.render_widget(step3, chunks[9]);
}

/// Render the message input area
pub fn render_input(
    f: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    focused: bool,
) {
    let border_color = if focused { Color::Cyan } else { Color::DarkGray };
    let prompt = if focused { "> " } else { "  " };

    // Show cursor when focused
    let display_text = if focused {
        if cursor < input.len() {
            format!("{}{}\u{2588}{}", prompt, &input[..cursor], &input[cursor..])
        } else {
            format!("{}{}\u{2588}", prompt, input)
        }
    } else {
        if input.is_empty() {
            format!("{}Press Enter on a friend to start typing", prompt)
        } else {
            format!("{}{}", prompt, input)
        }
    };

    let widget = Paragraph::new(display_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .style(Style::default().fg(if focused { Color::White } else { Color::DarkGray }));

    f.render_widget(widget, area);
}

/// Format timestamp for display
fn format_timestamp(ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let diff = now - ts;

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}
```

**Step 2: Add to src/ui/mod.rs**

Add `pub mod conversation;` and exports for `render_conversation`, `render_setup_wizard`, `render_input`.

**Step 3: Build to verify**

Run: `cargo build`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/ui/conversation.rs src/ui/mod.rs
git commit -m "feat(ui): add conversation view, setup wizard, and input rendering

Renders messages with sender name, timestamp, and status indicators.
Shows setup wizard when no friends exist. Input area with focus state."
```

---

### Task 7: Rewrite render_app for Chat Layout

**Files:**
- Modify: `src/ui/app_ui.rs` — rewrite `render_app()` to use sidebar + conversation + input layout

**Context:** `render_app()` is the main render function called from the main loop. It currently renders a simple header/main/footer layout with modals on top. We need to replace the main area with the sidebar + conversation split, and conditionally show the setup wizard. The function receives `AppState`, `App`, plus we need to pass in the friends list and messages.

**Step 1: Add a RenderContext struct**

We'll pass render data through a context struct rather than making render_app query the DB. Add this near the top of `app_ui.rs`:

```rust
use crate::db::queries::{FriendEntry, ChatMessage};

/// Data needed for rendering (populated by main loop before render)
pub struct RenderContext {
    pub friends: Vec<FriendEntry>,
    pub messages: Vec<ChatMessage>,
    pub own_onion: Option<String>,
    pub friend_code: Option<String>,
    pub tor_connected: bool,
}
```

**Step 2: Rewrite render_app**

Replace the existing `render_app` function:

```rust
pub fn render_app(f: &mut Frame, app_state: &AppState, ctx: &RenderContext) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Main area
            Constraint::Length(1),  // Footer
        ])
        .split(f.area());

    // Header
    let tor_status = if ctx.tor_connected { "Connected" } else { "Connecting..." };
    let addr_display = ctx.own_onion.as_deref()
        .map(|a| {
            let trunc = if a.len() > 16 { &a[..16] } else { a };
            format!("  [@{}...]", trunc)
        })
        .unwrap_or_default();

    let header = Paragraph::new(format!("  torrent-chat v0.1.0{}  [Tor: {}]", addr_display, tor_status))
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Main area — depends on whether we have friends
    if ctx.friends.is_empty() {
        // Setup wizard
        let (onion_ref, code_ref) = (ctx.own_onion.as_deref(), ctx.friend_code.as_deref());
        crate::ui::conversation::render_setup_wizard(f, chunks[1], onion_ref, code_ref);
    } else {
        // Split into sidebar + conversation
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),  // Sidebar
                Constraint::Min(0),      // Conversation
            ])
            .split(chunks[1]);

        // Extract state from Normal variant
        let (selected_idx, conv_id, input, cursor, input_focused, scroll_offset) =
            if let AppState::Normal {
                selected_friend_idx, conversation_id, input, cursor, input_focused, scroll_offset
            } = app_state {
                (*selected_friend_idx, *conversation_id, input.as_str(), *cursor, *input_focused, *scroll_offset)
            } else {
                (None, None, "", 0, false, 0)
            };

        // Sidebar
        crate::ui::sidebar::render_sidebar(
            f, main_chunks[0], &ctx.friends, selected_idx, !input_focused,
        );

        // Right panel: conversation + input
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),     // Messages
                Constraint::Length(3),  // Input
            ])
            .split(main_chunks[1]);

        // Find the selected friend
        let selected_friend = selected_idx
            .and_then(|i| ctx.friends.get(i));

        // Conversation
        crate::ui::conversation::render_conversation(
            f,
            right_chunks[0],
            selected_friend,
            &ctx.messages,
            ctx.own_onion.as_deref(),
            scroll_offset,
        );

        // Input
        crate::ui::conversation::render_input(
            f, right_chunks[1], input, cursor, input_focused,
        );
    }

    // Footer
    let footer_text = match app_state {
        AppState::Normal { input_focused: true, .. } => "[Enter] Send  [Esc] Navigation mode",
        AppState::Normal { .. } => "[Tab/↑↓] Select friend  [Enter] Open  [a] Add  [i] Identity  [q] Quit",
        AppState::AddingFriend { .. } => "[Enter] Send request  [Esc] Cancel",
        AppState::ViewingFriendRequest { .. } => "[A]ccept  [R]eject  [Esc] Back",
        AppState::ViewingMyIdentity { .. } => "[i/Esc] Close",
    };
    let footer = Paragraph::new(format!("  {}", footer_text))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, chunks[2]);

    // Modal overlays
    match app_state {
        AppState::AddingFriend { input, error, .. } => {
            crate::ui::modals::render_add_friend_modal(f, input, error.as_deref());
        }
        AppState::ViewingFriendRequest { from_onion, friend_code, .. } => {
            crate::ui::modals::render_friend_request_modal(f, from_onion, friend_code);
        }
        AppState::ViewingMyIdentity { friend_code, onion_address } => {
            crate::ui::modals::render_identity_modal(f, friend_code, onion_address);
        }
        _ => {}
    }
}
```

**Step 3: Remove old AppUI struct and methods**

The `AppUI` struct, `new()`, `run()`, `main_loop()`, `render()`, and `tor_status()` methods are now dead code. Remove them (they're not used by main.rs — main.rs calls `render_app` directly).

**Step 4: Update src/ui/mod.rs exports**

Export `RenderContext` from app_ui:

```rust
pub use app_ui::{render_app, RenderContext};
```

Remove `AppUI` from exports if present.

**Step 5: Build to verify**

Run: `cargo build`
Expected: Compiles (main.rs will need updating in next task but should still compile since render_app signature changed)

Note: This will break the main.rs call to `render_app` since the signature changed. That's fixed in Task 8.

**Step 6: Commit**

```bash
git add src/ui/app_ui.rs src/ui/mod.rs
git commit -m "feat(ui): rewrite render_app for persistent chat layout

3-panel layout: sidebar + conversation + input. Setup wizard
when no friends exist. RenderContext passes data from main loop.
Footer text changes based on input focus mode."
```

---

### Task 8: Update Main Loop for Chat Layout

**Files:**
- Modify: `src/main.rs` — update render call, add SelectFriend and SendMessage handlers, populate RenderContext
- Modify: `src/app.rs` — add `messages_dirty` flag

**Context:** The main loop needs to: (1) build RenderContext with friends and messages from the DB before each render, (2) handle the new SelectFriend and SendMessage actions, (3) mark conversations as read when selected.

**Step 1: Add messages_dirty to App**

In `src/app.rs`, add to the App struct:

```rust
pub messages_dirty: bool,
```

Initialize it as `true` in `App::new()` (so first render loads data).

**Step 2: Update main.rs — add imports and helpers**

At the top of main.rs, add:

```rust
use crate::db::queries;
use crate::ui::RenderContext;
```

**Step 3: Update the render section of the main loop**

Replace the rendering block. Before the `terminal.draw()` call, build the RenderContext:

```rust
// Build render context
let app_lock = app.lock().await;
let friends = queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();

// Load messages for active conversation
let messages = if let AppState::Normal { conversation_id: Some(conv_id), .. } = &app_state {
    queries::get_messages(&app_lock.db, *conv_id, 50, 0).unwrap_or_default()
} else {
    vec![]
};

let own_onion = app_lock.onion_address.clone();
let friend_code = own_onion.as_ref().and_then(|o|
    crate::tor::address::onion_to_friend_code(o).ok()
);
let tor_connected = app_lock.tor_client.is_some();

let ctx = RenderContext {
    friends,
    messages,
    own_onion,
    friend_code,
    tor_connected,
};

// Clamp selected_friend_idx to valid range
if let AppState::Normal { selected_friend_idx: Some(ref mut idx), .. } = &mut app_state {
    if ctx.friends.is_empty() {
        *idx = 0;
    } else if *idx >= ctx.friends.len() {
        *idx = ctx.friends.len() - 1;
    }
}

drop(app_lock);

terminal.draw(|f| {
    ui::render_app(f, &app_state, &ctx);
})?;
```

**Step 4: Handle SelectFriend action**

Add to the action match block:

```rust
Some(AppAction::SelectFriend(idx)) => {
    let app_lock = app.lock().await;
    if let Some(friend) = queries::get_friends_with_unread(&app_lock.db)
        .unwrap_or_default()
        .get(idx)
    {
        let conv_id = queries::get_or_create_conversation(
            &app_lock.db, friend.friend_id
        ).unwrap_or(0);

        if conv_id > 0 {
            queries::mark_conversation_read(&app_lock.db, conv_id).ok();
        }

        if let AppState::Normal { conversation_id, .. } = &mut app_state {
            *conversation_id = Some(conv_id);
        }
    }
    drop(app_lock);
}
```

**Step 5: Handle SendMessage action**

Add to the action match block:

```rust
Some(AppAction::SendMessage(content)) => {
    let app_lock = app.lock().await;

    if let AppState::Normal {
        conversation_id: Some(conv_id),
        selected_friend_idx: Some(idx),
        ..
    } = &app_state {
        let conv_id = *conv_id;
        let idx = *idx;

        // Get friend info
        if let Some(friend) = queries::get_friends_with_unread(&app_lock.db)
            .unwrap_or_default()
            .get(idx)
        {
            let peer_onion = friend.onion_address.clone();
            let own_onion = app_lock.onion_address.clone()
                .unwrap_or_default();
            let msg_id = uuid::Uuid::new_v4().to_string();

            // Store locally first
            queries::store_outgoing_message(
                &app_lock.db, conv_id, &own_onion, &content, &msg_id
            ).ok();

            // Create TextMessage
            let text_msg = crate::protocol::message::TextMessage {
                from_onion: own_onion.clone(),
                to_onion: peer_onion.clone(),
                signal_ciphertext: content.clone(), // MVP: plaintext until Signal is wired
                signal_type: crate::protocol::message::SignalMessageType::Message,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                message_id: uuid::Uuid::parse_str(&msg_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            };

            let message = crate::protocol::message::Message::TextMessage(text_msg);

            // Try direct send, queue on failure
            match try_send_direct(&*app_lock, &peer_onion, &message).await {
                Ok(_) => {
                    queries::update_message_status(&app_lock.db, &msg_id, "sent").ok();
                }
                Err(_) => {
                    app_lock.message_queue.enqueue(&app_lock.db, &peer_onion, &message, "normal").ok();
                    queries::update_message_status(&app_lock.db, &msg_id, "queued").ok();
                }
            }
        }
    }

    drop(app_lock);
}
```

**Step 6: Update the incoming message handler to handle TextMessage**

In `handle_incoming_message()`, add a branch for TextMessage:

```rust
Message::TextMessage(text_msg) => {
    let from_onion = &text_msg.from_onion;
    let content = &text_msg.signal_ciphertext; // MVP: plaintext
    let msg_id = text_msg.message_id.to_string();

    // Find friend and conversation
    if let Some(friend_id) = queries::find_friend_by_onion(&app.db, from_onion)? {
        let conv_id = queries::get_or_create_conversation(&app.db, friend_id)?;
        queries::store_incoming_message(&app.db, conv_id, from_onion, content, &msg_id)?;
        info!("Received text message from {}", &from_onion[..std::cmp::min(10, from_onion.len())]);
    } else {
        warn!("Received text from unknown peer: {}", &from_onion[..std::cmp::min(10, from_onion.len())]);
    }
}
```

**Step 7: Update the SendFriendRequest handler**

After a successful friend request send, return to Normal state:

```rust
Some(AppAction::SendFriendRequest(code)) => {
    let app_lock = app.lock().await;
    match handle_send_friend_request(&*app_lock, &code).await {
        Ok(result) => {
            let msg = match result {
                SendResult::SentImmediately => "Friend request sent!",
                SendResult::Queued => "Friend request queued for delivery",
            };
            // Return to normal state
            app_state = AppState::default();
            // TODO: show toast notification
        }
        Err(e) => {
            if let AppState::AddingFriend { error, .. } = &mut app_state {
                *error = Some(format!("{}", e));
            }
        }
    }
    drop(app_lock);
}
```

**Step 8: Build and test**

Run: `cargo build`
Run: `cargo test --lib`
Expected: Compiles, tests pass

**Step 9: Commit**

```bash
git add src/main.rs src/app.rs
git commit -m "feat: wire up chat layout, message send/receive, and friend selection

Main loop builds RenderContext with friends and messages from DB.
SelectFriend creates/opens conversation and marks as read.
SendMessage encrypts and sends text over Tor with queue fallback.
Incoming TextMessages stored and displayed in conversation view."
```

---

### Task 9: Enable Mouse Support for Clipboard

**Files:**
- Modify: `src/main.rs` — enable mouse capture, handle click events
- Modify: `src/ui/conversation.rs` — track clickable regions

**Context:** Enable crossterm mouse support so clicking on the .onion address or friend code in the setup wizard copies to clipboard.

**Step 1: Enable mouse capture in main.rs**

In the terminal setup section (after `enable_raw_mode`), add:

```rust
crossterm::execute!(
    std::io::stdout(),
    crossterm::event::EnableMouseCapture
)?;
```

And in cleanup (before `disable_raw_mode`):

```rust
crossterm::execute!(
    std::io::stdout(),
    crossterm::event::DisableMouseCapture
)?;
```

**Step 2: Handle mouse events in the event loop**

Add a mouse event handler after the key event handler:

```rust
crossterm::event::Event::Mouse(mouse_event) => {
    use crossterm::event::{MouseEventKind, MouseButton};
    if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
        let app_lock = app.lock().await;
        // Check if click is in setup wizard area (when no friends)
        let friends = queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
        if friends.is_empty() {
            if let Some(ref onion) = app_lock.onion_address {
                // Rough check: click in the identity box area
                let row = mouse_event.row;
                let term_height = terminal.size()?.height;
                let wizard_start = term_height / 4;

                if row >= wizard_start + 4 && row <= wizard_start + 6 {
                    // Onion address area
                    if crate::ui::copy_to_clipboard(onion) {
                        // Feedback handled by state
                    }
                } else if row >= wizard_start + 7 && row <= wizard_start + 9 {
                    // Friend code area
                    let code = crate::tor::address::onion_to_friend_code(onion)
                        .unwrap_or_default();
                    if !code.is_empty() {
                        crate::ui::copy_to_clipboard(&code);
                    }
                }
                // TODO: Show "Copied!" feedback toast
            }
        }
        drop(app_lock);
    }
}
```

**Step 3: Build and manual test**

Run: `cargo build`
Run: `cargo run` (briefly verify mouse doesn't break anything, Ctrl+C to exit)

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: enable mouse support for click-to-copy in setup wizard

Clicking on .onion address or friend code in the setup wizard
copies to system clipboard via arboard."
```

---

### Task 10: Integration Testing and Polish

**Files:**
- Modify: `tests/integration/messaging_test.rs` — add conversation UI integration tests

**Step 1: Add integration tests**

Add to `tests/integration/messaging_test.rs`:

```rust
use torrent_chat::db::Database;
use torrent_chat::db::queries;
use tempfile::NamedTempFile;

#[test]
fn test_full_conversation_flow() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    // Add a friend
    db.connection().execute(
        "INSERT INTO friends (onion_address, display_name, added_at, status)
         VALUES ('bob.onion', 'Bob', 1000, 'active')",
        [],
    ).unwrap();

    // Get friends
    let friends = queries::get_friends_with_unread(&db).unwrap();
    assert_eq!(friends.len(), 1);
    assert_eq!(friends[0].display_name, Some("Bob".to_string()));

    // Create conversation
    let conv_id = queries::get_or_create_conversation(&db, friends[0].friend_id).unwrap();

    // Send a message
    queries::store_outgoing_message(&db, conv_id, "me.onion", "Hello Bob!", "msg-001").unwrap();

    // Receive a reply
    queries::store_incoming_message(&db, conv_id, "bob.onion", "Hey there!", "msg-002").unwrap();

    // Check messages
    let messages = queries::get_messages(&db, conv_id, 50, 0).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "Hello Bob!");
    assert_eq!(messages[0].status, "sent");
    assert_eq!(messages[1].content, "Hey there!");
    assert_eq!(messages[1].status, "received");

    // Check unread (bob's message is unread)
    let friends = queries::get_friends_with_unread(&db).unwrap();
    assert_eq!(friends[0].unread_count, 1);

    // Mark read
    queries::mark_conversation_read(&db, conv_id).unwrap();
    let friends = queries::get_friends_with_unread(&db).unwrap();
    assert_eq!(friends[0].unread_count, 0);
}

#[test]
fn test_find_friend_for_incoming_message() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    // No friends yet
    assert_eq!(queries::find_friend_by_onion(&db, "unknown.onion").unwrap(), None);

    // Add friend
    db.connection().execute(
        "INSERT INTO friends (onion_address, display_name, added_at, status)
         VALUES ('alice.onion', 'Alice', 1000, 'active')",
        [],
    ).unwrap();

    // Now findable
    assert!(queries::find_friend_by_onion(&db, "alice.onion").unwrap().is_some());
}
```

**Step 2: Run all tests**

Run: `cargo test --lib`
Run: `cargo test --test '*'`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/
git commit -m "test: add integration tests for conversation flow

Tests full message send/receive cycle, unread tracking,
mark-as-read, and friend lookup for incoming messages."
```

---

## Plan Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Schema v5 migration (last_read_at) | schema.rs, connection.rs |
| 2 | Database queries module | queries.rs, db/mod.rs |
| 3 | Restructure AppState for chat | state.rs, ui/mod.rs |
| 4 | Add arboard dependency | Cargo.toml, ui/mod.rs |
| 5 | Render sidebar | sidebar.rs, ui/mod.rs |
| 6 | Render conversation view | conversation.rs, ui/mod.rs |
| 7 | Rewrite render_app | app_ui.rs, ui/mod.rs |
| 8 | Wire up main loop | main.rs, app.rs |
| 9 | Mouse support for clipboard | main.rs |
| 10 | Integration tests | tests/ |

**Dependencies:** Tasks 1-6 are independent and can be done in any order. Task 7 depends on 5+6. Task 8 depends on 2+3+7. Task 9 depends on 4+8. Task 10 depends on 8.
