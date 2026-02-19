//! Message Queue for Offline Delivery
//!
//! General-purpose outgoing message queue that serializes protocol Messages
//! as JSON and persists them in the database for reliable delivery.

use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use crate::protocol::message::Message;
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a queued message awaiting delivery
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub id: i64,
    pub peer_onion: String,
    pub message: Message,
    pub retry_count: i64,
    #[allow(dead_code)]
    pub priority: String,
    pub created_at: i64,
}

/// Message queue for managing offline message delivery
#[derive(Default)]
pub struct MessageQueue;

impl MessageQueue {
    /// Create a new MessageQueue instance
    pub fn new() -> Self {
        MessageQueue
    }

    /// Add a message to the delivery queue
    ///
    /// Serializes the message to JSON and inserts it into the message_queue table.
    /// Returns the queue ID of the newly inserted row.
    pub fn enqueue(
        &self,
        db: &Database,
        peer_onion: &str,
        message: &Message,
        priority: &str,
    ) -> Result<i64> {
        let conn = db.connection();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let message_json = serde_json::to_string(message).map_err(|e| {
            TorrentChatError::Database(format!("Failed to serialize message: {}", e))
        })?;

        conn.execute(
            "INSERT INTO message_queue (peer_onion, message_json, priority, retry_count, next_retry_at, created_at, status)
             VALUES (?1, ?2, ?3, 0, ?4, ?5, 'pending')",
            rusqlite::params![peer_onion, message_json, priority, now, now],
        )
        .map_err(|e| TorrentChatError::Database(format!("Failed to enqueue message: {}", e)))?;

        let id = conn.last_insert_rowid();
        Ok(id)
    }

