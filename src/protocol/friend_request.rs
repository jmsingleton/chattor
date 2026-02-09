use crate::error::{Result, TorrentChatError};
use crate::crypto::{IdentityKeypair, PreKeyBundle};
use crate::protocol::message::*;
use crate::db::Database;

/// Handles friend request protocol
pub struct FriendRequestHandler {
    db: Database,
}

impl FriendRequestHandler {
    /// Create new handler
    pub fn new(db: Database) -> Self {
        FriendRequestHandler { db }
    }

    /// Create friend request message
    pub fn create_request(
        identity: &IdentityKeypair,
        own_onion: &str,
        friend_code: &str,
    ) -> Result<FriendRequestMessage> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Create message data to sign
        let data = format!("{}{}{}",own_onion, friend_code, timestamp);

        // Sign with identity key
        let signature = identity.sign(data.as_bytes());
        let signature_base64 = base64::encode(&signature.to_bytes());

        Ok(FriendRequestMessage {
            from_onion: own_onion.to_string(),
            from_friendcode: friend_code.to_string(),
            timestamp,
            signature: signature_base64,
        })
    }

    /// Validate received friend request
    pub fn validate_request(&self, request: &FriendRequestMessage) -> Result<bool> {
        // Check timestamp (within 5 minutes)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let age = (now - request.timestamp).abs();
        if age > 300 {
            return Ok(false);
        }

        // TODO: Verify signature using public key from .onion
        // For MVP, accept all requests

        Ok(true)
    }

    /// Store friend request in database
    pub fn store_request(&self, request: &FriendRequestMessage) -> Result<i64> {
        let conn = self.db.connection();

        conn.execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status)
             VALUES (?1, ?2, ?3, 'pending')",
            (
                &request.from_onion,
                &request.from_friendcode,
                request.timestamp,
            ),
        ).map_err(|e| TorrentChatError::Database(format!("Failed to store request: {}", e)))?;

        let id = conn.last_insert_rowid();
        Ok(id)
    }

    /// Create friend request accept message with PreKey bundle
    pub fn create_accept_message(
        &self,
        identity: &IdentityKeypair,
        own_onion: &str,
        peer_onion: &str,
    ) -> Result<FriendRequestAcceptMessage> {
        // Generate PreKey bundle
        let bundle = PreKeyBundle::generate()?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Sign message
        let data = format!("{}{}{}", own_onion, peer_onion, timestamp);
        let signature = identity.sign(data.as_bytes());
        let signature_base64 = base64::encode(&signature.to_bytes());

        // Serialize bundle to JSON then base64
        let bundle_json = serde_json::to_string(&bundle)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to serialize bundle: {}", e)))?;

        Ok(FriendRequestAcceptMessage {
            from_onion: own_onion.to_string(),
            to_onion: peer_onion.to_string(),
            signal_prekey_bundle: bundle_json,
            timestamp,
            signature: signature_base64,
        })
    }

    /// Handle accept message - initialize session
    pub fn handle_accept(&self, accept: &FriendRequestAcceptMessage) -> Result<()> {
        use crate::crypto::{SignalSession, SessionStore};

        // Deserialize PreKey bundle from JSON
        let bundle: PreKeyBundle = serde_json::from_str(&accept.signal_prekey_bundle)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to parse bundle: {}", e)))?;

        // Initialize Signal session from PreKey bundle
        let session = SignalSession::from_prekey_bundle(
            accept.from_onion.clone(),
            &bundle
        )?;

        // Store session
        let store = SessionStore::new(&self.db);
        store.store_session(&session)?;

        // Add friend to database
        let conn = self.db.connection();
        conn.execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES (?1, ?2, ?3, 'active')",
            (
                &accept.from_onion,
                &accept.from_onion[..10], // Use first 10 chars as name
                accept.timestamp,
            ),
        ).map_err(|e| TorrentChatError::Database(format!("Failed to add friend: {}", e)))?;

        Ok(())
    }

    /// Accept friend request
    pub fn accept_request(&self, request_id: i64, identity: &IdentityKeypair) -> Result<FriendRequestAcceptMessage> {
        let conn = self.db.connection();

        // Get request details
        let (from_onion, own_onion): (String, String) = conn.query_row(
            "SELECT from_onion, 'bob.onion' FROM friend_requests WHERE id = ?1",
            [request_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(|e| TorrentChatError::Database(format!("Request not found: {}", e)))?;

        // Update request status
        conn.execute(
            "UPDATE friend_requests SET status = 'accepted' WHERE id = ?1",
            [request_id],
        ).map_err(|e| TorrentChatError::Database(format!("Failed to update request: {}", e)))?;

        // Add friend
        conn.execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES (?1, ?2, ?3, 'active')",
            (
                &from_onion,
                &from_onion[..10],
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            ),
        ).map_err(|e| TorrentChatError::Database(format!("Failed to add friend: {}", e)))?;

        // Create accept message
        self.create_accept_message(identity, &own_onion, &from_onion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_friend_request() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let friend_code = "happy-1234-tiger-5678";
        let onion = "test.onion";

        let request = FriendRequestHandler::create_request(
            &identity,
            onion,
            friend_code
        ).unwrap();

        assert_eq!(request.from_onion, onion);
        assert_eq!(request.from_friendcode, friend_code);
        assert!(request.signature.len() > 0);
        assert!(request.timestamp > 0);
    }

    #[test]
    fn test_accept_friend_request() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        let handler = FriendRequestHandler::new(db);
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();

        let accept = handler.create_accept_message(
            &identity,
            "bob.onion",
            "alice.onion"
        ).unwrap();

        assert_eq!(accept.from_onion, "bob.onion");
        assert_eq!(accept.to_onion, "alice.onion");
        assert!(accept.signal_prekey_bundle.len() > 0);
        assert!(accept.signature.len() > 0);
    }
}
