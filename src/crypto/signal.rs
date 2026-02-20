use crate::error::{Result, TorrentChatError};
use serde::{Deserialize, Serialize};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret, SharedSecret};
use hkdf::Hkdf;
use sha2::Sha256;

/// PreKey bundle for Signal Protocol session initialization.
///
/// All public keys are stored as raw 32-byte X25519 keys internally (no 0x05 prefix)
/// for serde compatibility. Use `to_libsignal_bundle()` to convert to the
/// `libsignal_protocol::x3dh::PreKeyBundle` type with properly encoded 33-byte keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreKeyBundle {
    pub identity_key: Vec<u8>,
    pub signed_prekey: SignedPreKey,
    pub prekey: Option<PreKey>,
}

/// Private key material for X3DH key agreement.
///
/// Contains the private keys needed to complete Signal Protocol's X3DH
/// key agreement when receiving messages encrypted to this PreKey bundle.
/// All secrets are raw 32-byte X25519 private key scalars.
#[derive(Debug)]
pub struct PreKeyPrivateMaterial {
    pub identity_secret: [u8; 32],      // X25519 private key bytes
    #[allow(dead_code)]
    pub signed_prekey_secret: [u8; 32], // X25519 private key bytes
    #[allow(dead_code)]
    pub prekey_secret: Option<[u8; 32]>, // X25519 private key bytes
}

/// A signed prekey with a VXEdDSA signature (96 bytes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPreKey {
    pub key_id: u32,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

