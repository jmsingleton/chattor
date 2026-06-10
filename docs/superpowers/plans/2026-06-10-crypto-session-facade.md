# Crypto-Session Facade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route all Signal/X3DH session orchestration through one `crypto::SessionManager` facade (backed by a typed `crypto::PreKeyStore`), migrate the four inline production sites to it, and delete the dead `net::MessageSender`/`MessageReceiver` scaffolding — with no change to wire bytes or crypto behavior.

**Architecture:** Two new modules under `src/crypto/`: `prekey_store.rs` (typed accessor that owns the `prekey_*:/signal_identity_secret:/prekey_created_at:<onion>` app_settings keys) and `session_manager.rs` (orchestration facade: `encrypt_for`, `decrypt_incoming`, `create_accept_bundle`, `establish_from_accept`). Build both with TDD (incl. a keystone full-handshake-through-the-facade test), then replace the inline crypto at each site with a facade call, preserving each caller's existing error/drop behavior. Finally delegate `cleanup_stale_prekey_material` to `PreKeyStore` and delete the dead `net/` files.

**Tech Stack:** Rust, libsignal-dezire (`libsignal_protocol`), rusqlite/SQLCipher, serde_json, base64, cargo.

**Spec:** `docs/superpowers/specs/2026-06-10-crypto-session-facade-design.md`

**Security note:** This is behavior-preserving but security-sensitive. Every site migration must keep the caller's existing error handling (daemon → RpcResponse error; TUI → silent drop) and the on-the-wire `TextMessage` fields identical (notably `x3dh_init: None` for normal sends). The existing `signal.rs`/`session_store.rs` primitives are NOT modified.

---

## Reference: primitive signatures (from `src/crypto/signal.rs`)

```rust
// PreKeyBundle impl:
PreKeyBundle::generate_real(signal_identity_secret: &[u8;32], signal_identity_public: &[u8;32])
    -> Result<(PreKeyBundle, PreKeyPrivateMaterial)>
bundle.verify_signature() -> Result<bool>

// SignalSession impl:
SignalSession::from_prekey_bundle_real(remote_onion: String, bundle: &PreKeyBundle,
    _private_material: &PreKeyPrivateMaterial, signal_identity_secret: &[u8;32])
    -> Result<(SignalSession, Vec<u8> /*ad*/, [u8;33] /*ephemeral_public*/)>
SignalSession::from_prekey_message_real(remote_onion: String, private_material: &PreKeyPrivateMaterial,
    alice_identity_public: &[u8;33], alice_ephemeral_public: &[u8;33])
    -> Result<(SignalSession, Vec<u8> /*ad*/)>
session.encrypt(&[u8]) -> Result<(Vec<u8> /*header*/, Vec<u8> /*ciphertext*/, bool /*is_prekey*/)>
session.decrypt(header: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>>

// PreKeyPrivateMaterial { identity_secret: [u8;32], signed_prekey_secret: [u8;32], prekey_secret: Option<[u8;32]> }

// raw libsignal (used only inside the facade):
libsignal_protocol::vxeddsa::gen_keypair()           // -> keypair with .secret: [u8;32], .public
libsignal_protocol::vxeddsa::gen_pubkey(&[u8;32])    // -> [u8;33] encoded public
libsignal_protocol::utils::decode_public_key(&public) // -> Result<[u8;32]>
```

`SessionStore::new(&Database)` with `.load_session(onion) -> Result<Option<SignalSession>>`,
`.store_session(&SignalSession) -> Result<()>`.

Protocol types (`src/protocol/message.rs`): `TextMessage { from_onion, to_onion, signal_header,
signal_ciphertext, signal_type, timestamp, message_id, x3dh_init }`,
`X3DHInitData { sender_identity_key: String, sender_ephemeral_key: String }`,
`PlaintextPayload { content, sent_at, message_type, ephemeral_ttl }`,
`SignalMessageType::{PrekeyMessage, Message}`.

