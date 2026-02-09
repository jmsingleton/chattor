use crate::error::{Result, TorrentChatError};
use serde::{Deserialize, Serialize};

/// PreKey bundle for Signal Protocol session initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreKeyBundle {
    pub identity_key: Vec<u8>,
    pub signed_prekey: SignedPreKey,
    pub prekey: Option<PreKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPreKey {
    pub key_id: u32,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreKey {
    pub key_id: u32,
    pub public_key: Vec<u8>,
}

impl PreKeyBundle {
    /// Generate new PreKey bundle
    pub fn generate() -> Result<Self> {
        use rand::Rng;

        // For MVP, generate placeholder keys
        // TODO: Replace with real libsignal key generation
        let mut rng = rand::thread_rng();

        let identity_key = (0..32).map(|_| rng.gen::<u8>()).collect();
        let prekey_public = (0..32).map(|_| rng.gen::<u8>()).collect();
        let signed_prekey_public = (0..32).map(|_| rng.gen::<u8>()).collect();
        let signature = (0..64).map(|_| rng.gen::<u8>()).collect();

        Ok(PreKeyBundle {
            identity_key,
            signed_prekey: SignedPreKey {
                key_id: 1,
                public_key: signed_prekey_public,
                signature,
            },
            prekey: Some(PreKey {
                key_id: 1,
                public_key: prekey_public,
            }),
        })
    }
}

/// Signal session for encryption/decryption
pub struct SignalSession {
    pub remote_onion: String,
    session_data: Vec<u8>, // Serialized session state
}

impl SignalSession {
    /// Create new session from PreKey bundle (X3DH)
    pub fn from_prekey_bundle(remote_onion: String, bundle: &PreKeyBundle) -> Result<Self> {
        // For MVP, store bundle as session data
        // TODO: Replace with real X3DH key agreement
        let session_data = serde_json::to_vec(bundle)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to serialize bundle: {}", e)))?;

        Ok(SignalSession {
            remote_onion,
            session_data,
        })
    }

    /// Create session from received PreKey message
    pub fn from_prekey_message(remote_onion: String, message: &[u8]) -> Result<Self> {
        // For MVP, use message as session data
        // TODO: Replace with real session initialization
        Ok(SignalSession {
            remote_onion,
            session_data: message.to_vec(),
        })
    }

    /// Encrypt plaintext
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<(Vec<u8>, bool)> {
        // For MVP, return plaintext with flag indicating if PreKey message
        // TODO: Replace with real Double Ratchet encryption
        let is_prekey_message = self.session_data.len() < 100;
        Ok((plaintext.to_vec(), is_prekey_message))
    }

    /// Decrypt ciphertext
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        // For MVP, return ciphertext as plaintext
        // TODO: Replace with real Double Ratchet decryption
        Ok(ciphertext.to_vec())
    }

    /// Serialize session state for storage
    pub fn to_bytes(&self) -> Vec<u8> {
        self.session_data.clone()
    }

    /// Deserialize session state from storage
    pub fn from_bytes(remote_onion: String, bytes: Vec<u8>) -> Result<Self> {
        Ok(SignalSession {
            remote_onion,
            session_data: bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_prekey_bundle() {
        let bundle = PreKeyBundle::generate().unwrap();

        assert!(bundle.identity_key.len() > 0);
        assert!(bundle.signed_prekey.key_id > 0);
        assert!(bundle.signed_prekey.public_key.len() > 0);
        assert!(bundle.signed_prekey.signature.len() > 0);
        assert!(bundle.prekey.is_some());
    }
}
