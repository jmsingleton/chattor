use crate::error::{Result, ChattorError};
use crate::crypto::{IdentityKeypair, PreKeyBundle};
use crate::protocol::message::*;
use crate::db::Database;
use base64::Engine;

/// Handles friend request protocol
#[allow(dead_code)]
pub struct FriendRequestHandler {
    db: Database,
}

#[allow(dead_code)]
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
        let signature_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

        Ok(FriendRequestMessage {
            from_onion: own_onion.to_string(),
            from_friendcode: friend_code.to_string(),
            timestamp,
            signature: signature_base64,
            ed25519_pubkey: Some(identity.public_key_base64()),
        })
    }

    /// Validate received friend request using TOFU — verify the Ed25519
    /// signature against the pubkey included in the message itself.
    pub fn validate_request(request: &FriendRequestMessage) -> Result<bool> {
        // Require Ed25519 pubkey for verification
        let pubkey_b64 = match &request.ed25519_pubkey {
            Some(pk) => pk,
            None => {
                tracing::warn!("Friend request from {} missing Ed25519 pubkey, rejecting", request.from_onion);
                return Ok(false);
            }
        };

        // Check timestamp (within 5 minutes)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let age = (now - request.timestamp).abs();
        if age > 300 {
            return Ok(false);
        }

        // Decode public key from base64
        let pubkey_bytes = match base64::engine::general_purpose::STANDARD.decode(pubkey_b64) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false),
        };
        let pubkey_array: [u8; 32] = match pubkey_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return Ok(false),
        };

        // Reconstruct the signed data
        let data = format!("{}{}{}", request.from_onion, request.from_friendcode, request.timestamp);

        // Decode signature from base64
        let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(&request.signature) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false),
        };

        // Verify Ed25519 signature
        use ed25519_dalek::{VerifyingKey, Verifier, Signature};

        let verifying_key = match VerifyingKey::from_bytes(&pubkey_array) {
            Ok(key) => key,
            Err(_) => return Ok(false),
        };

        let sig_array: [u8; 64] = match sig_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return Ok(false),
        };
        let signature = Signature::from_bytes(&sig_array);

        match verifying_key.verify(data.as_bytes(), &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
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
        ).map_err(|e| ChattorError::Database(format!("Failed to store request: {}", e)))?;

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
        // Generate a dedicated X25519 Signal identity keypair for X3DH
        let signal_identity = libsignal_protocol::vxeddsa::gen_keypair();
        let signal_identity_public_raw = libsignal_protocol::utils::decode_public_key(&signal_identity.public)
            .map_err(|_| ChattorError::Crypto("Failed to decode signal identity public key".into()))?;
        let (bundle, _private_material) = PreKeyBundle::generate_real(&signal_identity.secret, &signal_identity_public_raw)?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Sign message
        let data = format!("{}{}{}", own_onion, peer_onion, timestamp);
        let signature = identity.sign(data.as_bytes());
        let signature_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

        // Serialize bundle to JSON then base64
        let bundle_json = serde_json::to_string(&bundle)
            .map_err(|e| ChattorError::Crypto(format!("Failed to serialize bundle: {}", e)))?;

        Ok(FriendRequestAcceptMessage {
            from_onion: own_onion.to_string(),
            to_onion: peer_onion.to_string(),
            signal_prekey_bundle: bundle_json,
            timestamp,
            signature: signature_base64,
            ed25519_pubkey: Some(identity.public_key_base64()),
        })
    }

    /// Handle accept message - initialize session
    ///
    /// NOTE: This currently only stores the friend in the database.
    /// Real session initialization requires the local identity keypair to perform
    /// X3DH key exchange via `SignalSession::from_prekey_bundle_real()`.
    /// The caller in main.rs handles session establishment directly.
    pub fn handle_accept(&self, accept: &FriendRequestAcceptMessage) -> Result<()> {
        // Add friend to database
        let conn = self.db.connection();
        conn.execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES (?1, ?2, ?3, 'active')",
            (
                &accept.from_onion,
                &crate::ui::input::truncate_display_dots(&accept.from_onion, 10),
                accept.timestamp,
            ),
        ).map_err(|e| ChattorError::Database(format!("Failed to add friend: {}", e)))?;

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
        ).map_err(|e| ChattorError::Database(format!("Request not found: {}", e)))?;

        // Update request status
        conn.execute(
            "UPDATE friend_requests SET status = 'accepted' WHERE id = ?1",
            [request_id],
        ).map_err(|e| ChattorError::Database(format!("Failed to update request: {}", e)))?;

        // Add friend
        conn.execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES (?1, ?2, ?3, 'active')",
            (
                &from_onion,
                &crate::ui::input::truncate_display_dots(&from_onion, 10),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            ),
        ).map_err(|e| ChattorError::Database(format!("Failed to add friend: {}", e)))?;

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
    fn test_validate_request_verifies_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = "test.onion"; // Any onion works — we verify against included pubkey
        let friend_code = "happy-1234-tiger-5678";

        let request = FriendRequestHandler::create_request(&identity, onion, friend_code).unwrap();

        assert!(FriendRequestHandler::validate_request(&request).unwrap());
    }

    #[test]
    fn test_validate_request_rejects_forged_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = "test.onion";
        let friend_code = "happy-1234-tiger-5678";

        let mut request = FriendRequestHandler::create_request(&identity, onion, friend_code).unwrap();

        // Forge: replace with a different identity's pubkey
        let other_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        request.ed25519_pubkey = Some(other_identity.public_key_base64());

        assert!(!FriendRequestHandler::validate_request(&request).unwrap());
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
