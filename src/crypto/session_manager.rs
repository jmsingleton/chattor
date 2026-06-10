use crate::crypto::prekey_store::PreKeyStore;
use crate::crypto::session_store::SessionStore;
use crate::crypto::signal::{PreKeyBundle, PreKeyPrivateMaterial, SignalSession};
use crate::db::Database;
use crate::error::{ChattorError, Result};
use crate::protocol::message::{PlaintextPayload, SignalMessageType, TextMessage, X3DHInitData};
use base64::Engine as _;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Crypto fields for an outgoing `TextMessage`. `x3dh_init` is Some only for the
/// initiator handshake (`establish_from_accept`); None for normal sends.
pub struct OutgoingCrypto {
    pub header: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub is_prekey: bool,
    pub x3dh_init: Option<X3DHInitData>,
}

/// The single facade for Signal session orchestration. Owns sessions, X3DH
/// establishment, the raw libsignal calls, and PreKey material (via PreKeyStore).
pub struct SessionManager<'a> {
    db: &'a Database,
}

impl<'a> SessionManager<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Encrypt `plaintext` for an established session with `peer`.
    /// Ok(None) = no session (caller decides whether to drop or error).
    pub fn encrypt_for(&self, peer: &str, plaintext: &[u8]) -> Result<Option<OutgoingCrypto>> {
        let store = SessionStore::new(self.db);
        let mut session = match store.load_session(peer)? {
            Some(s) => s,
            None => return Ok(None),
        };
        let (header, ciphertext, is_prekey) = session.encrypt(plaintext)?;
        store.store_session(&session)?;
        Ok(Some(OutgoingCrypto {
            header,
            ciphertext,
            is_prekey,
            x3dh_init: None,
        }))
    }

    /// Decrypt an incoming `TextMessage`, establishing a session from stored PreKey
    /// material if needed (consuming it on success).
    /// Ok(Some)=payload; Ok(None)=no session & not a PreKey message (caller drops).
    pub fn decrypt_incoming(&self, msg: &TextMessage) -> Result<Option<PlaintextPayload>> {
        let store = SessionStore::new(self.db);
        let is_prekey = msg.signal_type == SignalMessageType::PrekeyMessage;
        let header = B64
            .decode(&msg.signal_header)
            .map_err(|e| ChattorError::Crypto(format!("Failed to decode header: {}", e)))?;
        let ciphertext = B64
            .decode(&msg.signal_ciphertext)
            .map_err(|e| ChattorError::Crypto(format!("Failed to decode ciphertext: {}", e)))?;

        let plaintext = match store.load_session(&msg.from_onion)? {
            Some(mut session) => {
                let pt = session.decrypt(&header, &ciphertext)?;
                store.store_session(&session)?;
                pt
            }
            None if is_prekey => {
                let x3dh_init = msg.x3dh_init.as_ref().ok_or_else(|| {
                    ChattorError::Crypto(format!(
                        "PreKey message from {} missing X3DH init data",
                        msg.from_onion
                    ))
                })?;
                let alice_identity_public: [u8; 33] = B64
                    .decode(&x3dh_init.sender_identity_key)
                    .map_err(|e| {
                        ChattorError::Crypto(format!("Failed to decode sender identity key: {}", e))
                    })?
                    .try_into()
                    .map_err(|_| {
                        ChattorError::Crypto("Sender identity key has wrong length (expected 33)".into())
                    })?;
                let alice_ephemeral_public: [u8; 33] = B64
                    .decode(&x3dh_init.sender_ephemeral_key)
                    .map_err(|e| {
                        ChattorError::Crypto(format!("Failed to decode sender ephemeral key: {}", e))
                    })?
                    .try_into()
                    .map_err(|_| {
                        ChattorError::Crypto("Sender ephemeral key has wrong length (expected 33)".into())
                    })?;

                let prekey_store = PreKeyStore::new(self.db);
                let material = prekey_store.load(&msg.from_onion)?.ok_or_else(|| {
                    ChattorError::Crypto(format!(
                        "No stored PreKey material for {}",
                        msg.from_onion
                    ))
                })?;

                let (mut session, _ad) = SignalSession::from_prekey_message_real(
                    msg.from_onion.clone(),
                    &material,
                    &alice_identity_public,
                    &alice_ephemeral_public,
                )?;
                let pt = session.decrypt(&header, &ciphertext)?;
                store.store_session(&session)?;
                prekey_store.delete(&msg.from_onion)?;
                pt
            }
            None => return Ok(None),
        };

        let payload = serde_json::from_slice::<PlaintextPayload>(&plaintext)
            .map_err(|e| ChattorError::Crypto(format!("Failed to parse payload: {}", e)))?;
        Ok(Some(payload))
    }

    /// Acceptor side: generate a dedicated Signal identity + PreKey bundle, persist the
    /// private material, and return the bundle for the accept message.
    pub fn create_accept_bundle(&self, peer: &str) -> Result<PreKeyBundle> {
        let signal_identity = libsignal_protocol::vxeddsa::gen_keypair();
        let signal_identity_public_raw =
            libsignal_protocol::utils::decode_public_key(&signal_identity.public).map_err(|_| {
                ChattorError::Crypto("Failed to decode signal identity public key".into())
            })?;
        let (bundle, private_keys) =
            PreKeyBundle::generate_real(&signal_identity.secret, &signal_identity_public_raw)?;
        PreKeyStore::new(self.db).store(peer, &private_keys, &signal_identity.secret, now_secs())?;
        Ok(bundle)
    }

    /// Initiator side: verify the bundle's VXEdDSA self-signature, establish a session
    /// (loading or generating our signal identity secret), store it, and encrypt the
    /// handshake PreKey message. Returns its crypto fields (with x3dh_init).
    pub fn establish_from_accept(&self, peer: &str, bundle: &PreKeyBundle) -> Result<OutgoingCrypto> {
        if !bundle.verify_signature()? {
            return Err(ChattorError::Crypto(format!(
                "PreKeyBundle from {} has invalid VXEdDSA signature",
                peer
            )));
        }
        let prekey_store = PreKeyStore::new(self.db);
        let signal_identity_secret = match prekey_store.load_signal_identity_secret(peer)? {
            Some(s) => s,
            None => libsignal_protocol::vxeddsa::gen_keypair().secret,
        };
        let dummy_private = PreKeyPrivateMaterial {
            identity_secret: [0u8; 32],
            signed_prekey_secret: [0u8; 32],
            prekey_secret: None,
        };
        let (mut session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
            peer.to_string(),
            bundle,
            &dummy_private,
            &signal_identity_secret,
        )?;
        let our_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&signal_identity_secret);

        let store = SessionStore::new(self.db);
        store.store_session(&session)?;

        let handshake = PlaintextPayload {
            content: String::new(),
            sent_at: now_secs(),
            message_type: "handshake".to_string(),
            ephemeral_ttl: None,
        };
        let plaintext = serde_json::to_vec(&handshake)
            .map_err(|e| ChattorError::Crypto(format!("Handshake serialize: {}", e)))?;
        let (header, ciphertext, is_prekey) = session.encrypt(&plaintext)?;
        store.store_session(&session)?;

        let x3dh_init = if is_prekey {
            Some(X3DHInitData {
                sender_identity_key: B64.encode(our_identity_encoded),
                sender_ephemeral_key: B64.encode(ephemeral_public),
            })
        } else {
            None
        };
        Ok(OutgoingCrypto {
            header,
            ciphertext,
            is_prekey,
            x3dh_init,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use uuid::Uuid;

    fn temp_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        (db, temp)
    }

    fn text_msg(from: &str, to: &str, oc: &OutgoingCrypto) -> TextMessage {
        TextMessage {
            from_onion: from.to_string(),
            to_onion: to.to_string(),
            signal_header: B64.encode(&oc.header),
            signal_ciphertext: B64.encode(&oc.ciphertext),
            signal_type: if oc.is_prekey {
                SignalMessageType::PrekeyMessage
            } else {
                SignalMessageType::Message
            },
            timestamp: now_secs(),
            message_id: Uuid::new_v4(),
            x3dh_init: oc.x3dh_init.clone(),
        }
    }

    // Keystone: the full handshake + bidirectional messaging through ONLY the facade.
    #[test]
    fn test_full_handshake_through_facade() {
        let (alice_db, _a) = temp_db();
        let (bob_db, _b) = temp_db();
        let alice = "alice.onion";
        let bob = "bob.onion";

        // 1. Bob accepts: generates a bundle and persists his material.
        let bundle = SessionManager::new(&bob_db).create_accept_bundle(alice).unwrap();
        assert!(PreKeyStore::new(&bob_db).load(alice).unwrap().is_some());

        // 2. Alice receives the accept bundle: establishes a session + handshake.
        let handshake = SessionManager::new(&alice_db)
            .establish_from_accept(bob, &bundle)
            .unwrap();
        assert!(handshake.is_prekey);
        assert!(handshake.x3dh_init.is_some());

        // 3. Bob receives the handshake PreKey message: establishes his session,
        //    consumes the material, returns the (handshake) payload.
        let hs_msg = text_msg(alice, bob, &handshake);
        let payload = SessionManager::new(&bob_db)
            .decrypt_incoming(&hs_msg)
            .unwrap()
            .unwrap();
        assert_eq!(payload.message_type, "handshake");
        assert!(PreKeyStore::new(&bob_db).load(alice).unwrap().is_none());

        // 4. Bidirectional messaging now works.
        let pt = serde_json::to_vec(&PlaintextPayload {
            content: "hello bob".to_string(),
            sent_at: now_secs(),
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        })
        .unwrap();
        let oc = SessionManager::new(&alice_db).encrypt_for(bob, &pt).unwrap().unwrap();
        assert!(oc.x3dh_init.is_none());
        let msg = text_msg(alice, bob, &oc);
        let got = SessionManager::new(&bob_db).decrypt_incoming(&msg).unwrap().unwrap();
        assert_eq!(got.content, "hello bob");
    }

    #[test]
    fn test_encrypt_for_no_session_returns_none() {
        let (db, _t) = temp_db();
        let result = SessionManager::new(&db).encrypt_for("stranger.onion", b"hi").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_decrypt_incoming_no_session_not_prekey_returns_none() {
        let (db, _t) = temp_db();
        let msg = TextMessage {
            from_onion: "stranger.onion".to_string(),
            to_onion: "me.onion".to_string(),
            signal_header: B64.encode([1u8, 2, 3]),
            signal_ciphertext: B64.encode([4u8, 5, 6]),
            signal_type: SignalMessageType::Message,
            timestamp: now_secs(),
            message_id: Uuid::new_v4(),
            x3dh_init: None,
        };
        let result = SessionManager::new(&db).decrypt_incoming(&msg).unwrap();
        assert!(result.is_none());
    }
}