## Reference: the app_settings key format (owned by PreKeyStore)

```
prekey_identity:<onion>          base64([u8;32])   identity_secret
prekey_spk:<onion>               base64([u8;32])   signed_prekey_secret
prekey_opk:<onion>               base64([u8;32])   prekey_secret (omitted if None)
signal_identity_secret:<onion>   base64([u8;32])   our X3DH signal identity secret
prekey_created_at:<onion>        decimal seconds   creation timestamp (for TTL cleanup)
```

---

### Task 1: `PreKeyStore` — typed establishment-material accessor (TDD)

**Files:**
- Create: `src/crypto/prekey_store.rs`
- Modify: `src/crypto/mod.rs`

- [ ] **Step 1: Wire the module**

In `src/crypto/mod.rs`, add (keep alphabetical-ish with existing `pub mod` lines):
```rust
pub mod prekey_store;
```
and add to the re-export line group:
```rust
pub use prekey_store::PreKeyStore;
```

- [ ] **Step 2: Write `src/crypto/prekey_store.rs` with implementation and tests**

```rust
use crate::crypto::signal::PreKeyPrivateMaterial;
use crate::db::Database;
use crate::error::{ChattorError, Result};
use base64::Engine as _;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Typed accessor for X3DH establishment material persisted in `app_settings`.
/// Owns the key-string format so it lives in exactly one place.
pub struct PreKeyStore<'a> {
    db: &'a Database,
}

impl<'a> PreKeyStore<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
                (key, value),
            )
            .map_err(|e| ChattorError::Database(format!("PreKeyStore set failed: {}", e)))?;
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.db.connection().query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ChattorError::Database(format!("PreKeyStore get failed: {}", e))),
        }
    }

    fn decode32(b64: &str, what: &str) -> Result<[u8; 32]> {
        B64.decode(b64)
            .map_err(|e| ChattorError::Crypto(format!("Failed to decode {}: {}", what, e)))?
            .try_into()
            .map_err(|_| ChattorError::Crypto(format!("{} has wrong length", what)))
    }

    /// Persist private material + signal identity secret + creation timestamp for `peer`.
    pub fn store(
        &self,
        peer: &str,
        material: &PreKeyPrivateMaterial,
        signal_identity_secret: &[u8; 32],
        created_at: i64,
    ) -> Result<()> {
        self.set(
            &format!("prekey_identity:{}", peer),
            &B64.encode(material.identity_secret),
        )?;
        self.set(
            &format!("prekey_spk:{}", peer),
            &B64.encode(material.signed_prekey_secret),
        )?;
        if let Some(opk) = material.prekey_secret {
            self.set(&format!("prekey_opk:{}", peer), &B64.encode(opk))?;
        }
        self.set(
            &format!("signal_identity_secret:{}", peer),
            &B64.encode(signal_identity_secret),
        )?;
        self.set(
            &format!("prekey_created_at:{}", peer),
            &format!("{}", created_at),
        )?;
        Ok(())
    }

    /// Load the stored `PreKeyPrivateMaterial` for `peer` (None if the identity row is absent).
    pub fn load(&self, peer: &str) -> Result<Option<PreKeyPrivateMaterial>> {
        let identity_b64 = match self.get(&format!("prekey_identity:{}", peer))? {
            Some(v) => v,
            None => return Ok(None),
        };
        let spk_b64 = self
            .get(&format!("prekey_spk:{}", peer))?
            .ok_or_else(|| ChattorError::Crypto(format!("Missing PreKey SPK for {}", peer)))?;
        let opk = match self.get(&format!("prekey_opk:{}", peer))? {
            Some(b64) => Some(Self::decode32(&b64, "PreKey OPK")?),
            None => None,
        };
        Ok(Some(PreKeyPrivateMaterial {
            identity_secret: Self::decode32(&identity_b64, "PreKey identity")?,
            signed_prekey_secret: Self::decode32(&spk_b64, "PreKey SPK")?,
            prekey_secret: opk,
        }))
    }

    /// Load the stored signal identity secret for `peer` (None if absent).
    pub fn load_signal_identity_secret(&self, peer: &str) -> Result<Option<[u8; 32]>> {
        match self.get(&format!("signal_identity_secret:{}", peer))? {
            Some(b64) => Ok(Some(Self::decode32(&b64, "signal identity secret")?)),
            None => Ok(None),
        }
    }

    /// Delete all establishment material for `peer` (idempotent).
    pub fn delete(&self, peer: &str) -> Result<()> {
        let conn = self.db.connection();
        conn.execute(
            "DELETE FROM app_settings WHERE key LIKE ?1",
            [&format!("prekey_%:{}", peer)],
        )
        .map_err(|e| ChattorError::Database(format!("PreKeyStore delete failed: {}", e)))?;
        conn.execute(
            "DELETE FROM app_settings WHERE key = ?1",
            [&format!("signal_identity_secret:{}", peer)],
        )
        .map_err(|e| ChattorError::Database(format!("PreKeyStore delete failed: {}", e)))?;
        Ok(())
    }

    /// Delete material older than `max_age_secs`; returns the count of peers cleaned.
    pub fn cleanup_stale(&self, max_age_secs: u64) -> Result<usize> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let conn = self.db.connection();
        let mut stmt = conn
            .prepare("SELECT key, value FROM app_settings WHERE key LIKE 'prekey_created_at:%'")
            .map_err(|e| ChattorError::Database(format!("cleanup query failed: {}", e)))?;
        let stale_peers: Vec<String> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| ChattorError::Database(format!("cleanup read failed: {}", e)))?
            .filter_map(|r| r.ok())
            .filter_map(|(key, ts_str)| {
                let ts: u64 = ts_str.parse().ok()?;
                if now.saturating_sub(ts) > max_age_secs {
                    key.strip_prefix("prekey_created_at:").map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        let count = stale_peers.len();
        for peer in &stale_peers {
            self.delete(peer).ok();
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn temp_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        (db, temp)
    }

    fn material(with_opk: bool) -> PreKeyPrivateMaterial {
        PreKeyPrivateMaterial {
            identity_secret: [1u8; 32],
            signed_prekey_secret: [2u8; 32],
            prekey_secret: if with_opk { Some([3u8; 32]) } else { None },
        }
    }

    #[test]
    fn test_store_and_load() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        store.store("peer.onion", &material(true), &[9u8; 32], 1000).unwrap();
        let loaded = store.load("peer.onion").unwrap().unwrap();
        assert_eq!(loaded.identity_secret, [1u8; 32]);
        assert_eq!(loaded.signed_prekey_secret, [2u8; 32]);
        assert_eq!(loaded.prekey_secret, Some([3u8; 32]));
        assert_eq!(store.load_signal_identity_secret("peer.onion").unwrap(), Some([9u8; 32]));
    }

    #[test]
    fn test_load_absent_returns_none() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        assert!(store.load("nobody.onion").unwrap().is_none());
        assert!(store.load_signal_identity_secret("nobody.onion").unwrap().is_none());
    }

    #[test]
    fn test_store_without_opk() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        store.store("peer.onion", &material(false), &[9u8; 32], 1000).unwrap();
        let loaded = store.load("peer.onion").unwrap().unwrap();
        assert_eq!(loaded.prekey_secret, None);
    }

    #[test]
    fn test_delete_is_idempotent() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        store.store("peer.onion", &material(true), &[9u8; 32], 1000).unwrap();
        store.delete("peer.onion").unwrap();
        store.delete("peer.onion").unwrap(); // idempotent
        assert!(store.load("peer.onion").unwrap().is_none());
        assert!(store.load_signal_identity_secret("peer.onion").unwrap().is_none());
    }

    #[test]
    fn test_cleanup_stale() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        // stale: created_at far in the past
        store.store("old.onion", &material(true), &[9u8; 32], 0).unwrap();
        // fresh: created_at ~now
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        store.store("new.onion", &material(true), &[8u8; 32], now).unwrap();
        let cleaned = store.cleanup_stale(60).unwrap();
        assert_eq!(cleaned, 1);
        assert!(store.load("old.onion").unwrap().is_none());
        assert!(store.load("new.onion").unwrap().is_some());
    }
}
```

