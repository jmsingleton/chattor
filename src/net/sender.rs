use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use crate::crypto::SessionStore;
use crate::protocol::message::*;
use crate::tor::connection::TorConnection;
use crate::tor::client::TorClient;
use uuid::Uuid;
use std::sync::Arc;
use base64::Engine;

/// Handles sending encrypted messages
#[allow(dead_code)]
pub struct MessageSender {
    db: Arc<Database>,
}

#[allow(dead_code)]
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
            ephemeral_ttl: None,
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
            signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
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

        // Create real session
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let _bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let (bob_bundle, bob_private) = {
            let sig_id = libsignal_protocol::vxeddsa::gen_keypair();
            let sig_pub = libsignal_protocol::utils::decode_public_key(&sig_id.public).unwrap();
            crate::crypto::PreKeyBundle::generate_real(&sig_id.secret, &sig_pub).unwrap()
        };

        let session = crate::crypto::SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_identity,
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

    #[test]
    fn test_prepare_message_with_real_signal() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = Arc::new(crate::db::Database::open(temp_db.path()).unwrap());

        // Create real session
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let _bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let (bob_bundle, bob_private) = {
            let sig_id = libsignal_protocol::vxeddsa::gen_keypair();
            let sig_pub = libsignal_protocol::utils::decode_public_key(&sig_id.public).unwrap();
            crate::crypto::PreKeyBundle::generate_real(&sig_id.secret, &sig_pub).unwrap()
        };

        let session = crate::crypto::SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_identity,
        ).unwrap();

        let store = crate::crypto::SessionStore::new(&db);
        store.store_session(&session).unwrap();

        // Create sender
        let sender = MessageSender::new(db);

        // Prepare message
        let result = sender.prepare_message(
            "alice.onion",
            "bob.onion",
            "Hello Bob!",
        );

        assert!(result.is_ok());
        let message = result.unwrap();

        // Verify real encryption was used:
        // 1. Ciphertext should not contain plaintext
        assert!(!message.signal_ciphertext.contains("Hello Bob!"));

        // 2. Decode base64 and verify it's not the plaintext bytes
        let decoded_ciphertext = base64::engine::general_purpose::STANDARD.decode(&message.signal_ciphertext).unwrap();
        assert_ne!(decoded_ciphertext, b"Hello Bob!");

        // 3. Should be longer than plaintext due to encryption overhead
        assert!(decoded_ciphertext.len() > 12); // "Hello Bob!" is 10 bytes

        // 4. Should be PreKey message type for first message
        assert_eq!(message.signal_type, crate::protocol::message::SignalMessageType::PrekeyMessage);
    }
}