    /// Get all pending messages that are due for delivery
    ///
    /// Returns messages where status = 'pending' and next_retry_at <= now,
    /// ordered by priority (high before normal), then created_at ASC (FIFO).
    pub fn get_pending_messages(
        &self,
        db: &Database,
        now: i64,
    ) -> Result<Vec<QueuedMessage>> {
        let conn = db.connection();
        let mut stmt = conn
            .prepare(
                "SELECT id, peer_onion, message_json, retry_count, priority, created_at
                 FROM message_queue
                 WHERE status = 'pending' AND next_retry_at <= ?1
                 ORDER BY CASE priority WHEN 'high' THEN 0 ELSE 1 END, created_at ASC",
            )
            .map_err(|e| {
                TorrentChatError::Database(format!("Failed to prepare statement: {}", e))
            })?;

        // Collect raw rows first, then deserialize outside of query_map
        // to avoid issues with serde_json errors inside rusqlite callbacks.
        let raw_rows: Vec<(i64, String, String, i64, String, i64)> = stmt
            .query_map([now], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(|e| {
                TorrentChatError::Database(format!("Failed to query messages: {}", e))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                TorrentChatError::Database(format!("Failed to collect messages: {}", e))
            })?;

        let mut messages = Vec::with_capacity(raw_rows.len());
        for (id, peer_onion, message_json, retry_count, priority, created_at) in raw_rows {
            let message: Message = serde_json::from_str(&message_json).map_err(|e| {
                TorrentChatError::Database(format!(
                    "Failed to deserialize message_json for queue id {}: {}",
                    id, e
                ))
            })?;
            messages.push(QueuedMessage {
                id,
                peer_onion,
                message,
                retry_count,
                priority,
                created_at,
            });
        }

        Ok(messages)
    }

    /// Mark a queued message as successfully delivered
    pub fn mark_delivered(&self, db: &Database, id: i64) -> Result<()> {
        let conn = db.connection();
        conn.execute(
            "UPDATE message_queue SET status = 'delivered' WHERE id = ?1",
            [id],
        )
        .map_err(|e| {
            TorrentChatError::Database(format!("Failed to mark message delivered: {}", e))
        })?;
        Ok(())
    }

    /// Mark a queued message as permanently failed
    pub fn mark_failed(&self, db: &Database, id: i64) -> Result<()> {
        let conn = db.connection();
        conn.execute(
            "UPDATE message_queue SET status = 'failed' WHERE id = ?1",
            [id],
        )
        .map_err(|e| {
            TorrentChatError::Database(format!("Failed to mark message failed: {}", e))
        })?;
        Ok(())
    }

    /// Schedule a retry for a queued message
    ///
    /// Increments the retry_count and sets the next_retry_at timestamp.
    pub fn schedule_retry(
        &self,
        db: &Database,
        id: i64,
        next_retry_at: i64,
    ) -> Result<()> {
        let conn = db.connection();
        conn.execute(
            "UPDATE message_queue SET retry_count = retry_count + 1, next_retry_at = ?1 WHERE id = ?2",
            rusqlite::params![next_retry_at, id],
        )
        .map_err(|e| {
            TorrentChatError::Database(format!("Failed to schedule retry: {}", e))
        })?;
        Ok(())
    }
}

/// 24 hours in seconds
const MESSAGE_EXPIRY_SECS: i64 = 86400;

/// Maximum retry delay in seconds (15 minutes)
const MAX_RETRY_DELAY_SECS: i64 = 900;

/// Base retry delay in seconds
const BASE_RETRY_DELAY_SECS: i64 = 30;

/// Compute the next retry timestamp using exponential backoff.
///
/// Returns `None` if the message has expired (older than 24 hours).
/// Formula: min(30 * 2^retry_count, 900) seconds from now.
pub fn compute_next_retry(retry_count: i64, created_at: i64, now: i64) -> Option<i64> {
    // Check 24h expiry
    if now - created_at >= MESSAGE_EXPIRY_SECS {
        return None;
    }

    let delay = BASE_RETRY_DELAY_SECS
        .checked_shl(retry_count as u32)
        .unwrap_or(MAX_RETRY_DELAY_SECS)
        .min(MAX_RETRY_DELAY_SECS);

    Some(now + delay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::protocol::message::{FriendRequestMessage, Message};
    use tempfile::NamedTempFile;

    fn setup_test_db() -> (NamedTempFile, Database) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        // No need to create friends/conversations since the new queue has no foreign keys
        (temp, db)
    }

    fn create_test_message() -> Message {
        Message::FriendRequest(FriendRequestMessage {
            from_onion: "test.onion".to_string(),
            from_friendcode: "test-code".to_string(),
            timestamp: 123456,
            signature: "sig".to_string(),
        })
    }

    #[test]
    fn test_enqueue_message() {
        let (_temp, db) = setup_test_db();
        let queue = MessageQueue::new();
        let msg = create_test_message();

        let id = queue.enqueue(&db, "peer.onion", &msg, "normal").unwrap();
        assert!(id > 0);

        // Verify the row exists in the database
        let count: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM message_queue WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_get_pending_messages() {
        let (_temp, db) = setup_test_db();
        let queue = MessageQueue::new();
        let msg = create_test_message();

        // Enqueue a message (created_at and next_retry_at will be "now")
        let id = queue.enqueue(&db, "peer.onion", &msg, "normal").unwrap();

        // Retrieve with a timestamp far in the future so it's definitely due
        let future = i64::MAX;
        let pending = queue.get_pending_messages(&db, future).unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id);
        assert_eq!(pending[0].peer_onion, "peer.onion");
        assert_eq!(pending[0].retry_count, 0);
        assert_eq!(pending[0].priority, "normal");
    }

    #[test]
    fn test_mark_delivered() {
        let (_temp, db) = setup_test_db();
        let queue = MessageQueue::new();
        let msg = create_test_message();

        let id = queue.enqueue(&db, "peer.onion", &msg, "normal").unwrap();

        // Mark as delivered
        queue.mark_delivered(&db, id).unwrap();

        // Verify status changed
        let status: String = db
            .connection()
            .query_row(
                "SELECT status FROM message_queue WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "delivered");

        // Should no longer appear in pending
        let pending = queue.get_pending_messages(&db, i64::MAX).unwrap();
        assert_eq!(pending.len(), 0);
    }

    #[test]
    fn test_schedule_retry() {
        let (_temp, db) = setup_test_db();
        let queue = MessageQueue::new();
        let msg = create_test_message();

        let id = queue.enqueue(&db, "peer.onion", &msg, "normal").unwrap();

        // Schedule a retry with a future timestamp
        let future_time = 9999999999i64;
        queue.schedule_retry(&db, id, future_time).unwrap();

        // Verify retry_count incremented
        let retry_count: i64 = db
            .connection()
            .query_row(
                "SELECT retry_count FROM message_queue WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retry_count, 1);

        // Verify next_retry_at updated
        let next_retry: i64 = db
            .connection()
            .query_row(
                "SELECT next_retry_at FROM message_queue WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(next_retry, future_time);
    }

    #[test]
    fn test_mark_failed() {
        let (_temp, db) = setup_test_db();
        let queue = MessageQueue::new();
        let msg = create_test_message();

        let id = queue.enqueue(&db, "peer.onion", &msg, "normal").unwrap();

        // Mark as failed
        queue.mark_failed(&db, id).unwrap();

        // Verify status changed
        let status: String = db
            .connection()
            .query_row(
                "SELECT status FROM message_queue WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "failed");

        // Should no longer appear in pending
        let pending = queue.get_pending_messages(&db, i64::MAX).unwrap();
        assert_eq!(pending.len(), 0);
    }

    #[test]
    fn test_priority_ordering() {
        let (_temp, db) = setup_test_db();
        let queue = MessageQueue::new();
        let msg = create_test_message();

        // Enqueue normal priority first, then high priority
        let id_normal = queue.enqueue(&db, "peer.onion", &msg, "normal").unwrap();
        let id_high = queue.enqueue(&db, "peer.onion", &msg, "high").unwrap();

        let pending = queue.get_pending_messages(&db, i64::MAX).unwrap();

        assert_eq!(pending.len(), 2);
        // High priority messages come first
        assert_eq!(pending[0].id, id_high);
        assert_eq!(pending[0].priority, "high");
        assert_eq!(pending[1].id, id_normal);
        assert_eq!(pending[1].priority, "normal");
    }

    #[test]
    fn test_pending_only_returns_due_messages() {
        let (_temp, db) = setup_test_db();
        let queue = MessageQueue::new();
        let msg = create_test_message();

        // Enqueue a message
        let id = queue.enqueue(&db, "peer.onion", &msg, "normal").unwrap();

        // Schedule a retry far in the future
        let far_future = 9999999999i64;
        queue.schedule_retry(&db, id, far_future).unwrap();

        // Query with a timestamp before the retry time -- should get nothing
        let pending = queue.get_pending_messages(&db, 1000).unwrap();
        assert_eq!(pending.len(), 0);

        // Query with a timestamp at or after the retry time -- should get the message
        let pending = queue.get_pending_messages(&db, far_future).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id);
    }

    #[test]
    fn test_compute_next_retry_backoff_schedule() {
        let now = 1000000;
        let created_at = now - 60; // created 60s ago

        // retry 0 → 30s delay
        assert_eq!(compute_next_retry(0, created_at, now), Some(now + 30));
        // retry 1 → 60s delay
        assert_eq!(compute_next_retry(1, created_at, now), Some(now + 60));
        // retry 2 → 120s delay
        assert_eq!(compute_next_retry(2, created_at, now), Some(now + 120));
        // retry 3 → 240s delay
        assert_eq!(compute_next_retry(3, created_at, now), Some(now + 240));
        // retry 4 → 480s delay
        assert_eq!(compute_next_retry(4, created_at, now), Some(now + 480));
        // retry 5 → 900s (capped at 15 min)
        assert_eq!(compute_next_retry(5, created_at, now), Some(now + 900));
        // retry 10 → still 900s (cap holds)
        assert_eq!(compute_next_retry(10, created_at, now), Some(now + 900));
    }

    #[test]
    fn test_compute_next_retry_expires_after_24h() {
        let now = 1000000;
        let created_at = now - 86401; // created 24h + 1s ago

        assert_eq!(compute_next_retry(0, created_at, now), None);
    }

    #[test]
    fn test_compute_next_retry_just_within_window() {
        let now = 1000000;
        let created_at = now - 86399; // created 24h - 1s ago

        assert!(compute_next_retry(0, created_at, now).is_some());
    }
}