- [ ] **Step 3: Build and test**

Run: `cargo test --lib crypto::prekey_store`
Expected: 5 tests pass.

- [ ] **Step 4: Clippy**

Run: `cargo clippy --lib -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(crypto): add typed PreKeyStore for X3DH establishment material"
```

---

### Task 2: `SessionManager` — orchestration facade (TDD, keystone test)

**Files:**
- Create: `src/crypto/session_manager.rs`
- Modify: `src/crypto/mod.rs`

- [ ] **Step 1: Wire the module**

In `src/crypto/mod.rs` add:
```rust
pub mod session_manager;
```
and re-export:
```rust
pub use session_manager::{OutgoingCrypto, SessionManager};
```

- [ ] **Step 2: Write `src/crypto/session_manager.rs`**

```rust
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
        // Two peers, two databases.
        let (alice_db, _a) = temp_db(); // initiator (sent the friend request)
        let (bob_db, _b) = temp_db(); // acceptor
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
        // material consumed:
        assert!(PreKeyStore::new(&bob_db).load(alice).unwrap().is_none());

        // 4. Bidirectional messaging now works (sessions established both sides).
        let pt = serde_json::to_vec(&PlaintextPayload {
            content: "hello bob".to_string(),
            sent_at: now_secs(),
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        })
        .unwrap();
        let oc = SessionManager::new(&alice_db).encrypt_for(bob, &pt).unwrap().unwrap();
        assert!(oc.x3dh_init.is_none()); // normal send carries no x3dh_init
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
            signal_type: SignalMessageType::Message, // NOT a prekey message
            timestamp: now_secs(),
            message_id: Uuid::new_v4(),
            x3dh_init: None,
        };
        let result = SessionManager::new(&db).decrypt_incoming(&msg).unwrap();
        assert!(result.is_none());
    }
}
```

