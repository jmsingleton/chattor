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

    fn setup_test_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        // Add a test friend
        db.connection().execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
            [],
        ).unwrap();

        (db, temp)
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
        let (db, _temp) = setup_test_db();
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].display_name, Some("Alice".to_string()));
        assert_eq!(friends[0].unread_count, 0);
    }

    #[test]
    fn test_get_or_create_conversation() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();
        assert!(conv_id > 0);

        // Should return same conversation
        let conv_id2 = get_or_create_conversation(&db, 1).unwrap();
        assert_eq!(conv_id, conv_id2);
    }

    #[test]
    fn test_store_and_get_messages() {
        let (db, _temp) = setup_test_db();
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
        let (db, _temp) = setup_test_db();
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
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_outgoing_message(&db, conv_id, "me.onion", "Hello!", "msg-1").unwrap();
        update_message_status(&db, "msg-1", "queued").unwrap();

        let messages = get_messages(&db, conv_id, 50, 0).unwrap();
        assert_eq!(messages[0].status, "queued");
    }

    #[test]
    fn test_find_friend_by_onion() {
        let (db, _temp) = setup_test_db();
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
