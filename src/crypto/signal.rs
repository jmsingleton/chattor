use crate::error::{Result, TorrentChatError};
use serde::{Deserialize, Serialize};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret, SharedSecret};
use hkdf::Hkdf;
use sha2::Sha256;

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
    #[allow(dead_code)]
    pub signed_prekey_secret: [u8; 32], // X25519 private key bytes
    #[allow(dead_code)]
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

/// Serializable session state for proper persistence
#[derive(Serialize, Deserialize)]
struct SessionState {
    remote_onion: String,
    shared_secret_bytes: Option<[u8; 32]>,
    send_counter: u64,
    recv_counter: u64,
    ephemeral_public: Option<[u8; 32]>,
}

impl PreKeyBundle {
    /// Generate new PreKey bundle
    pub fn generate() -> Result<Self> {
        use rand::Rng;

        // Test-only: creates random bytes for error-path testing.
        // See generate_real() for production use.
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
    #[allow(dead_code)]
    session_data: Vec<u8>,
    // Real Signal Protocol session data
    shared_secret: Option<SharedSecret>,
    send_counter: u64,
    recv_counter: u64,
    ephemeral_public: Option<[u8; 32]>, // Store ephemeral public key for PreKey message
}

impl SignalSession {
    /// Create test session with no shared_secret (test-only).
    /// encrypt/decrypt will error — use from_prekey_bundle_real() for functional sessions.
    #[cfg(test)]
    pub fn from_prekey_bundle(remote_onion: String, bundle: &PreKeyBundle) -> Result<Self> {
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

    /// Create session from PreKey bundle (initiator) with real Signal Protocol
    ///
    /// **Security Note:** This is a simplified X3DH implementation using only one
    /// Diffie-Hellman operation. Full X3DH requires 3-4 DH operations for proper
    /// forward secrecy and authentication.
    ///
    /// # Arguments
    /// * `remote_onion` - The remote peer's .onion address
    /// * `bundle` - The remote peer's PreKey bundle
    /// * `_remote_private` - Unused in simplified X3DH
    /// * `_local_identity` - Unused in simplified X3DH
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
    ///
    /// **Security Note:** This is a simplified X3DH implementation using only one
    /// Diffie-Hellman operation. Full X3DH requires 3-4 DH operations for proper
    /// forward secrecy and authentication.
    ///
    /// # Arguments
    /// * `remote_onion` - The remote peer's .onion address
    /// * `ciphertext` - The PreKey message (includes ephemeral public key)
    /// * `_local_bundle` - Unused in simplified X3DH
    /// * `local_private` - Private key material for this peer
    /// * `_local_identity` - Unused in simplified X3DH
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
            // Use HKDF for proper key derivation
            let shared_secret_bytes = shared_secret.as_bytes();
            let hk = Hkdf::<Sha256>::new(None, shared_secret_bytes);
            let mut key_bytes = [0u8; 32];
            let counter_bytes = self.send_counter.to_be_bytes();
            let mut info = Vec::new();
            info.extend_from_slice(b"chattor-message-key");
            info.extend_from_slice(&counter_bytes);
            hk.expand(&info, &mut key_bytes)
                .map_err(|_| TorrentChatError::Crypto("HKDF expand failed".into()))?;
            let key = chacha20poly1305::Key::from_slice(&key_bytes);
            let cipher = ChaCha20Poly1305::new(key);

            // Generate nonce from counter
            let mut nonce_bytes = [0u8; 12];
            nonce_bytes[4..12].copy_from_slice(&self.send_counter.to_be_bytes());
            let nonce = Nonce::from_slice(&nonce_bytes);

            // Encrypt
            let ciphertext = cipher.encrypt(nonce, plaintext)
                .map_err(|e| TorrentChatError::Crypto(format!("Encryption failed: {}", e)))?;

            // PreKey message = first message from the initiator (who has an ephemeral key).
            // Use .take() so that only the very first encrypt prepends the ephemeral.
            let ephemeral_for_prekey = self.ephemeral_public.take();
            let is_prekey = ephemeral_for_prekey.is_some();
            let mut result = Vec::new();

            if let Some(ephemeral_pub) = ephemeral_for_prekey {
                result.extend_from_slice(&ephemeral_pub);
            }
            result.extend_from_slice(&ciphertext);

            // Increment counter
            self.send_counter += 1;

            Ok((result, is_prekey))
        } else {
            Err(TorrentChatError::Crypto(
                format!("No encryption session established for {}", self.remote_onion)
            ))
        }
    }