- [ ] **Step 3: Build and run the facade tests**

Run: `cargo test --lib crypto::session_manager`
Expected: 3 tests pass — crucially `test_full_handshake_through_facade`.

- [ ] **Step 4: Clippy**

Run: `cargo clippy --lib -- -D warnings`
Expected: clean (trim any unused import).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(crypto): add SessionManager facade with keystone handshake test"
```

---

### Task 3: Migrate the inbound handler (`handlers/messaging.rs`)

**Files:**
- Modify: `src/handlers/messaging.rs`

The `Message::TextMessage(text_msg)` arm currently inlines ~190 lines (decode →
load-or-establish → decrypt → cleanup, producing `payload`). Replace the body that
computes `let payload = match store.load_session(...) { ... };` (the block from the
`let store = crypto::SessionStore::new(&app.db);` line through the closing of that
`match` that binds `payload`) with a single facade call, preserving the
no-session-drop behavior.

- [ ] **Step 1: Replace the inline crypto with the facade call**

Replace the inline block (decode + the `let payload = match store.load_session(...) {...};`)
with:
```rust
            let payload = match crate::crypto::SessionManager::new(&app.db)
                .decrypt_incoming(text_msg)?
            {
                Some(p) => p,
                None => {
                    eprintln!(
                        "No session for {} and not a PreKey message, cannot decrypt",
                        from_onion
                    );
                    return Ok(());
                }
            };
