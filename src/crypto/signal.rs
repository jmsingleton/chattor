use crate::error::{ChattorError, Result};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

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
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PreKeyPrivateMaterial {
    pub identity_secret: [u8; 32],       // X25519 private key bytes
    pub signed_prekey_secret: [u8; 32],  // X25519 private key bytes
    pub prekey_secret: Option<[u8; 32]>, // X25519 private key bytes
}

impl std::fmt::Debug for PreKeyPrivateMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreKeyPrivateMaterial")
            .field("identity_secret", &"[REDACTED]")
            .field("signed_prekey_secret", &"[REDACTED]")
            .field(
                "prekey_secret",
                &self.prekey_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
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

impl PreKeyBundle {
    /// Generate new PreKey bundle with random bytes (test-only).
    ///
    /// Creates a bundle with random (non-cryptographic) key material.
    /// Useful only for error-path testing where valid keys are not needed.
    /// For production use, see `generate_real()`.
    #[allow(dead_code)]
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
            .map_err(|()| ChattorError::Crypto("VXEdDSA signing failed".into()))?;

        // Generate one-time pre-key
        let prekey_pair = gen_keypair();
        let prekey_id = 1u32;

        // Decode 33-byte encoded public keys to raw 32-byte keys for internal storage
        let signed_prekey_raw = libsignal_protocol::utils::decode_public_key(
            &signed_prekey_pair.public,
        )
        .map_err(|_| ChattorError::Crypto("Failed to decode signed prekey public key".into()))?;
        let prekey_raw = libsignal_protocol::utils::decode_public_key(&prekey_pair.public)
            .map_err(|_| ChattorError::Crypto("Failed to decode prekey public key".into()))?;

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
    pub fn to_libsignal_bundle(&self) -> Result<libsignal_protocol::x3dh::PreKeyBundle> {
        use libsignal_protocol::utils::encode_public_key;

        // Encode identity key (32 bytes -> 33 bytes with 0x05 prefix)
        let identity_raw: [u8; 32] = self.identity_key[..].try_into().map_err(|_| {
            ChattorError::Crypto(format!(
                "Invalid identity key length: expected 32, got {}",
                self.identity_key.len()
            ))
        })?;
        let identity_encoded = encode_public_key(&identity_raw);

        // Encode signed prekey public key
        let spk_raw: [u8; 32] = self.signed_prekey.public_key[..].try_into().map_err(|_| {
            ChattorError::Crypto(format!(
                "Invalid signed prekey length: expected 32, got {}",
                self.signed_prekey.public_key.len()
            ))
        })?;
        let spk_encoded = encode_public_key(&spk_raw);

        // Convert signature to fixed-size array
        let signature: [u8; 96] = self.signed_prekey.signature[..].try_into().map_err(|_| {
            ChattorError::Crypto(format!(
                "Invalid signature length: expected 96, got {}",
                self.signed_prekey.signature.len()
            ))
        })?;

        // Convert one-time prekey if present
        let one_time_prekey = match &self.prekey {
            Some(pk) => {
                let pk_raw: [u8; 32] = pk.public_key[..].try_into().map_err(|_| {
                    ChattorError::Crypto(format!(
                        "Invalid prekey length: expected 32, got {}",
                        pk.public_key.len()
                    ))
                })?;
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
        use libsignal_protocol::utils::encode_public_key;
        use libsignal_protocol::vxeddsa::vxeddsa_verify;

        let identity_raw: [u8; 32] = self.identity_key[..]
            .try_into()
            .map_err(|_| ChattorError::Crypto("Invalid identity key length".into()))?;
        let identity_encoded = encode_public_key(&identity_raw);

        let spk_raw: [u8; 32] = self.signed_prekey.public_key[..]
            .try_into()
            .map_err(|_| ChattorError::Crypto("Invalid signed prekey length".into()))?;
        let spk_encoded = encode_public_key(&spk_raw);

        let signature: [u8; 96] = self.signed_prekey.signature[..]
            .try_into()
            .map_err(|_| ChattorError::Crypto("Invalid signature length".into()))?;

        Ok(vxeddsa_verify(&identity_encoded, &spk_encoded, &signature).is_some())
    }
}

// ---------------------------------------------------------------------------
// SignalSession — backed by libsignal-dezire Double Ratchet
// ---------------------------------------------------------------------------

/// Signal session for encryption/decryption using the Double Ratchet algorithm.
///
/// Wraps `libsignal_protocol::ratchet::RatchetState` to provide forward secrecy,
/// out-of-order message handling, and replay detection.
pub struct SignalSession {
    pub remote_onion: String,
    state: libsignal_protocol::ratchet::RatchetState,
    /// Associated data for encrypt/decrypt (alice_identity || bob_identity, both 33-byte encoded)
    associated_data: Vec<u8>,
    /// Only set for the first message (PreKey message)
    is_first_message: bool,
    /// Ephemeral public key from X3DH (needed for wire format of first message)
    ephemeral_public: Option<[u8; 33]>,
}

/// Serializable form for session persistence (serialization direction — uses references)
#[derive(Serialize)]
struct SerializableSessionRef<'a> {
    remote_onion: &'a str,
    state: &'a libsignal_protocol::ratchet::RatchetState,
    associated_data: &'a [u8],
    is_first_message: bool,
    /// Stored as Vec<u8> because [u8; 33] doesn't impl Serialize in serde (max 32)
    ephemeral_public: Option<Vec<u8>>,
}

/// Deserializable form for session persistence (deserialization direction — owns data)
#[derive(Deserialize)]
struct DeserializableSession {
    remote_onion: String,
    state: libsignal_protocol::ratchet::RatchetState,
    associated_data: Vec<u8>,
    is_first_message: bool,
    /// Stored as Vec<u8> because [u8; 33] doesn't impl Deserialize in serde (max 32)
    ephemeral_public: Option<Vec<u8>>,
}

impl SignalSession {
    /// Create session from PreKey bundle (initiator / Alice side) with real X3DH + Double Ratchet.
    ///
    /// Performs full X3DH key agreement with the remote peer's PreKey bundle,
    /// then initializes a Double Ratchet sender state.
    ///
    /// # Arguments
    /// * `remote_onion` - The remote peer's .onion address
    /// * `bundle` - The remote peer's PreKey bundle
    /// * `_private_material` - Unused (kept for API compat during transition)
    /// * `signal_identity_secret` - Our X25519 Signal identity private key (32 bytes)
    ///
    /// # Returns
    /// `(session, associated_data, ephemeral_public)` — the caller needs `ephemeral_public`
    /// and `associated_data` for the wire format.
    pub fn from_prekey_bundle_real(
        remote_onion: String,
        bundle: &PreKeyBundle,
        _private_material: &PreKeyPrivateMaterial,
        signal_identity_secret: &[u8; 32],
    ) -> Result<(Self, Vec<u8>, [u8; 33])> {
        use libsignal_protocol::utils::encode_public_key;

        // Convert our PreKeyBundle to libsignal format
        let ls_bundle = bundle.to_libsignal_bundle()?;

        // Perform full X3DH as initiator
        let x3dh_result =
            libsignal_protocol::x3dh::x3dh_initiator(signal_identity_secret, &ls_bundle)
                .map_err(|e| ChattorError::Crypto(format!("X3DH initiator failed: {:?}", e)))?;

        // Decode Bob's signed prekey public from the bundle (raw 32 bytes)
        let bob_spk_raw: [u8; 32] = bundle.signed_prekey.public_key[..]
            .try_into()
            .map_err(|_| ChattorError::Crypto("Invalid signed prekey length".into()))?;
        let bob_spk_pubkey = X25519PublicKey::from(bob_spk_raw);

        // Initialize Double Ratchet sender state
        let ratchet_state = libsignal_protocol::ratchet::init_sender_state(
            x3dh_result.shared_secret,
            bob_spk_pubkey,
        )
        .map_err(|e| ChattorError::Crypto(format!("Ratchet init_sender failed: {:?}", e)))?;

        // Build associated data: encode(alice_identity) || encode(bob_identity)
        // Alice's identity public key (derived from our secret)
        let alice_identity_encoded =
            libsignal_protocol::vxeddsa::gen_pubkey(signal_identity_secret);
        // Bob's identity key (from bundle, raw 32 bytes -> encode to 33 bytes)
        let bob_identity_raw: [u8; 32] = bundle.identity_key[..]
            .try_into()
            .map_err(|_| ChattorError::Crypto("Invalid identity key length".into()))?;
        let bob_identity_encoded = encode_public_key(&bob_identity_raw);

        let mut associated_data = Vec::with_capacity(66);
        associated_data.extend_from_slice(&alice_identity_encoded);
        associated_data.extend_from_slice(&bob_identity_encoded);

        let ephemeral_public = x3dh_result.ephemeral_public;

        Ok((
            SignalSession {
                remote_onion,
                state: ratchet_state,
                associated_data: associated_data.clone(),
                is_first_message: true,
                ephemeral_public: Some(ephemeral_public),
            },
            associated_data,
            ephemeral_public,
        ))
    }

    /// Create session from received PreKey message (responder / Bob side) with real X3DH + Double Ratchet.
    ///
    /// Performs the X3DH responder side, then initializes a Double Ratchet receiver state.
    ///
    /// # Arguments
    /// * `remote_onion` - The remote peer's .onion address
    /// * `private_material` - Our PreKey private material (identity, signed prekey, one-time prekey)
    /// * `alice_identity_public` - Alice's identity public key (33-byte encoded with 0x05 prefix)
    /// * `alice_ephemeral_public` - Alice's ephemeral public key (33-byte encoded with 0x05 prefix)
    ///
    /// # Returns
    /// `(session, associated_data)`
    pub fn from_prekey_message_real(
        remote_onion: String,
        private_material: &PreKeyPrivateMaterial,
        alice_identity_public: &[u8; 33],
        alice_ephemeral_public: &[u8; 33],
    ) -> Result<(Self, Vec<u8>)> {
        // Perform X3DH responder
        let shared_secret = libsignal_protocol::x3dh::x3dh_responder(
            &private_material.identity_secret,
            &private_material.signed_prekey_secret,
            private_material.prekey_secret.as_ref(),
            alice_identity_public,
            alice_ephemeral_public,
        )
        .map_err(|e| ChattorError::Crypto(format!("X3DH responder failed: {:?}", e)))?;

        // Build the receiver's DH keypair from the signed prekey
        let spk_secret = StaticSecret::from(private_material.signed_prekey_secret);
        let spk_public = X25519PublicKey::from(&spk_secret);

        // Initialize Double Ratchet receiver state
        let ratchet_state = libsignal_protocol::ratchet::init_receiver_state(
            shared_secret,
            (spk_secret, spk_public),
        );

        // Build associated data: encode(alice_identity) || encode(bob_identity)
        // Alice's identity is already encoded (33 bytes)
        // Bob's identity public key (derived from our secret)
        let bob_identity_encoded =
            libsignal_protocol::vxeddsa::gen_pubkey(&private_material.identity_secret);

        let mut associated_data = Vec::with_capacity(66);
        associated_data.extend_from_slice(alice_identity_public);
        associated_data.extend_from_slice(&bob_identity_encoded);

        Ok((
            SignalSession {
                remote_onion,
                state: ratchet_state,
                associated_data: associated_data.clone(),
                is_first_message: false,
                ephemeral_public: None,
            },
            associated_data,
        ))
    }

    /// Get the associated data for this session.
    #[allow(dead_code)]
    pub fn associated_data(&self) -> &[u8] {
        &self.associated_data
    }

    /// Get the ephemeral public key (only available before first encrypt on initiator side).
    #[allow(dead_code)]
    pub fn ephemeral_public(&self) -> Option<&[u8; 33]> {
        self.ephemeral_public.as_ref()
    }

    /// Encrypt plaintext using the Double Ratchet.
    ///
    /// # Returns
    /// `(header, ciphertext, is_first_message)` where:
    /// - `header` is the encrypted ratchet header
    /// - `ciphertext` is the encrypted message
    /// - `is_first_message` indicates this is the first message (PreKey message)
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>, bool)> {
        let (header, ciphertext) =
            libsignal_protocol::ratchet::encrypt(&mut self.state, plaintext, &self.associated_data)
                .map_err(|e| ChattorError::Crypto(format!("Ratchet encrypt failed: {:?}", e)))?;

        let was_first = self.is_first_message;
        self.is_first_message = false;

        Ok((header, ciphertext, was_first))
    }

    /// Decrypt ciphertext using the Double Ratchet.
    ///
    /// # Arguments
    /// * `header` - The encrypted ratchet header
    /// * `ciphertext` - The encrypted message body
    pub fn decrypt(&mut self, header: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        libsignal_protocol::ratchet::decrypt(
            &mut self.state,
            header,
            ciphertext,
            &self.associated_data,
        )
        .map_err(|e| ChattorError::Crypto(format!("Ratchet decrypt failed: {:?}", e)))
    }

    /// Serialize session state for storage (JSON).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let s = SerializableSessionRef {
            remote_onion: &self.remote_onion,
            state: &self.state,
            associated_data: &self.associated_data,
            is_first_message: self.is_first_message,
            ephemeral_public: self.ephemeral_public.map(|ep| ep.to_vec()),
        };
        serde_json::to_vec(&s).map_err(|e| ChattorError::Crypto(format!("session serialize: {e}")))
    }

    /// Deserialize session state from storage (JSON).
    pub fn from_bytes(_remote_onion: String, data: Vec<u8>) -> Result<Self> {
        let s: DeserializableSession = serde_json::from_slice(&data)
            .map_err(|e| ChattorError::Crypto(format!("session deserialize: {e}")))?;
        let ephemeral_public = match s.ephemeral_public {
            Some(v) => {
                let arr: [u8; 33] = v
                    .try_into()
                    .map_err(|_| ChattorError::Crypto("Invalid ephemeral_public length".into()))?;
                Some(arr)
            }
            None => None,
        };
        Ok(Self {
            remote_onion: s.remote_onion,
            state: s.state,
            associated_data: s.associated_data,
            is_first_message: s.is_first_message,
            ephemeral_public,
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

    /// Helper: set up a complete Alice-Bob session pair using real X3DH + Double Ratchet.
    /// Returns (alice_session, bob_session).
    fn setup_session_pair() -> (SignalSession, SignalSession) {
        let (alice_signal_secret, _alice_signal_public) = gen_signal_identity();
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();

        // Bob generates his PreKey bundle
        let (bob_bundle, bob_private) =
            PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        // Alice creates session from Bob's bundle (initiator)
        let (alice_session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private, // unused but required by signature
            &alice_signal_secret,
        )
        .unwrap();

        // Bob creates session from Alice's PreKey message (responder)
        // He needs Alice's identity public and ephemeral public (both 33-byte encoded)
        let alice_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&alice_signal_secret);
        let (bob_session, _bob_ad) = SignalSession::from_prekey_message_real(
            "alice.onion".into(),
            &bob_private,
            &alice_identity_encoded,
            &ephemeral_public,
        )
        .unwrap();

        (alice_session, bob_session)
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
        let (bundle, private_material) =
            PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

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
        let (bundle, _private_material) =
            PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

        // Verify the VXEdDSA signature on the signed prekey
        assert!(
            bundle.verify_signature().unwrap(),
            "VXEdDSA signature should verify"
        );
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
        assert!(
            !bundle.verify_signature().unwrap(),
            "Tampered bundle should fail verification"
        );
    }

    #[test]
    fn test_vxeddsa_signature_rejects_wrong_identity() {
        let (signal_secret, signal_public) = gen_signal_identity();
        let (mut bundle, _) = PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

        // Replace identity key with a different key
        let (_, other_public) = gen_signal_identity();
        bundle.identity_key = other_public.to_vec();

        // Signature should fail because identity key doesn't match the signer
        assert!(
            !bundle.verify_signature().unwrap(),
            "Wrong identity key should fail verification"
        );
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
        let (bob_bundle, _bob_private) =
            PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        // Convert to libsignal bundle
        let libsignal_bundle = bob_bundle.to_libsignal_bundle().unwrap();

        // Alice performs X3DH initiator with Bob's bundle
        let (alice_secret, _alice_public) = gen_signal_identity();
        let result = libsignal_protocol::x3dh::x3dh_initiator(&alice_secret, &libsignal_bundle);
        assert!(
            result.is_ok(),
            "X3DH initiator should succeed with a valid bundle: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_libsignal_x3dh_shared_secret_matches() {
        // Generate Bob's Signal identity and PreKey bundle
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();
        let (bob_bundle, bob_private) =
            PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        let libsignal_bundle = bob_bundle.to_libsignal_bundle().unwrap();

        // Alice initiates X3DH
        let (alice_secret, _alice_public) = gen_signal_identity();
        let alice_result =
            libsignal_protocol::x3dh::x3dh_initiator(&alice_secret, &libsignal_bundle).unwrap();

        // Bob responds: derive Alice's encoded public key from her secret
        let alice_pub_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&alice_secret);

        let bob_result = libsignal_protocol::x3dh::x3dh_responder(
            &bob_private.identity_secret,
            &bob_private.signed_prekey_secret,
            bob_private.prekey_secret.as_ref(),
            &alice_pub_encoded,
            &alice_result.ephemeral_public,
        );
        assert!(
            bob_result.is_ok(),
            "X3DH responder should succeed: {:?}",
            bob_result.err()
        );

        // Shared secrets should match
        assert_eq!(alice_result.shared_secret, bob_result.unwrap());
    }

    // -----------------------------------------------------------------------
    // Double Ratchet session tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_real_session_encrypt_decrypt_roundtrip() {
        let (mut alice, mut bob) = setup_session_pair();

        // Alice sends first message (PreKey message)
        let plaintext = b"Hello Bob!";
        let (header, ciphertext, is_prekey) = alice.encrypt(plaintext).unwrap();

        assert!(is_prekey, "First message should be PreKey type");
        assert_ne!(
            ciphertext, plaintext,
            "Ciphertext should differ from plaintext"
        );

        // Bob decrypts
        let decrypted = bob.decrypt(&header, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_bidirectional_messaging() {
        let (mut alice, mut bob) = setup_session_pair();

        // Alice -> Bob
        let (h1, c1, _) = alice.encrypt(b"Hello Bob!").unwrap();
        let d1 = bob.decrypt(&h1, &c1).unwrap();
        assert_eq!(d1, b"Hello Bob!");

        // Bob -> Alice
        let (h2, c2, _) = bob.encrypt(b"Hello Alice!").unwrap();
        let d2 = alice.decrypt(&h2, &c2).unwrap();
        assert_eq!(d2, b"Hello Alice!");

        // Alice -> Bob again
        let (h3, c3, is_prekey) = alice.encrypt(b"How are you?").unwrap();
        assert!(!is_prekey, "Not first message anymore");
        let d3 = bob.decrypt(&h3, &c3).unwrap();
        assert_eq!(d3, b"How are you?");
    }

    #[test]
    fn test_multiple_messages_same_direction() {
        let (mut alice, mut bob) = setup_session_pair();

        let (h1, c1, _) = alice.encrypt(b"Message 1").unwrap();
        let (h2, c2, _) = alice.encrypt(b"Message 2").unwrap();
        let (h3, c3, _) = alice.encrypt(b"Message 3").unwrap();

        // Bob decrypts in order
        assert_eq!(bob.decrypt(&h1, &c1).unwrap(), b"Message 1");
        assert_eq!(bob.decrypt(&h2, &c2).unwrap(), b"Message 2");
        assert_eq!(bob.decrypt(&h3, &c3).unwrap(), b"Message 3");
    }

    #[test]
    fn test_out_of_order_messages() {
        let (mut alice, mut bob) = setup_session_pair();

        let (h1, c1, _) = alice.encrypt(b"Message 1").unwrap();
        let (h2, c2, _) = alice.encrypt(b"Message 2").unwrap();
        let (h3, c3, _) = alice.encrypt(b"Message 3").unwrap();

        // Bob receives out of order: 1, 3, 2
        assert_eq!(bob.decrypt(&h1, &c1).unwrap(), b"Message 1");
        assert_eq!(bob.decrypt(&h3, &c3).unwrap(), b"Message 3");
        assert_eq!(bob.decrypt(&h2, &c2).unwrap(), b"Message 2");
    }

    #[test]
    fn test_session_serialization_roundtrip() {
        let (mut alice, mut bob) = setup_session_pair();

        // Alice sends two messages
        let (h1, c1, _) = alice.encrypt(b"Message 1").unwrap();
        let (h2, c2, _) = alice.encrypt(b"Message 2").unwrap();

        // Serialize Alice's session
        let serialized = alice.to_bytes().unwrap();

        // Deserialize into a new session
        let mut alice_restored = SignalSession::from_bytes("bob.onion".into(), serialized).unwrap();

        // Send another message from restored session
        let (h3, c3, is_prekey) = alice_restored.encrypt(b"Message 3").unwrap();
        assert!(!is_prekey, "Restored session should not be first message");

        // Bob decrypts all three
        assert_eq!(bob.decrypt(&h1, &c1).unwrap(), b"Message 1");
        assert_eq!(bob.decrypt(&h2, &c2).unwrap(), b"Message 2");
        assert_eq!(bob.decrypt(&h3, &c3).unwrap(), b"Message 3");
    }

    #[test]
    fn test_session_serialization_both_sides() {
        let (mut alice, mut bob) = setup_session_pair();

        // Exchange a message
        let (h1, c1, _) = alice.encrypt(b"Hello").unwrap();
        bob.decrypt(&h1, &c1).unwrap();

        // Serialize both
        let alice_bytes = alice.to_bytes().unwrap();
        let bob_bytes = bob.to_bytes().unwrap();

        // Restore both
        let mut alice2 = SignalSession::from_bytes("bob.onion".into(), alice_bytes).unwrap();
        let mut bob2 = SignalSession::from_bytes("alice.onion".into(), bob_bytes).unwrap();

        // Continue conversation
        let (h2, c2, _) = bob2.encrypt(b"World").unwrap();
        assert_eq!(alice2.decrypt(&h2, &c2).unwrap(), b"World");
    }

    #[test]
    fn test_tampered_ciphertext_rejected() {
        let (mut alice, mut bob) = setup_session_pair();

        let (header, mut ciphertext, _) = alice.encrypt(b"Secret message").unwrap();

        // Tamper with ciphertext
        if !ciphertext.is_empty() {
            ciphertext[0] ^= 0xFF;
        }

        let result = bob.decrypt(&header, &ciphertext);
        assert!(
            result.is_err(),
            "Tampered ciphertext should fail decryption"
        );
    }

    #[test]
    fn test_tampered_header_rejected() {
        let (mut alice, mut bob) = setup_session_pair();

        let (mut header, ciphertext, _) = alice.encrypt(b"Secret message").unwrap();

        // Tamper with header
        if !header.is_empty() {
            header[0] ^= 0xFF;
        }

        let result = bob.decrypt(&header, &ciphertext);
        assert!(result.is_err(), "Tampered header should fail decryption");
    }

    #[test]
    fn test_replay_detection() {
        let (mut alice, mut bob) = setup_session_pair();

        let (header, ciphertext, _) = alice.encrypt(b"Once only").unwrap();

        // First decrypt succeeds
        let d1 = bob.decrypt(&header, &ciphertext).unwrap();
        assert_eq!(d1, b"Once only");

        // Replay should fail
        let result = bob.decrypt(&header, &ciphertext);
        assert!(result.is_err(), "Replayed message should be rejected");
    }

    #[test]
    fn test_is_first_message_flag() {
        let (mut alice, _bob) = setup_session_pair();

        let (_, _, first) = alice.encrypt(b"First").unwrap();
        assert!(first, "First encrypt should set is_first_message=true");

        let (_, _, second) = alice.encrypt(b"Second").unwrap();
        assert!(!second, "Second encrypt should set is_first_message=false");
    }

    #[test]
    fn test_associated_data_consistency() {
        let (alice, bob) = setup_session_pair();

        // AD should be 66 bytes (two 33-byte encoded keys)
        assert_eq!(alice.associated_data().len(), 66);
        assert_eq!(bob.associated_data().len(), 66);

        // Both sides should have the same AD (alice_ik || bob_ik)
        assert_eq!(
            alice.associated_data(),
            bob.associated_data(),
            "Both sides should compute identical associated data"
        );
    }

    #[test]
    fn test_ephemeral_public_available_on_initiator() {
        let (alice_signal_secret, _) = gen_signal_identity();
        let (bob_signal_secret, bob_signal_public) = gen_signal_identity();
        let (bob_bundle, bob_private) =
            PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

        let (session, _, eph) = SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_signal_secret,
        )
        .unwrap();

        // Ephemeral public should be a 33-byte encoded key
        assert_eq!(eph.len(), 33);
        assert_eq!(eph[0], 0x05);

        // Also accessible from session
        assert!(session.ephemeral_public().is_some());
    }
}