    /// Decrypt ciphertext
    ///
    /// `is_prekey_message` indicates whether the ciphertext has a 32-byte
    /// ephemeral public key prefix (from the initiator's first message).
    /// The caller determines this from the wire-format `signal_type` field.
    pub fn decrypt(&mut self, ciphertext: &[u8], is_prekey_message: bool) -> Result<Vec<u8>> {
        // If we have a real shared secret, use real decryption
        if let Some(ref shared_secret) = self.shared_secret {
            // Strip ephemeral key header if this is a PreKey message
            let actual_ciphertext = if is_prekey_message && ciphertext.len() > 32 {
                &ciphertext[32..]
            } else {
                ciphertext
            };

            // Use HKDF for proper key derivation
            let shared_secret_bytes = shared_secret.as_bytes();
            let hk = Hkdf::<Sha256>::new(None, shared_secret_bytes);
            let mut key_bytes = [0u8; 32];
            let counter_bytes = self.recv_counter.to_be_bytes();
            let mut info = Vec::new();
            info.extend_from_slice(b"chattor-message-key");
            info.extend_from_slice(&counter_bytes);
            hk.expand(&info, &mut key_bytes)
                .map_err(|_| TorrentChatError::Crypto("HKDF expand failed".into()))?;
            let key = chacha20poly1305::Key::from_slice(&key_bytes);
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
            Err(TorrentChatError::Crypto(
                format!("No decryption session established for {}", self.remote_onion)
            ))
        }
    }

    /// Serialize session state for storage
    pub fn to_bytes(&self) -> Vec<u8> {
        let state = SessionState {
            remote_onion: self.remote_onion.clone(),
            shared_secret_bytes: self.shared_secret.as_ref().map(|s| s.to_bytes()),
            send_counter: self.send_counter,
            recv_counter: self.recv_counter,
            ephemeral_public: self.ephemeral_public,
        };
        bincode::serialize(&state).expect("Failed to serialize session state")
    }

    /// Deserialize session state from storage
    pub fn from_bytes(_remote_onion: String, bytes: Vec<u8>) -> Result<Self> {
        let state: SessionState = bincode::deserialize(&bytes)
            .map_err(|e| TorrentChatError::Crypto(format!("Failed to deserialize session: {}", e)))?;

        // Reconstruct SharedSecret from raw bytes
        // SharedSecret is an opaque type without a public constructor from bytes,
        // so we use unsafe transmute since we control the serialization on both
        // sides and know the internal representation is [u8; 32].
        let shared_secret = state.shared_secret_bytes.map(|secret_bytes| {
            // SAFETY: SharedSecret is a wrapper around [u8; 32] with the same memory layout.
            // This is safe because:
            // 1. We serialized it from SharedSecret.to_bytes() which gives us the raw [u8; 32]
            // 2. We're reconstructing the exact same type
            // 3. SharedSecret has no invariants beyond being 32 bytes
            unsafe { std::mem::transmute::<[u8; 32], SharedSecret>(secret_bytes) }
        });

        Ok(SignalSession {
            remote_onion: state.remote_onion,
            shared_secret,
            send_counter: state.send_counter,
            recv_counter: state.recv_counter,
            ephemeral_public: state.ephemeral_public,
            session_data: bytes,  // Keep for backward compatibility
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

        // Bob decrypts (this is a PreKey message — has ephemeral prefix)
        let decrypted = bob_session.decrypt(&ciphertext, true).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_without_shared_secret_errors() {
        let bundle = PreKeyBundle::generate().unwrap();
        let mut session = SignalSession::from_prekey_bundle(
            "test.onion".into(),
            &bundle,
        ).unwrap();

        // Session has no real shared_secret — should error, not return plaintext
        let result = session.encrypt(b"hello");
        assert!(result.is_err(), "encrypt() should error without shared_secret");
    }

    #[test]
    fn test_decrypt_without_shared_secret_errors() {
        let bundle = PreKeyBundle::generate().unwrap();
        let mut session = SignalSession::from_prekey_bundle(
            "test.onion".into(),
            &bundle,
        ).unwrap();

        // Session has no real shared_secret — should error, not return plaintext
        let result = session.decrypt(b"some ciphertext", false);
        assert!(result.is_err(), "decrypt() should error without shared_secret");
    }

    #[test]
    fn test_session_serialization_prevents_nonce_reuse() {
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();

        // Bob generates PreKey bundle
        let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_identity).unwrap();

        // Alice creates session and sends two messages
        let mut alice_session = SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_identity,
        ).unwrap();

        let (msg1, _) = alice_session.encrypt(b"Message 1").unwrap();
        let (msg2, _) = alice_session.encrypt(b"Message 2").unwrap();

        // Serialize Alice's session after sending two messages
        let serialized = alice_session.to_bytes();

        // Deserialize into a new session
        let mut alice_restored = SignalSession::from_bytes("bob.onion".into(), serialized).unwrap();

        // Verify counters were preserved (send_counter should be 2)
        // Send another message - this should use counter 2, not reuse 0 or 1
        let (msg3, is_prekey) = alice_restored.encrypt(b"Message 3").unwrap();

        // Third message should NOT be a PreKey message (counter != 0)
        assert!(!is_prekey);

        // Verify all messages use different ciphertexts (different nonces)
        assert_ne!(msg1, msg2);
        assert_ne!(msg2, msg3);
        assert_ne!(msg1, msg3);

        // Bob should be able to decrypt all three messages in order
        let mut bob_session = SignalSession::from_prekey_message_real(
            "alice.onion".into(),
            &msg1,
            &bob_bundle,
            &bob_private,
            &bob_identity,
        ).unwrap();

        assert_eq!(bob_session.decrypt(&msg1, true).unwrap(), b"Message 1");
        assert_eq!(bob_session.decrypt(&msg2, false).unwrap(), b"Message 2");
        assert_eq!(bob_session.decrypt(&msg3, false).unwrap(), b"Message 3");
    }
}