```
Keep everything after that unchanged: the `if payload.message_type == "handshake" { ... return Ok(()); }` early return, and the `find_friend_by_onion`/`get_or_create_conversation`/`store_incoming_message_with_ttl` writes. Remove the now-unused `is_prekey`/`store`/`header`/`ciphertext` locals and any now-unused imports (`crypto::SessionStore`, `base64`) — the compiler/clippy will flag them.

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: `Finished`, no errors.

- [ ] **Step 3: Run the full suite (behavior preservation)**

Run: `cargo test -- --test-threads=1`
Expected: lib green, daemon 32, e2e 6, integration 16 — all pass. (`--test-threads=1` avoids the pre-existing `app.rs` `$HOME`-race flake.)

- [ ] **Step 4: Clippy**

Run: `cargo clippy --lib -- -D warnings`
Expected: clean (remove any import left unused by the migration).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(handlers): route inbound decrypt through SessionManager"
```

---

### Task 4: Migrate the friend-request handlers (`handlers/friend_request.rs`)

**Files:**
- Modify: `src/handlers/friend_request.rs`

- [ ] **Step 1: `handle_accept_friend_request` — use `create_accept_bundle`**

Replace the bundle-generation + 5 KV-write block (from
`let signal_identity = libsignal_protocol::vxeddsa::gen_keypair();` through the
`prekey_created_at` `conn.execute(...)?;` block — i.e. the raw keypair gen, the
`PreKeyBundle::generate_real`, and all the `INSERT OR REPLACE INTO app_settings`
writes for prekey_identity/spk/opk/signal_identity_secret/prekey_created_at) with:
```rust
    let bundle = crate::crypto::SessionManager::new(&app.db).create_accept_bundle(&from_onion)?;
```
Keep the surrounding code: loading `from_onion`/`identity`, the `timestamp`/`signature`
(Ed25519 `identity.sign`), `bundle_json = serde_json::to_string(&bundle)`, the
`FriendRequestAcceptMessage` construction, the `friend_requests`/`friends`/subscription
DB writes, and the queue. Remove the now-unused `use crate::crypto::PreKeyBundle;` and
the `signal_identity*`/`private_keys`/`*_b64` locals (compiler-guided).

- [ ] **Step 2: `handle_incoming_accept` — use `establish_from_accept`**

Keep the bundle parse (`let bundle: PreKeyBundle = serde_json::from_str(...)`) and the
entire Ed25519 accept-signature TOFU verification block unchanged. Then replace
everything from the VXEdDSA `if !bundle.verify_signature()? { ... }` check through the
session establishment, `our_identity_encoded`, session store, and handshake encryption
(the block that produces `header`/`ciphertext`/`is_prekey`/`x3dh_init`) with:
```rust
    let own_onion = app
        .onion_address
        .as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized".into()))?;
    let hs = crate::crypto::SessionManager::new(&app.db)
        .establish_from_accept(&accept.from_onion, &bundle)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let handshake_msg = protocol::message::Message::TextMessage(protocol::message::TextMessage {
        from_onion: own_onion.clone(),
        to_onion: accept.from_onion.clone(),
        signal_header: base64::engine::general_purpose::STANDARD.encode(&hs.header),
        signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&hs.ciphertext),
        signal_type: if hs.is_prekey {
            protocol::message::SignalMessageType::PrekeyMessage
        } else {
            protocol::message::SignalMessageType::Message
        },
        timestamp: now,
        message_id: uuid::Uuid::new_v4(),
        x3dh_init: hs.x3dh_init,
    });
```
Keep whatever queueing/return code followed the original `handshake_msg` construction
(the `message_queue.enqueue(...)` for the handshake and the friend/DB updates). Remove
the now-unused `use crate::crypto::{PreKeyBundle, PreKeyPrivateMaterial, SessionStore, SignalSession};`
down to just `use crate::crypto::PreKeyBundle;` (still needed for the parse), and drop
the `signal_identity_secret`/`dummy_private`/`SessionStore`/raw-libsignal locals
(compiler-guided).

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors.

- [ ] **Step 4: Full suite**

Run: `cargo test -- --test-threads=1`
Expected: lib green, daemon 32, e2e 6, integration 16 — all pass.

- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected clean (trim unused imports in friend_request.rs).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(handlers): route friend-request X3DH through SessionManager"
```

---

### Task 5: Migrate the two outbound sites (`main.rs`, `daemon/rpc/messaging.rs`)

**Files:**
- Modify: `src/daemon/rpc/messaging.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Daemon outbound (`daemon/rpc/messaging.rs`)**

Replace the inline encrypt block — `let store = crate::crypto::SessionStore::new(&app.db);`
through the `match store.load_session(&peer_onion) { Ok(Some(mut session)) => {...} Ok(None) => {...} Err(e) => {...} }`
that builds `(msg, msg_id, pool)` — with a facade call that preserves the two error
messages:
```rust
        // Encrypt with Signal Protocol via the session facade
        let payload = crate::protocol::message::PlaintextPayload {
            content: message.clone(),
            sent_at: now,
            message_type: "text".to_string(),
            ephemeral_ttl: ttl,
        };
        let plaintext = match serde_json::to_vec(&payload) {
            Ok(p) => p,
            Err(e) => return RpcResponse::error(id, -32000, format!("Serialize error: {}", e)),
        };
        let oc = match crate::crypto::SessionManager::new(&app.db).encrypt_for(&peer_onion, &plaintext) {
            Ok(Some(oc)) => oc,
            Ok(None) => {
                return RpcResponse::error(
                    id,
                    -32000,
                    format!("No encryption session with {}", peer_onion),
                )
            }
            Err(e) => return RpcResponse::error(id, -32000, format!("Encrypt error: {}", e)),
        };

        use base64::Engine;
        let msg = crate::protocol::message::Message::TextMessage(crate::protocol::message::TextMessage {
            from_onion: own_onion,
            to_onion: peer_onion.clone(),
            signal_header: base64::engine::general_purpose::STANDARD.encode(&oc.header),
            signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&oc.ciphertext),
            signal_type: if oc.is_prekey {
                crate::protocol::message::SignalMessageType::PrekeyMessage
            } else {
                crate::protocol::message::SignalMessageType::Message
            },
            timestamp: now,
            message_id: msg_id,
            x3dh_init: None,
        });

        let pool = app.connection_pool.as_ref().map(Arc::clone);
        (msg, msg_id, pool)
```
(The `store_outgoing_message_with_ttl` call before this block and the send/queue code
after `// Lock released here` stay unchanged.)

- [ ] **Step 2: TUI outbound (`main.rs` ~908)**

Replace the `let encrypted_msg = { let store = crypto::SessionStore::new(&app_lock.db); ... };`
block (which yields `Option<TextMessage>`) with a facade-based version that preserves
the "drop on no session / error" behavior (`None`):
```rust
                                    // Encrypt the message using the session facade
                                    let encrypted_msg = {
                                        let payload = protocol::message::PlaintextPayload {
                                            content: content.clone(),
                                            sent_at: std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_secs()
                                                as i64,
                                            message_type: "text".to_string(),
                                            ephemeral_ttl: conv_ttl,
                                        };
                                        match serde_json::to_vec(&payload).ok().and_then(|pt| {
                                            crypto::SessionManager::new(&app_lock.db)
                                                .encrypt_for(&peer_onion, &pt)
                                                .ok()
                                                .flatten()
                                        }) {
                                            Some(oc) => Some(protocol::message::TextMessage {
                                                from_onion: own_onion.clone(),
                                                to_onion: peer_onion.clone(),
                                                signal_header: base64::engine::general_purpose::STANDARD.encode(&oc.header),
                                                signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&oc.ciphertext),
                                                signal_type: if oc.is_prekey {
                                                    protocol::message::SignalMessageType::PrekeyMessage
                                                } else {
                                                    protocol::message::SignalMessageType::Message
                                                },
                                                timestamp: payload.sent_at,
                                                message_id: uuid::Uuid::parse_str(&msg_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                                                x3dh_init: None,
                                            }),
                                            None => None,
                                        }
                                    };
```
(Everything after — `if let Some(text_msg) = encrypted_msg { ... try_send_direct ... }` —
stays unchanged.)

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors.

