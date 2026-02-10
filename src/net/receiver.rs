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
                // TODO(Phase 2b): Implement PreKey message handling with real crypto
                //
                // To properly handle PreKey messages without a pre-established session,
                // we need access to PreKeyPrivateMaterial, which requires:
                // 1. Persisting PreKey bundles and their private keys in the database
                // 2. Loading the correct PreKey bundle based on the message header
                // 3. Calling SignalSession::from_prekey_message_real() with the private keys
                //
                // For MVP, sessions are pre-established via friend request exchange,
                // so this code path should not be reached in normal operation.
                // If reached, it falls back to stub behavior.
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

    /// Create delivery receipt for message
    pub fn create_delivery_receipt(&self, message_id: uuid::Uuid) -> DeliveryReceiptMessage {
        DeliveryReceiptMessage {
            message_id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
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
            ephemeral_ttl: None,
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

    #[test]
    fn test_send_delivery_receipt() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        let receiver = MessageReceiver::new(Arc::new(db));

        let message_id = uuid::Uuid::new_v4();
        let receipt = receiver.create_delivery_receipt(message_id);

        assert_eq!(receipt.message_id, message_id);
        assert!(receipt.timestamp > 0);
    }

    #[test]
    fn test_decrypt_message_with_real_signal() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = Arc::new(crate::db::Database::open(temp_db.path()).unwrap());

        // Create real session for Alice (sender) and Bob (receiver)
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let (bob_bundle, bob_private) = crate::crypto::PreKeyBundle::generate_real(&bob_identity).unwrap();

        // Alice creates session with Bob
        let mut alice_session = crate::crypto::SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_identity,
        ).unwrap();

        // Alice encrypts a message
        let plaintext_content = "Hello Bob from Alice!";
        let payload = crate::protocol::message::PlaintextPayload {
            content: plaintext_content.to_string(),
            sent_at: 12345,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };
        let plaintext_bytes = serde_json::to_vec(&payload).unwrap();
        let (ciphertext, is_prekey) = alice_session.encrypt(&plaintext_bytes).unwrap();

        assert!(is_prekey); // First message should be PreKey type

        // Create TextMessage
        let message = crate::protocol::message::TextMessage {
            from_onion: "alice.onion".into(),
            to_onion: "bob.onion".into(),
            signal_ciphertext: base64::encode(&ciphertext),
            signal_type: crate::protocol::message::SignalMessageType::PrekeyMessage,
            timestamp: 12345,
            message_id: uuid::Uuid::new_v4(),
        };

        // Bob receives and decrypts
        let receiver = MessageReceiver::new(db.clone());

        // For PreKey message reception, Bob needs to create session from the message
        // This is a limitation: we need Bob's private keys available
        // For now, let's test with pre-established session

        // Bob creates his session from Alice's PreKey message
        let mut bob_session = crate::crypto::SignalSession::from_prekey_message_real(
            "alice.onion".into(),
            &ciphertext,
            &bob_bundle,
            &bob_private,
            &bob_identity,
        ).unwrap();

        // Store Bob's session
        let store = crate::crypto::SessionStore::new(&db);
        store.store_session(&bob_session).unwrap();

        // Now Bob can decrypt
        let result = receiver.decrypt_message(&message);
        assert!(result.is_ok());

        let decrypted = result.unwrap();
        assert_eq!(decrypted.content, plaintext_content);
        assert_eq!(decrypted.sent_at, 12345);
    }

    #[test]
    fn test_decrypt_regular_message_with_real_signal() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = Arc::new(crate::db::Database::open(temp_db.path()).unwrap());

        // Create real session for Alice and Bob
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let (bob_bundle, bob_private) = crate::crypto::PreKeyBundle::generate_real(&bob_identity).unwrap();

        // Alice creates session
        let mut alice_session = crate::crypto::SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_identity,
        ).unwrap();

        // Send first message (PreKey) to establish session
        let payload1 = crate::protocol::message::PlaintextPayload {
            content: "First message".to_string(),
            sent_at: 12345,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };
        let (ciphertext1, _) = alice_session.encrypt(&serde_json::to_vec(&payload1).unwrap()).unwrap();

        // Bob establishes his side
        let mut bob_session = crate::crypto::SignalSession::from_prekey_message_real(
            "alice.onion".into(),
            &ciphertext1,
            &bob_bundle,
            &bob_private,
            &bob_identity,
        ).unwrap();
        bob_session.decrypt(&ciphertext1).unwrap(); // Process first message

        let store = crate::crypto::SessionStore::new(&db);
        store.store_session(&bob_session).unwrap();

        // Now send a second message (regular Message type, not PreKey)
        let plaintext_content = "Second message";
        let payload2 = crate::protocol::message::PlaintextPayload {
            content: plaintext_content.to_string(),
            sent_at: 12346,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };
        let (ciphertext2, is_prekey) = alice_session.encrypt(&serde_json::to_vec(&payload2).unwrap()).unwrap();

        assert!(!is_prekey); // Second message should NOT be PreKey type

        let message = crate::protocol::message::TextMessage {
            from_onion: "alice.onion".into(),
            to_onion: "bob.onion".into(),
            signal_ciphertext: base64::encode(&ciphertext2),
            signal_type: crate::protocol::message::SignalMessageType::Message,
            timestamp: 12346,
            message_id: uuid::Uuid::new_v4(),
        };

        // Bob decrypts
        let receiver = MessageReceiver::new(db);
        let result = receiver.decrypt_message(&message);
        assert!(result.is_ok());

        let decrypted = result.unwrap();
        assert_eq!(decrypted.content, plaintext_content);
    }
}
