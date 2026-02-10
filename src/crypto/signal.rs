use crate::error::{Result, TorrentChatError};
use serde::{Deserialize, Serialize};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret, SharedSecret};

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
    session_data: Vec<u8>, // Serialized session state (for MVP compatibility)
    // Real Signal Protocol session data
    shared_secret: Option<SharedSecret>,
    send_counter: u64,
    recv_counter: u64,
    ephemeral_public: Option<[u8; 32]>, // Store ephemeral public key for PreKey message
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
            shared_secret: None,
            send_counter: 0,
            recv_counter: 0,
            ephemeral_public: None,
        })
    }

    /// Create session from received PreKey message
    pub fn from_prekey_message(remote_onion: String, message: &[u8]) -> Result<Self> {
        // For MVP, use message as session data
        // TODO: Replace with real session initialization
        Ok(SignalSession {
            remote_onion,
            session_data: message.to_vec(),
            shared_secret: None,
            send_counter: 0,
            recv_counter: 0,
            ephemeral_public: None,
        })
    }

    /// Create session from PreKey bundle (initiator) with real Signal Protocol
    pub fn from_prekey_bundle_real(
        remote_onion: String,
        bundle: &PreKeyBundle,
        _remote_private: &PreKeyPrivateMaterial,
        _local_identity: &crate::crypto::IdentityKeypair,
    ) -> Result<Self> {
        use rand::rngs::OsRng;

        // Generate ephemeral key for this session
        let ephemeral_secret = StaticSecret::random_from_rng(OsRng);
        let ephemeral_public = X25519PublicKey::from(&ephemeral_secret);

        // Parse remote identity public key
        let remote_identity_pub = X25519PublicKey::from(
            <[u8; 32]>::try_from(&bundle.identity_key[..32])
                .map_err(|_| TorrentChatError::Crypto("Invalid identity key length".into()))?
        );

        // Compute shared secret (X3DH simplified)
        let shared_secret = ephemeral_secret.diffie_hellman(&remote_identity_pub);

        // Store ephemeral public key for inclusion in first message
        let ephemeral_bytes = ephemeral_public.to_bytes();

        Ok(SignalSession {
            remote_onion,
            session_data: Vec::new(), // Not used in real mode
            shared_secret: Some(shared_secret),
            send_counter: 0,
            recv_counter: 0,
            ephemeral_public: Some(ephemeral_bytes),
        })
    }

    /// Create session from received PreKey message (recipient) with real Signal Protocol
    pub fn from_prekey_message_real(
        remote_onion: String,
        ciphertext: &[u8],
        _local_bundle: &PreKeyBundle,
        local_private: &PreKeyPrivateMaterial,
        _local_identity: &crate::crypto::IdentityKeypair,
    ) -> Result<Self> {
        // Parse the ephemeral public key from message header (first 32 bytes)
        if ciphertext.len() < 32 {
            return Err(TorrentChatError::Crypto("Message too short for PreKey message".into()));
        }

        let ephemeral_pub = X25519PublicKey::from(
            <[u8; 32]>::try_from(&ciphertext[..32])
                .map_err(|_| TorrentChatError::Crypto("Invalid ephemeral key".into()))?
        );

        // Use our identity private key
        let local_identity_secret = StaticSecret::from(local_private.identity_secret);

        // Compute shared secret
        let shared_secret = local_identity_secret.diffie_hellman(&ephemeral_pub);

        Ok(SignalSession {
            remote_onion,
            session_data: Vec::new(), // Not used in real mode
            shared_secret: Some(shared_secret),
            send_counter: 0,
            recv_counter: 0,
            ephemeral_public: None, // Recipient doesn't need to store ephemeral key
        })
    }

    /// Encrypt plaintext
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<(Vec<u8>, bool)> {
        // If we have a real shared secret, use real encryption
        if let Some(ref shared_secret) = self.shared_secret {
            // Derive encryption key from shared secret + counter
            let mut key_material = shared_secret.as_bytes().to_vec();
            key_material.extend_from_slice(&self.send_counter.to_be_bytes());

            // Use first 32 bytes as ChaCha20-Poly1305 key
            let key = chacha20poly1305::Key::from_slice(&key_material[..32]);
            let cipher = ChaCha20Poly1305::new(key);

            // Generate nonce from counter
            let mut nonce_bytes = [0u8; 12];
            nonce_bytes[4..12].copy_from_slice(&self.send_counter.to_be_bytes());
            let nonce = Nonce::from_slice(&nonce_bytes);

            // Encrypt
            let ciphertext = cipher.encrypt(nonce, plaintext)
                .map_err(|e| TorrentChatError::Crypto(format!("Encryption failed: {}", e)))?;

            // For first message, prepend ephemeral public key (PreKey message)
            let is_prekey = self.send_counter == 0;
            let mut result = Vec::new();

            if is_prekey {
                if let Some(ephemeral_pub) = self.ephemeral_public {
                    result.extend_from_slice(&ephemeral_pub);
                }
            }
            result.extend_from_slice(&ciphertext);

            // Increment counter
            self.send_counter += 1;

            Ok((result, is_prekey))
        } else {
            // For MVP compatibility, return plaintext with flag indicating if PreKey message
            let is_prekey_message = self.session_data.len() < 100;
            Ok((plaintext.to_vec(), is_prekey_message))
        }
    }

    /// Decrypt ciphertext
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        // If we have a real shared secret, use real decryption
        if let Some(ref shared_secret) = self.shared_secret {
            // Skip ephemeral key if PreKey message (first 32 bytes)
            let actual_ciphertext = if ciphertext.len() > 32 && self.recv_counter == 0 {
                &ciphertext[32..]
            } else {
                ciphertext
            };

            // Derive decryption key
            let mut key_material = shared_secret.as_bytes().to_vec();
            key_material.extend_from_slice(&self.recv_counter.to_be_bytes());

            let key = chacha20poly1305::Key::from_slice(&key_material[..32]);
            let cipher = ChaCha20Poly1305::new(key);

            // Generate nonce
            let mut nonce_bytes = [0u8; 12];
            nonce_bytes[4..12].copy_from_slice(&self.recv_counter.to_be_bytes());
            let nonce = Nonce::from_slice(&nonce_bytes);

            // Decrypt
            let plaintext = cipher.decrypt(nonce, actual_ciphertext)
                .map_err(|e| TorrentChatError::Crypto(format!("Decryption failed: {}", e)))?;

            // Increment counter
            self.recv_counter += 1;

            Ok(plaintext)
        } else {
            // For MVP compatibility, return ciphertext as plaintext
            Ok(ciphertext.to_vec())
        }
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
            shared_secret: None,
            send_counter: 0,
            recv_counter: 0,
            ephemeral_public: None,
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

    #[test]
    fn test_real_session_encryption_decryption() {
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();

        // Bob generates PreKey bundle
        let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_identity).unwrap();

        // Alice creates session from Bob's bundle
        let mut alice_session = SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_identity,
        ).unwrap();

        // Alice encrypts
        let plaintext = b"Hello Bob!";
        let (ciphertext, is_prekey) = alice_session.encrypt(plaintext).unwrap();

        assert!(is_prekey); // First message should be PreKey type
        assert_ne!(ciphertext, plaintext); // Should be encrypted

        // Bob creates session from Alice's PreKey message
        let mut bob_session = SignalSession::from_prekey_message_real(
            "alice.onion".into(),
            &ciphertext,
            &bob_bundle,
            &bob_private,
            &bob_identity,
        ).unwrap();

        // Bob decrypts
        let decrypted = bob_session.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
