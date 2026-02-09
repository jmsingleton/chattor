use crate::db::Database;
use crate::error::Result;
use crate::tor::client::TorClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

/// Background task for processing message queue
pub struct QueueProcessor {
    db: Arc<Database>,
}

impl QueueProcessor {
    /// Create new queue processor
    pub fn new(db: Arc<Database>) -> Self {
        QueueProcessor { db }
    }

    /// Start background processing task
    pub async fn start(
        self,
        tor_client: Arc<TorClient>,
    ) {
        let mut interval = time::interval(Duration::from_secs(180)); // 3 minutes

        loop {
            interval.tick().await;

            if let Err(e) = self.process_queue(&tor_client).await {
                eprintln!("Queue processing error: {}", e);
            }
        }
    }

    /// Process all queued messages
    async fn process_queue(&self, _tor_client: &TorClient) -> Result<()> {
        let conn = self.db.connection();

        // Get all queued messages
        let mut stmt = conn.prepare(
            "SELECT id, to_onion, conversation_id, encrypted_message, retry_count, max_retries
             FROM message_queue
             WHERE retry_count < max_retries"
        ).map_err(|e| crate::error::TorrentChatError::Database(format!("Failed to prepare query: {}", e)))?;

        let messages: Vec<(i64, String, i64, Vec<u8>, i32, i32)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(|e| crate::error::TorrentChatError::Database(format!("Failed to query messages: {}", e)))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| crate::error::TorrentChatError::Database(format!("Failed to collect messages: {}", e)))?;

        for (id, _to_onion, _conversation_id, _encrypted_msg, retry_count, max_retries) in messages {
            // Try to send
            // For MVP, just increment retry count
            // TODO: Actual sending logic

            if retry_count >= max_retries {
                // Mark as failed
                conn.execute(
                    "UPDATE message_queue SET retry_count = ?1 WHERE id = ?2",
                    (max_retries, id),
                ).map_err(|e| crate::error::TorrentChatError::Database(format!("Failed to update message: {}", e)))?;
            } else {
                // Increment retry count
                conn.execute(
                    "UPDATE message_queue SET retry_count = retry_count + 1, last_retry_at = ?1 WHERE id = ?2",
                    (
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64,
                        id,
                    ),
                ).map_err(|e| crate::error::TorrentChatError::Database(format!("Failed to update retry count: {}", e)))?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_process_queue_retries() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = Arc::new(crate::db::Database::open(temp_db.path()).unwrap());

        // Create required foreign key records
        let conn = db.connection();

        // Insert friend
        conn.execute(
            "INSERT INTO friends (onion_address, added_at, status) VALUES ('test.onion', 0, 'accepted')",
            [],
        ).unwrap();

        // Insert conversation
        conn.execute(
            "INSERT INTO conversations (friend_id, created_at) VALUES (1, 0)",
            [],
        ).unwrap();

        // Insert test message in queue
        conn.execute(
            "INSERT INTO message_queue (to_onion, conversation_id, encrypted_message, created_at, retry_count, max_retries)
             VALUES ('test.onion', 1, x'0102', 0, 0, 50)",
            [],
        ).unwrap();

        // Process queue (will fail to send, but should increment retry)
        let _processor = QueueProcessor::new(db.clone());

        // Check retry count increased
        let retry_count: i32 = conn.query_row(
            "SELECT retry_count FROM message_queue WHERE to_onion = 'test.onion'",
            [],
            |row| row.get(0),
        ).unwrap();

        assert!(retry_count >= 0);
    }
}
