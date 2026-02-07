//! Message Queue for Offline Delivery
//!
//! Manages queued messages that couldn't be delivered immediately.

use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use chrono::Utc;

/// Represents a queued message awaiting delivery
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedMessage {
    pub id: i64,
    pub to_onion: String,
    pub conversation_id: i64,
    pub encrypted_message: Vec<u8>,
    pub retry_count: i32,
    pub max_retries: i32,
}

/// Message queue for managing offline message delivery
pub struct MessageQueue;

impl MessageQueue {
    /// Create a new MessageQueue instance
    pub fn new() -> Self {
        MessageQueue
    }

    /// Add a message to the delivery queue
    ///
    /// Returns the queue ID of the newly inserted message
    pub fn enqueue(
        &self,
        db: &Database,
        to_onion: &str,
        conversation_id: i64,
        encrypted_message: &[u8],
    ) -> Result<i64> {
        let conn = db.connection();
        let now = Utc::now().timestamp();

        conn.execute(
            "INSERT INTO message_queue (to_onion, conversation_id, encrypted_message, created_at, retry_count, max_retries)
             VALUES (?1, ?2, ?3, ?4, 0, 50)",
            rusqlite::params![to_onion, conversation_id, encrypted_message, now],
        )
        .map_err(|e| TorrentChatError::Database(format!("Failed to enqueue message: {}", e)))?;

        let id = conn.last_insert_rowid();
        Ok(id)
    }

    /// Get all messages pending delivery to a specific peer
    pub fn get_queued(&self, db: &Database, to_onion: &str) -> Result<Vec<QueuedMessage>> {
        let conn = db.connection();
        let mut stmt = conn
            .prepare(
                "SELECT id, to_onion, conversation_id, encrypted_message, retry_count, max_retries
                 FROM message_queue
                 WHERE to_onion = ?1 AND retry_count < max_retries
                 ORDER BY created_at ASC",
            )
            .map_err(|e| TorrentChatError::Database(format!("Failed to prepare statement: {}", e)))?;

        let messages = stmt
            .query_map([to_onion], |row| {
                Ok(QueuedMessage {
                    id: row.get(0)?,
                    to_onion: row.get(1)?,
                    conversation_id: row.get(2)?,
                    encrypted_message: row.get(3)?,
                    retry_count: row.get(4)?,
                    max_retries: row.get(5)?,
                })
            })
            .map_err(|e| TorrentChatError::Database(format!("Failed to query messages: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| TorrentChatError::Database(format!("Failed to collect messages: {}", e)))?;

        Ok(messages)
    }

    /// Mark a message as successfully delivered and remove from queue
    pub fn remove(&self, db: &Database, id: i64) -> Result<()> {
        let conn = db.connection();
        conn.execute("DELETE FROM message_queue WHERE id = ?1", [id])
            .map_err(|e| {
                TorrentChatError::Database(format!("Failed to remove message from queue: {}", e))
            })?;
        Ok(())
    }

    /// Increment retry count for a failed delivery attempt
    pub fn increment_retry(&self, db: &Database, id: i64) -> Result<()> {
        let conn = db.connection();
        let now = Utc::now().timestamp();

        conn.execute(
            "UPDATE message_queue SET retry_count = retry_count + 1, last_retry_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )
        .map_err(|e| {
            TorrentChatError::Database(format!("Failed to increment retry count: {}", e))
        })?;

        Ok(())
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::CREATE_TABLES)
            .unwrap();
        // Need to create a conversation first
        conn.execute(
            "INSERT INTO friends (onion_address, added_at, status) VALUES ('test.onion', 0, 'active')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (friend_id, is_ephemeral, created_at) VALUES (1, 0, 0)",
            [],
        )
        .unwrap();
        Database::from_connection(conn)
    }

    #[test]
    fn test_enqueue_message() {
        let db = setup_test_db();
        let queue = MessageQueue::new();

        let id = queue
            .enqueue(&db, "test.onion", 1, b"encrypted data")
            .unwrap();

        assert!(id > 0);
    }

    #[test]
    fn test_get_queued_messages() {
        let db = setup_test_db();
        let queue = MessageQueue::new();

        // Enqueue two messages
        let id1 = queue
            .enqueue(&db, "test.onion", 1, b"message 1")
            .unwrap();
        let id2 = queue
            .enqueue(&db, "test.onion", 1, b"message 2")
            .unwrap();

        let messages = queue.get_queued(&db, "test.onion").unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, id1);
        assert_eq!(messages[0].to_onion, "test.onion");
        assert_eq!(messages[0].conversation_id, 1);
        assert_eq!(messages[0].encrypted_message, b"message 1");
        assert_eq!(messages[0].retry_count, 0);
        assert_eq!(messages[0].max_retries, 50);

        assert_eq!(messages[1].id, id2);
        assert_eq!(messages[1].encrypted_message, b"message 2");
    }

    #[test]
    fn test_get_queued_filters_by_onion() {
        let db = setup_test_db();
        let queue = MessageQueue::new();

        // Add another friend
        db.connection()
            .execute(
                "INSERT INTO friends (onion_address, added_at, status) VALUES ('other.onion', 0, 'active')",
                [],
            )
            .unwrap();

        queue.enqueue(&db, "test.onion", 1, b"message 1").unwrap();
        queue.enqueue(&db, "other.onion", 1, b"message 2").unwrap();

        let messages = queue.get_queued(&db, "test.onion").unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].to_onion, "test.onion");
        assert_eq!(messages[0].encrypted_message, b"message 1");
    }

    #[test]
    fn test_remove_message() {
        let db = setup_test_db();
        let queue = MessageQueue::new();

        let id = queue
            .enqueue(&db, "test.onion", 1, b"message 1")
            .unwrap();

        queue.remove(&db, id).unwrap();

        let messages = queue.get_queued(&db, "test.onion").unwrap();
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_increment_retry() {
        let db = setup_test_db();
        let queue = MessageQueue::new();

        let id = queue
            .enqueue(&db, "test.onion", 1, b"message 1")
            .unwrap();

        queue.increment_retry(&db, id).unwrap();

        let messages = queue.get_queued(&db, "test.onion").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].retry_count, 1);
    }

    #[test]
    fn test_max_retries_filters_messages() {
        let db = setup_test_db();
        let queue = MessageQueue::new();

        let id = queue
            .enqueue(&db, "test.onion", 1, b"message 1")
            .unwrap();

        // Increment retry count to max (50 times)
        for _ in 0..50 {
            queue.increment_retry(&db, id).unwrap();
        }

        // Message should no longer appear in get_queued
        let messages = queue.get_queued(&db, "test.onion").unwrap();
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_fifo_ordering() {
        let db = setup_test_db();
        let queue = MessageQueue::new();

        // Enqueue three messages
        let id1 = queue
            .enqueue(&db, "test.onion", 1, b"first")
            .unwrap();
        let id2 = queue
            .enqueue(&db, "test.onion", 1, b"second")
            .unwrap();
        let id3 = queue
            .enqueue(&db, "test.onion", 1, b"third")
            .unwrap();

        let messages = queue.get_queued(&db, "test.onion").unwrap();

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].id, id1);
        assert_eq!(messages[1].id, id2);
        assert_eq!(messages[2].id, id3);
    }
}
