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
}
