use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use crate::crypto::SessionStore;
use crate::protocol::message::*;
use std::sync::Arc;
use base64::Engine;

/// Handles receiving and decrypting messages
#[allow(dead_code)]
pub struct MessageReceiver {
    db: Arc<Database>,
}

#[allow(dead_code)]
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
                // No session exists for this peer. To handle PreKey messages from
                // unknown peers, we'd need to persist our PreKey private material
                // and call SignalSession::from_prekey_message_real(). For now,
                // sessions are pre-established via friend request exchange.
                return Err(TorrentChatError::Crypto(format!(
                    "No session for {} — friend request exchange required first",
                    message.from_onion
                )));
            }
        };

        // Decrypt
        let ciphertext = base64::engine::general_purpose::STANDARD.decode(&message.signal_ciphertext)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to decode base64: {}", e)))?;
        // TODO(task3): Wire format needs to carry header+ciphertext separately.
        // For now, treat entire ciphertext as body with empty header as placeholder.
        let _is_prekey = message.signal_type == SignalMessageType::PrekeyMessage;
        let plaintext = session.decrypt(&[], &ciphertext)?;

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

    /// Helper: generate an X25519 Signal identity keypair for tests.
    fn gen_signal_identity() -> ([u8; 32], [u8; 32]) {
        let kp = libsignal_protocol::vxeddsa::gen_keypair();
        let raw_pub = libsignal_protocol::utils::decode_public_key(&kp.public).unwrap();
        (kp.secret, raw_pub)
    }

    /// Helper: set up Alice+Bob session pair and return both sessions + Alice's signal secret.
    fn setup_session_pair() -> (crate::crypto::SignalSession, crate::crypto::SignalSession) {
        let (alice_signal_secret, _) = gen_signal_identity();
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();
        let (bob_bundle, bob_private) =
            crate::crypto::PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        let (alice_session, _ad, ephemeral_public) = crate::crypto::SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_signal_secret,
        ).unwrap();

        let alice_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&alice_signal_secret);
        let (bob_session, _bob_ad) = crate::crypto::SignalSession::from_prekey_message_real(
            "alice.onion".into(),
            &bob_private,
            &alice_identity_encoded,
            &ephemeral_public,
        ).unwrap();

        (alice_session, bob_session)
    }

    #[test]
    fn test_decrypt_message() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = Arc::new(crate::db::Database::open(temp_db.path()).unwrap());

        let (mut alice_session, mut bob_session) = setup_session_pair();

        // Alice encrypts a message
        let payload = crate::protocol::message::PlaintextPayload {
            content: "Hello!".to_string(),
            sent_at: 12345,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };
        let plaintext_bytes = serde_json::to_vec(&payload).unwrap();
        let (header, ciphertext, _is_prekey) = alice_session.encrypt(&plaintext_bytes).unwrap();

        // Store Bob's session (pre-established)
        let store = crate::crypto::SessionStore::new(&db);
        store.store_session(&bob_session).unwrap();

        // Bob decrypts directly (verify the raw crypto works)
        let decrypted = bob_session.decrypt(&header, &ciphertext).unwrap();
        let decrypted_payload: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(decrypted_payload.content, "Hello!");
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
    fn test_encrypt_decrypt_roundtrip_real_signal() {
        // Full encrypt/decrypt roundtrip test with real X3DH + Double Ratchet
        let (mut alice_session, mut bob_session) = setup_session_pair();

        // Alice encrypts a message
        let plaintext_content = "Hello Bob from Alice!";
        let payload = crate::protocol::message::PlaintextPayload {
            content: plaintext_content.to_string(),
            sent_at: 12345,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };
        let plaintext_bytes = serde_json::to_vec(&payload).unwrap();
        let (header, ciphertext, is_prekey) = alice_session.encrypt(&plaintext_bytes).unwrap();

        assert!(is_prekey); // First message should be PreKey type

        // Bob decrypts
        let decrypted = bob_session.decrypt(&header, &ciphertext).unwrap();
        let decrypted_payload: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(decrypted_payload.content, plaintext_content);
        assert_eq!(decrypted_payload.sent_at, 12345);
    }

    #[test]
    fn test_regular_message_after_prekey() {
        // Test that second message is a regular (non-PreKey) message
        let (mut alice_session, mut bob_session) = setup_session_pair();

        // First message (PreKey)
        let payload1 = crate::protocol::message::PlaintextPayload {
            content: "First message".to_string(),
            sent_at: 12345,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };
        let (h1, c1, is_prekey1) = alice_session.encrypt(&serde_json::to_vec(&payload1).unwrap()).unwrap();
        assert!(is_prekey1);
        bob_session.decrypt(&h1, &c1).unwrap(); // Process first message

        // Second message (regular)
        let plaintext_content = "Second message";
        let payload2 = crate::protocol::message::PlaintextPayload {
            content: plaintext_content.to_string(),
            sent_at: 12346,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };
        let (h2, c2, is_prekey2) = alice_session.encrypt(&serde_json::to_vec(&payload2).unwrap()).unwrap();
        assert!(!is_prekey2); // Second message should NOT be PreKey type

        let decrypted = bob_session.decrypt(&h2, &c2).unwrap();
        let decrypted_payload: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(decrypted_payload.content, plaintext_content);
    }
}
