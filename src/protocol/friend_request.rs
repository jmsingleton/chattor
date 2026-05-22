use crate::error::Result;
use crate::crypto::IdentityKeypair;
use crate::protocol::message::*;
use base64::Engine;

/// Friend request protocol helpers (message construction + signature validation).
///
/// The actual database persistence and X3DH-bundle generation paths live in
/// `main.rs::handle_incoming_message` and `main.rs::handle_accept_friend_request`
/// because they need access to the live `App` state (identity, onion address,
/// queue, presence). This struct only carries pure functions.
pub struct FriendRequestHandler;

impl FriendRequestHandler {
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
        })
    }

    /// Validate received friend request: verify Ed25519 signature against the
    /// pubkey embedded in the sender's v3 .onion address, and check the
    /// timestamp is within a 5-minute window.
    ///
    /// Returns Ok(true) if the request is authentic, Ok(false) if forged,
    /// stale, or malformed.
    pub fn validate_request(request: &FriendRequestMessage) -> Result<bool> {
        // Check timestamp (within 5 minutes)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let age = (now - request.timestamp).abs();
        if age > 300 {
            return Ok(false);
        }

        // Extract public key from sender's .onion address
        let pubkey_bytes = match crate::protocol::friend_code::onion_to_pubkey(&request.from_onion) {
            Ok(bytes) => bytes,
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

        let verifying_key = match VerifyingKey::from_bytes(&pubkey_bytes) {
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
        assert!(!request.signature.is_empty());
        assert!(request.timestamp > 0);
    }

    #[test]
    fn test_validate_request_verifies_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = identity.to_onion_address();
        let friend_code = "happy-1234-tiger-5678";

        let request = FriendRequestHandler::create_request(&identity, &onion, friend_code).unwrap();

        assert!(FriendRequestHandler::validate_request(&request).unwrap());
    }

    #[test]
    fn test_validate_request_rejects_forged_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = identity.to_onion_address();
        let friend_code = "happy-1234-tiger-5678";

        let mut request = FriendRequestHandler::create_request(&identity, &onion, friend_code).unwrap();

        // Forge: change the from_onion to a different identity's address
        let other_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        request.from_onion = other_identity.to_onion_address();

        assert!(!FriendRequestHandler::validate_request(&request).unwrap());
    }

    #[test]
    fn test_validate_request_rejects_stale_timestamp() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = identity.to_onion_address();
        let friend_code = "happy-1234-tiger-5678";

        let mut request = FriendRequestHandler::create_request(&identity, &onion, friend_code).unwrap();
        request.timestamp -= 600; // 10 minutes ago

        // The signature was over the original timestamp, so backdating breaks
        // both the freshness check and the signature check.
        assert!(!FriendRequestHandler::validate_request(&request).unwrap());
    }

    #[test]
    fn test_validate_request_rejects_malformed_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let onion = identity.to_onion_address();
        let friend_code = "happy-1234-tiger-5678";

        let mut request = FriendRequestHandler::create_request(&identity, &onion, friend_code).unwrap();
        request.signature = "not-valid-base64!!!".to_string();

        assert!(!FriendRequestHandler::validate_request(&request).unwrap());
    }

}
