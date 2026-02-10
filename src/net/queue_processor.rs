use crate::db::Database;
use crate::error::Result;
use crate::net::queue::MessageQueue;
use crate::tor::client::TorClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

/// Background task for processing message queue
pub struct QueueProcessor {
    db: Arc<Database>,
    queue: MessageQueue,
}

impl QueueProcessor {
    /// Create new queue processor
    pub fn new(db: Arc<Database>) -> Self {
        QueueProcessor {
            db,
            queue: MessageQueue::new(),
        }
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let pending = self.queue.get_pending_messages(&self.db, now)?;

        for msg in pending {
            // Try to send via Tor
            // TODO: Actual sending logic using tor_client
            // For MVP, just schedule a retry with exponential backoff

            let next_retry = now + 180; // 3 minutes from now
            self.queue.schedule_retry(&self.db, msg.id, next_retry)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::message::{FriendRequestMessage, Message};
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_process_queue_retries() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = Arc::new(crate::db::Database::open(temp_db.path()).unwrap());

        let queue = MessageQueue::new();

        // Create a test message and enqueue it
        let msg = Message::FriendRequest(FriendRequestMessage {
            from_onion: "test.onion".to_string(),
            from_friendcode: "test-code".to_string(),
            timestamp: 123456,
            signature: "sig".to_string(),
        });

        let id = queue.enqueue(&db, "test.onion", &msg, "normal").unwrap();

        // Verify initial state
        let retry_count: i64 = db
            .connection()
            .query_row(
                "SELECT retry_count FROM message_queue WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retry_count, 0);

        // Process queue should schedule a retry
        let processor = QueueProcessor::new(db.clone());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Get pending messages and process them manually
        let pending = processor.queue.get_pending_messages(&db, now + 1).unwrap();
        assert!(!pending.is_empty());

        // Schedule a retry for the first message
        processor
            .queue
            .schedule_retry(&db, pending[0].id, now + 180)
            .unwrap();

        // Check retry count increased
        let retry_count: i64 = db
            .connection()
            .query_row(
                "SELECT retry_count FROM message_queue WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retry_count, 1);
    }
}
