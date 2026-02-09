use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use crate::crypto::{SessionStore, SignalSession};
use crate::protocol::message::*;
use std::sync::Arc;

/// Handles receiving and decrypting messages
pub struct MessageReceiver {
    db: Arc<Database>,
}

impl MessageReceiver {
    /// Create new message receiver
    pub fn new(db: Arc<Database>) -> Self {
        MessageReceiver { db }
    }

    /// Decrypt received message
    pub fn decrypt_message(&self, message: &TextMessage) -> Result<PlaintextPayload> {
        let store = SessionStore::new(&self.db);

        // Load or create session
        let mut session = match store.load_session(&message.from_onion)? {
            Some(s) => s,
            None => {
                // If PreKey message, initialize session
                if matches!(message.signal_type, SignalMessageType::PrekeyMessage) {
                    let ciphertext = base64::decode(&message.signal_ciphertext)
                        .map_err(|e| TorrentChatError::Crypto(format!("Failed to decode base64: {}", e)))?;
                    SignalSession::from_prekey_message(
                        message.from_onion.clone(),
                        &ciphertext
                    )?
                } else {
                    return Err(TorrentChatError::Crypto("No session and not PreKey message".into()));
                }
            }
        };

        // Decrypt
        let ciphertext = base64::decode(&message.signal_ciphertext)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to decode base64: {}", e)))?;
        let plaintext = session.decrypt(&ciphertext)?;

        // Update session
        store.store_session(&session)?;

        // Deserialize payload
        let payload: PlaintextPayload = serde_json::from_slice(&plaintext)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to parse payload: {}", e)))?;

        Ok(payload)
    }

    /// Store decrypted message in database
    pub fn store_message(
        &self,
        message: &TextMessage,
        payload: &PlaintextPayload,
        conversation_id: i64,
    ) -> Result<()> {
        let conn = self.db.connection();

        conn.execute(
            "INSERT INTO messages (message_id, conversation_id, sender_onion, content, timestamp, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'delivered')",
            (
                message.message_id.to_string(),
                conversation_id,
                &message.from_onion,
                &payload.content,
                payload.sent_at,
            ),
        ).map_err(|e| TorrentChatError::Database(format!("Failed to store message: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_decrypt_message() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        // Create session
        let bundle = crate::crypto::PreKeyBundle::generate().unwrap();
        let session = crate::crypto::SignalSession::from_prekey_bundle(
            "alice.onion".into(),
            &bundle
        ).unwrap();

        let store = crate::crypto::SessionStore::new(&db);
        store.store_session(&session).unwrap();

        // Create receiver
        let receiver = MessageReceiver::new(Arc::new(db));

        // Create test message (plaintext for MVP)
        let payload = crate::protocol::message::PlaintextPayload {
            content: "Hello!".to_string(),
            sent_at: 12345,
            message_type: "text".to_string(),
        };
        let ciphertext_bytes = serde_json::to_vec(&payload).unwrap();
        let ciphertext = base64::encode(&ciphertext_bytes);

        let message = crate::protocol::message::TextMessage {
            from_onion: "alice.onion".into(),
            to_onion: "bob.onion".into(),
            signal_ciphertext: ciphertext,
            signal_type: crate::protocol::message::SignalMessageType::Message,
            timestamp: 12345,
            message_id: uuid::Uuid::new_v4(),
        };

        let result = receiver.decrypt_message(&message);
        assert!(result.is_ok());

        let decrypted = result.unwrap();
        assert_eq!(decrypted.content, "Hello!");
    }
}
