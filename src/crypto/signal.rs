use crate::error::{Result, TorrentChatError};
use serde::{Deserialize, Serialize};

/// PreKey bundle for Signal Protocol session initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreKeyBundle {
    pub identity_key: Vec<u8>,
    pub signed_prekey: SignedPreKey,
    pub prekey: Option<PreKey>,
}

/// Private key material for X3DH key agreement
///
/// Contains the private keys needed to complete Signal Protocol's X3DH
/// key agreement when receiving messages encrypted to this PreKey bundle.
#[derive(Debug)]
pub struct PreKeyPrivateMaterial {
    pub identity_secret: [u8; 32],      // X25519 private key bytes
    pub signed_prekey_secret: [u8; 32], // X25519 private key bytes
    pub prekey_secret: Option<[u8; 32]>, // X25519 private key bytes
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

    /// Generate real PreKey bundle with libsignal
    ///
    /// Creates a new PreKey bundle for Signal Protocol X3DH key agreement.
    /// Generates fresh X25519 keys for identity, signed prekey, and one-time prekey.
    ///
    /// **Note:** Generates an independent X25519 identity key pair for Signal Protocol.
    /// The provided Ed25519 identity is used ONLY for signing the prekey, not for
    /// deriving the X25519 identity.
    ///
    /// # Arguments
    /// * `identity` - Ed25519 identity keypair used for signing the prekey
    ///
    /// # Returns
    /// A tuple of (PreKeyBundle, PreKeyPrivateMaterial):
    /// - PreKeyBundle: Public keys for transmission to peers
    /// - PreKeyPrivateMaterial: Private keys needed for X3DH key agreement
    pub fn generate_real(identity: &crate::crypto::IdentityKeypair) -> Result<(Self, PreKeyPrivateMaterial)> {
        use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
        use rand::rngs::OsRng;

        // Generate identity key pair for X25519
        let identity_secret = StaticSecret::random_from_rng(OsRng);
        let identity_public = X25519PublicKey::from(&identity_secret);

        // Generate signed pre-key
        let signed_prekey_secret = StaticSecret::random_from_rng(OsRng);
        let signed_prekey_public = X25519PublicKey::from(&signed_prekey_secret);
        let signed_prekey_id = 1u32;

        // Sign the pre-key with Ed25519 identity
        let signature = identity.sign(signed_prekey_public.as_bytes());

        // Generate one-time pre-key
        let prekey_secret = StaticSecret::random_from_rng(OsRng);
        let prekey_public = X25519PublicKey::from(&prekey_secret);
        let prekey_id = 1u32;

        // Convert public keys to our format (extract 32 bytes)
        let identity_key_bytes = identity_public.as_bytes().to_vec();
        let signed_prekey_bytes = signed_prekey_public.as_bytes().to_vec();
        let prekey_bytes = prekey_public.as_bytes().to_vec();
        let signature_bytes = signature.to_bytes().to_vec();

        // Store private keys as byte arrays
        let private_material = PreKeyPrivateMaterial {
            identity_secret: identity_secret.to_bytes(),
            signed_prekey_secret: signed_prekey_secret.to_bytes(),
            prekey_secret: Some(prekey_secret.to_bytes()),
        };

        let bundle = PreKeyBundle {
            identity_key: identity_key_bytes,
            signed_prekey: SignedPreKey {
                key_id: signed_prekey_id,
                public_key: signed_prekey_bytes,
                signature: signature_bytes,
            },
            prekey: Some(PreKey {
                key_id: prekey_id,
                public_key: prekey_bytes,
            }),
        };

        Ok((bundle, private_material))
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

    #[test]
    fn test_generate_real_prekey_bundle() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let (bundle, private_material) = PreKeyBundle::generate_real(&identity).unwrap();

        // Real keys should be 32 bytes (Curve25519)
        assert_eq!(bundle.identity_key.len(), 32);
        assert_eq!(bundle.signed_prekey.public_key.len(), 32);
        assert_eq!(bundle.signed_prekey.signature.len(), 64);
        assert!(bundle.prekey.is_some());
        assert_eq!(bundle.prekey.as_ref().unwrap().public_key.len(), 32);

        // Verify private keys are returned
        assert_eq!(private_material.identity_secret.len(), 32);
        assert_eq!(private_material.signed_prekey_secret.len(), 32);
        assert!(private_material.prekey_secret.is_some());
        assert_eq!(private_material.prekey_secret.unwrap().len(), 32);
    }
}