/// A one-time prekey.
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
    /// Generate new PreKey bundle with random bytes (test-only).
    ///
    /// Creates a bundle with random (non-cryptographic) key material.
    /// Useful only for error-path testing where valid keys are not needed.
    /// For production use, see `generate_real()`.
    pub fn generate() -> Result<Self> {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let identity_key = (0..32).map(|_| rng.gen::<u8>()).collect();
        let prekey_public = (0..32).map(|_| rng.gen::<u8>()).collect();
        let signed_prekey_public = (0..32).map(|_| rng.gen::<u8>()).collect();
        // VXEdDSA signatures are 96 bytes
        let signature = (0..96).map(|_| rng.gen::<u8>()).collect();

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

    /// Generate real PreKey bundle using libsignal-dezire X3DH types.
    ///
    /// Creates a new PreKey bundle for Signal Protocol X3DH key agreement.
    /// Uses VXEdDSA (instead of Ed25519) to sign the signed prekey with the
    /// X25519 Signal identity key. This is the standard Signal approach: the
    /// same X25519 identity key is used for both DH and signing (via XEdDSA/VXEdDSA).
    ///
    /// The Ed25519 identity (used for friend request signing and .onion address)
    /// is completely separate from this X25519 Signal identity.
    ///
    /// # Arguments
    /// * `signal_identity_secret` - X25519 Signal identity private key (32 bytes)
    /// * `signal_identity_public` - X25519 Signal identity public key (32 bytes, raw, no prefix)
    ///
    /// # Returns
    /// A tuple of (PreKeyBundle, PreKeyPrivateMaterial):
    /// - PreKeyBundle: Public keys for transmission to peers (32-byte raw keys internally)
    /// - PreKeyPrivateMaterial: Private keys needed for X3DH key agreement
    pub fn generate_real(
        signal_identity_secret: &[u8; 32],
        signal_identity_public: &[u8; 32],
    ) -> Result<(Self, PreKeyPrivateMaterial)> {
        use libsignal_protocol::vxeddsa::{gen_keypair, vxeddsa_sign};

        // Generate signed pre-key using libsignal key generation
        let signed_prekey_pair = gen_keypair();
        let signed_prekey_id = 1u32;

        // Sign the signed prekey's encoded public key (33 bytes with 0x05 prefix)
        // using VXEdDSA with our Signal identity private key.
        // The message being signed is the encoded SPK public key, matching the
        // X3DH spec: Sig(IK, Encode(SPK)).
        let sig_output = vxeddsa_sign(signal_identity_secret, &signed_prekey_pair.public)
            .map_err(|()| TorrentChatError::Crypto("VXEdDSA signing failed".into()))?;

        // Generate one-time pre-key
        let prekey_pair = gen_keypair();
        let prekey_id = 1u32;

        // Decode 33-byte encoded public keys to raw 32-byte keys for internal storage
        let signed_prekey_raw = libsignal_protocol::utils::decode_public_key(&signed_prekey_pair.public)
            .map_err(|_| TorrentChatError::Crypto("Failed to decode signed prekey public key".into()))?;
        let prekey_raw = libsignal_protocol::utils::decode_public_key(&prekey_pair.public)
            .map_err(|_| TorrentChatError::Crypto("Failed to decode prekey public key".into()))?;

        // Store private keys as byte arrays
        let private_material = PreKeyPrivateMaterial {
            identity_secret: *signal_identity_secret,
            signed_prekey_secret: signed_prekey_pair.secret,
            prekey_secret: Some(prekey_pair.secret),
        };

        let bundle = PreKeyBundle {
            identity_key: signal_identity_public.to_vec(),
            signed_prekey: SignedPreKey {
                key_id: signed_prekey_id,
                public_key: signed_prekey_raw.to_vec(),
                signature: sig_output.signature.to_vec(),
            },
            prekey: Some(PreKey {
                key_id: prekey_id,
                public_key: prekey_raw.to_vec(),
            }),
        };

        Ok((bundle, private_material))
    }

    /// Convert this bundle to a `libsignal_protocol::x3dh::PreKeyBundle`.
    ///
    /// Encodes the internally-stored raw 32-byte public keys into 33-byte
    /// encoded keys (with 0x05 prefix) as expected by the libsignal X3DH API.
    #[allow(dead_code)]
    pub fn to_libsignal_bundle(&self) -> Result<libsignal_protocol::x3dh::PreKeyBundle> {
        use libsignal_protocol::utils::encode_public_key;

        // Encode identity key (32 bytes -> 33 bytes with 0x05 prefix)
        let identity_raw: [u8; 32] = self.identity_key[..].try_into()
            .map_err(|_| TorrentChatError::Crypto(
                format!("Invalid identity key length: expected 32, got {}", self.identity_key.len())
            ))?;
        let identity_encoded = encode_public_key(&identity_raw);

        // Encode signed prekey public key
        let spk_raw: [u8; 32] = self.signed_prekey.public_key[..].try_into()
            .map_err(|_| TorrentChatError::Crypto(
                format!("Invalid signed prekey length: expected 32, got {}", self.signed_prekey.public_key.len())
            ))?;
        let spk_encoded = encode_public_key(&spk_raw);

        // Convert signature to fixed-size array
        let signature: [u8; 96] = self.signed_prekey.signature[..].try_into()
            .map_err(|_| TorrentChatError::Crypto(
                format!("Invalid signature length: expected 96, got {}", self.signed_prekey.signature.len())
            ))?;

        // Convert one-time prekey if present
        let one_time_prekey = match &self.prekey {
            Some(pk) => {
                let pk_raw: [u8; 32] = pk.public_key[..].try_into()
                    .map_err(|_| TorrentChatError::Crypto(
                        format!("Invalid prekey length: expected 32, got {}", pk.public_key.len())
                    ))?;
                let pk_encoded = encode_public_key(&pk_raw);
                Some(libsignal_protocol::x3dh::OneTimePreKey {
                    id: pk.key_id,
                    public_key: pk_encoded,
                })
            }
            None => None,
        };

        Ok(libsignal_protocol::x3dh::PreKeyBundle {
            identity_key: identity_encoded,
            signed_prekey: libsignal_protocol::x3dh::SignedPreKey {
                id: self.signed_prekey.key_id,
                public_key: spk_encoded,
                signature,
            },
            one_time_prekey,
        })
    }

    /// Verify that this bundle's signed prekey signature is valid.
    ///
    /// Uses VXEdDSA to verify that the signed prekey was signed by the
    /// identity key in this bundle.
    #[allow(dead_code)]
    pub fn verify_signature(&self) -> Result<bool> {
        use libsignal_protocol::vxeddsa::vxeddsa_verify;
        use libsignal_protocol::utils::encode_public_key;

        let identity_raw: [u8; 32] = self.identity_key[..].try_into()
            .map_err(|_| TorrentChatError::Crypto("Invalid identity key length".into()))?;
        let identity_encoded = encode_public_key(&identity_raw);

        let spk_raw: [u8; 32] = self.signed_prekey.public_key[..].try_into()
            .map_err(|_| TorrentChatError::Crypto("Invalid signed prekey length".into()))?;
        let spk_encoded = encode_public_key(&spk_raw);

        let signature: [u8; 96] = self.signed_prekey.signature[..].try_into()
            .map_err(|_| TorrentChatError::Crypto("Invalid signature length".into()))?;

        Ok(vxeddsa_verify(&identity_encoded, &spk_encoded, &signature).is_some())
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

    /// Helper: generate an X25519 Signal identity keypair for tests.
    fn gen_signal_identity() -> ([u8; 32], [u8; 32]) {
        let kp = libsignal_protocol::vxeddsa::gen_keypair();
        let raw_pub = libsignal_protocol::utils::decode_public_key(&kp.public).unwrap();
        (kp.secret, raw_pub)
    }

    #[test]
    fn test_generate_prekey_bundle() {
        let bundle = PreKeyBundle::generate().unwrap();

        assert!(!bundle.identity_key.is_empty());
        assert!(bundle.signed_prekey.key_id > 0);
        assert!(!bundle.signed_prekey.public_key.is_empty());
        assert!(!bundle.signed_prekey.signature.is_empty());
        assert!(bundle.prekey.is_some());
    }

    #[test]
    fn test_generate_real_prekey_bundle() {
        let (signal_secret, signal_public) = gen_signal_identity();
        let (bundle, private_material) = PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

        // Real keys should be 32 bytes (raw X25519 Curve25519)
        assert_eq!(bundle.identity_key.len(), 32);
        assert_eq!(bundle.signed_prekey.public_key.len(), 32);
        // VXEdDSA signatures are 96 bytes
        assert_eq!(bundle.signed_prekey.signature.len(), 96);
        assert!(bundle.prekey.is_some());
        assert_eq!(bundle.prekey.as_ref().unwrap().public_key.len(), 32);

        // Verify private keys are returned
        assert_eq!(private_material.identity_secret.len(), 32);
        assert_eq!(private_material.signed_prekey_secret.len(), 32);
        assert!(private_material.prekey_secret.is_some());
        assert_eq!(private_material.prekey_secret.unwrap().len(), 32);
    }

    #[test]
    fn test_vxeddsa_signature_roundtrip() {
        // Generate a Signal identity and a PreKey bundle, then verify the signature
        let (signal_secret, signal_public) = gen_signal_identity();
        let (bundle, _private_material) = PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

        // Verify the VXEdDSA signature on the signed prekey
        assert!(bundle.verify_signature().unwrap(), "VXEdDSA signature should verify");
    }

    #[test]
    fn test_vxeddsa_signature_rejects_tampered_key() {
        let (signal_secret, signal_public) = gen_signal_identity();
        let (mut bundle, _) = PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

        // Tamper with the signed prekey public key
        if let Some(byte) = bundle.signed_prekey.public_key.get_mut(0) {
            *byte ^= 0xFF;
        }

        // Signature should no longer verify
        assert!(!bundle.verify_signature().unwrap(), "Tampered bundle should fail verification");
    }

    #[test]
    fn test_vxeddsa_signature_rejects_wrong_identity() {
        let (signal_secret, signal_public) = gen_signal_identity();
        let (mut bundle, _) = PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

        // Replace identity key with a different key
        let (_, other_public) = gen_signal_identity();
        bundle.identity_key = other_public.to_vec();

        // Signature should fail because identity key doesn't match the signer
        assert!(!bundle.verify_signature().unwrap(), "Wrong identity key should fail verification");
    }

    #[test]
    fn test_to_libsignal_bundle_conversion() {
        let (signal_secret, signal_public) = gen_signal_identity();
        let (bundle, _) = PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

        let libsignal_bundle = bundle.to_libsignal_bundle().unwrap();

        // Identity key should be 33 bytes with 0x05 prefix
        assert_eq!(libsignal_bundle.identity_key[0], 0x05);
        assert_eq!(libsignal_bundle.identity_key.len(), 33);

        // Signed prekey should be 33 bytes with 0x05 prefix
        assert_eq!(libsignal_bundle.signed_prekey.public_key[0], 0x05);
        assert_eq!(libsignal_bundle.signed_prekey.public_key.len(), 33);

        // Signature should be 96 bytes
        assert_eq!(libsignal_bundle.signed_prekey.signature.len(), 96);

        // One-time prekey should be present and 33 bytes
        assert!(libsignal_bundle.one_time_prekey.is_some());
        let otpk = libsignal_bundle.one_time_prekey.unwrap();
        assert_eq!(otpk.public_key[0], 0x05);
        assert_eq!(otpk.public_key.len(), 33);
    }

    #[test]
    fn test_libsignal_bundle_x3dh_initiator_accepts() {
        // Generate Bob's Signal identity and PreKey bundle
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();
        let (bob_bundle, _bob_private) = PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        // Convert to libsignal bundle
        let libsignal_bundle = bob_bundle.to_libsignal_bundle().unwrap();

        // Alice performs X3DH initiator with Bob's bundle
        let (alice_secret, _alice_public) = gen_signal_identity();
        let result = libsignal_protocol::x3dh::x3dh_initiator(&alice_secret, &libsignal_bundle);
        assert!(result.is_ok(), "X3DH initiator should succeed with a valid bundle: {:?}", result.err());
    }

    #[test]
    fn test_libsignal_x3dh_shared_secret_matches() {
        // Generate Bob's Signal identity and PreKey bundle
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();
        let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        let libsignal_bundle = bob_bundle.to_libsignal_bundle().unwrap();

        // Alice initiates X3DH
        let (alice_secret, _alice_public) = gen_signal_identity();
        let alice_result = libsignal_protocol::x3dh::x3dh_initiator(&alice_secret, &libsignal_bundle).unwrap();

        // Bob responds: derive Alice's encoded public key from her secret
        let alice_pub_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&alice_secret);

        let bob_result = libsignal_protocol::x3dh::x3dh_responder(
            &bob_private.identity_secret,
            &bob_private.signed_prekey_secret,
            bob_private.prekey_secret.as_ref(),
            &alice_pub_encoded,
            &alice_result.ephemeral_public,
        );
        assert!(bob_result.is_ok(), "X3DH responder should succeed: {:?}", bob_result.err());

        // Shared secrets should match
        assert_eq!(alice_result.shared_secret, bob_result.unwrap());
    }

    #[test]
    fn test_real_session_encryption_decryption() {
        // This test uses the old simplified X3DH in SignalSession (Task 2 scope).
        // We keep it passing by generating a signal identity and passing it through.
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();

        // Generate Bob's Signal identity keypair
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();

        // Bob generates PreKey bundle with signal identity
        let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        // Alice creates session from Bob's bundle (simplified X3DH -- will be updated in Task 2)
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

        // Bob decrypts (this is a PreKey message -- has ephemeral prefix)
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

        // Generate Bob's Signal identity keypair
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();

        // Bob generates PreKey bundle
        let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

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