- [ ] **Step 4: Full suite**

Run: `cargo test -- --test-threads=1`
Expected: lib green, daemon 32, e2e 6, integration 16 — all pass.

- [ ] **Step 5: Clippy** — `cargo clippy --lib -- -D warnings`. Expected clean (trim unused `SessionStore`/`base64` imports if any).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: route TUI + daemon outbound encryption through SessionManager"
```

---

### Task 6: Delegate cleanup, delete the dead facade, final gate

**Files:**
- Modify: `src/db/queries/settings.rs`
- Delete: `src/net/sender.rs`, `src/net/receiver.rs`
- Modify: `src/net/mod.rs`

- [ ] **Step 1: Delegate `cleanup_stale_prekey_material` to `PreKeyStore`**

In `src/db/queries/settings.rs`, replace the entire body of
`cleanup_stale_prekey_material` with a delegation (keep the exact public signature so
its two callers — `main.rs:445`, `daemon/tasks.rs:93` — are unchanged):
```rust
pub fn cleanup_stale_prekey_material(db: &Database, max_age_secs: u64) -> Result<usize> {
    crate::crypto::PreKeyStore::new(db).cleanup_stale(max_age_secs)
}
```
Remove any imports in `settings.rs` left unused by this change (compiler/clippy-guided).

- [ ] **Step 2: Delete the dead `net` scaffolding**

Run:
```bash
git rm src/net/sender.rs src/net/receiver.rs
```
In `src/net/mod.rs`, delete the two lines:
```rust
pub mod receiver;
pub mod sender;
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: `Finished`, no errors. (Confirmed earlier: nothing outside those two files imports `MessageSender`/`MessageReceiver`.)

- [ ] **Step 4: Full verification gate**

Run: `cargo test -- --test-threads=1`
Expected: all green. Lib count = baseline 361 − 6 (deleted net tests: sender 2 + receiver 4) + 8 new (PreKeyStore 5 + SessionManager 3) = **363**. Daemon 32, e2e 6, integration 16 unchanged. If the lib total differs, reconcile against this arithmetic before proceeding.
Run: `cargo clippy --lib -- -D warnings` → clean.
Run: `cargo fmt` then `cargo fmt --check` → clean.

- [ ] **Step 5: Confirm the boundary holds**

Run:
```bash
rg -n 'SignalSession|from_prekey|vxeddsa|PreKeyPrivateMaterial' src/handlers src/main.rs src/daemon/rpc/messaging.rs || echo "no raw Signal orchestration left in callers"
```
Expected: prints `no raw Signal orchestration left in callers` (the only crypto reference left in handlers should be `crate::crypto::SessionManager` / `PreKeyBundle` parse in friend_request).
Run:
```bash
rg -n "prekey_identity:|prekey_spk:|prekey_opk:|signal_identity_secret:|prekey_created_at:" src --type rust | rg -v 'src/crypto/prekey_store.rs'
```
Expected: no matches outside `prekey_store.rs` (the KV key format now lives in exactly one place).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(crypto): delegate prekey cleanup to PreKeyStore; remove dead net facade"
```

---

## Final state

```
src/crypto/
  prekey_store.rs     // PreKeyStore — owns the establishment-material KV (5 tests)
  session_manager.rs  // SessionManager facade: encrypt_for/decrypt_incoming/
                      //   create_accept_bundle/establish_from_accept (3 tests incl. keystone)
  signal.rs, session_store.rs, identity.rs   // unchanged
// removed: src/net/sender.rs, src/net/receiver.rs
```

All four production sites call `SessionManager`; the prekey KV format lives only in
`PreKeyStore`; the dead `net` facade is gone. Wire format and crypto behavior unchanged
(guarded by the keystone facade test + the existing e2e/integration suite).
