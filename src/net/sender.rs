use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use crate::crypto::{SessionStore, SignalSession};
use crate::protocol::message::*;
use crate::tor::connection::TorConnection;
use crate::tor::client::TorClient;
use uuid::Uuid;
use std::sync::Arc;

/// Handles sending encrypted messages
pub struct MessageSender {
    db: Arc<Database>,
}

impl MessageSender {
    /// Create new message sender
    pub fn new(db: Arc<Database>) -> Self {
        MessageSender { db }
    }

    /// Prepare encrypted message (without sending)
    pub fn prepare_message(
        &self,
        from_onion: &str,
        to_onion: &str,
        content: &str,
    ) -> Result<TextMessage> {
        let store = SessionStore::new(&self.db);

        // Load session
        let mut session = store.load_session(to_onion)?
            .ok_or_else(|| TorrentChatError::Crypto("No session found".into()))?;

        // Create plaintext payload
        let payload = PlaintextPayload {
            content: content.to_string(),
            sent_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            message_type: "text".to_string(),
        };

        let plaintext = serde_json::to_vec(&payload)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to serialize: {}", e)))?;

        // Encrypt with Signal
        let (ciphertext, is_prekey) = session.encrypt(&plaintext)?;

        // Update session in database
        store.store_session(&session)?;

        // Create message envelope
        let message = TextMessage {
            from_onion: from_onion.to_string(),
            to_onion: to_onion.to_string(),
            signal_ciphertext: base64::encode(&ciphertext),
            signal_type: if is_prekey {
                SignalMessageType::PrekeyMessage
            } else {
                SignalMessageType::Message
            },
            timestamp: payload.sent_at,
            message_id: Uuid::new_v4(),
        };

        Ok(message)
    }

    /// Send encrypted message over Tor
    pub async fn send_message(
        &self,
        tor_client: &TorClient,
        from_onion: &str,
        to_onion: &str,
        content: &str,
    ) -> Result<Uuid> {
        let message = self.prepare_message(from_onion, to_onion, content)?;
        let message_id = message.message_id;

        // Connect to peer
        let mut conn = TorConnection::connect(tor_client, to_onion).await?;

        // Send message
        conn.send(&Message::TextMessage(message)).await?;

        Ok(message_id)
    }

    /// Handle received delivery receipt
    pub fn handle_delivery_receipt(&self, receipt: &DeliveryReceiptMessage) -> Result<()> {
        let conn = self.db.connection();

        conn.execute(
            "UPDATE messages SET status = 'delivered' WHERE message_id = ?1",
            [receipt.message_id.to_string()],
        ).map_err(|e| TorrentChatError::Database(format!("Failed to update status: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_send_encrypted_message() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        // Create session
        let bundle = crate::crypto::PreKeyBundle::generate().unwrap();
        let session = crate::crypto::SignalSession::from_prekey_bundle(
            "bob.onion".into(),
            &bundle
        ).unwrap();

        let store = crate::crypto::SessionStore::new(&db);
        store.store_session(&session).unwrap();

        // Create sender
        let sender = MessageSender::new(Arc::new(db));

        // Send would fail without real connection, but we can test preparation
        let result = sender.prepare_message(
            "alice.onion",
            "bob.onion",
            "Hello, Bob!"
        );

        assert!(result.is_ok());
    }
}
