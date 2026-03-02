//! End-to-end tests for the complete friend-request -> X3DH session -> messaging pipeline.
//!
//! These tests exercise the full Signal Protocol flow **offline** (no Tor required)
//! using public APIs: IdentityKeypair, PreKeyBundle, SignalSession, SessionStore,
//! Database, MessageQueue, and protocol message types.
//!
//! Run with: cargo test --test e2e_messaging -- --nocapture

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chattor::crypto::{IdentityKeypair, PreKeyBundle, PreKeyPrivateMaterial, SessionStore, SignalSession};
use chattor::db::queries::{get_app_setting, set_app_setting};
use chattor::db::Database;
use chattor::net::queue::MessageQueue;
use chattor::protocol::friend_request::FriendRequestHandler;
use chattor::protocol::message::*;
use tempfile::NamedTempFile;
use uuid::Uuid;

// --- Helpers ---------------------------------------------------------------

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn temp_db() -> (NamedTempFile, Database) {
    let tmp = NamedTempFile::new().unwrap();
    let db = Database::open(tmp.path()).unwrap();
    (tmp, db)
}

/// Generate an X25519 Signal identity keypair for tests.
fn gen_signal_identity() -> ([u8; 32], [u8; 32]) {
    let kp = libsignal_protocol::vxeddsa::gen_keypair();
    let raw_pub = libsignal_protocol::utils::decode_public_key(&kp.public).unwrap();
    (kp.secret, raw_pub)
}

/// Create a PlaintextPayload, serialize to JSON bytes (the canonical pre-encryption format).
fn make_payload(content: &str, message_type: &str) -> Vec<u8> {
    let payload = PlaintextPayload {
        content: content.to_string(),
        sent_at: now_ts(),
        message_type: message_type.to_string(),
        ephemeral_ttl: None,
    };
    serde_json::to_vec(&payload).unwrap()
}

/// Wrap header + ciphertext + is_prekey into a wire-format TextMessage.
fn wrap_text_message(
    from: &str,
    to: &str,
    header: &[u8],
    ciphertext: &[u8],
    is_prekey: bool,
) -> Message {
    Message::TextMessage(TextMessage {
        from_onion: from.to_string(),
        to_onion: to.to_string(),
        signal_header: STANDARD.encode(header),
        signal_ciphertext: STANDARD.encode(ciphertext),
        signal_type: if is_prekey {
            SignalMessageType::PrekeyMessage
        } else {
            SignalMessageType::Message
        },
        timestamp: now_ts(),
        message_id: Uuid::new_v4(),
        x3dh_init: None,
    })
}

/// Set up a full Alice-Bob session pair using real X3DH + Double Ratchet.
/// Returns (alice_session, bob_session).
fn setup_session_pair() -> (SignalSession, SignalSession) {
    let (alice_signal_secret, _) = gen_signal_identity();
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

// --- Test 1: Full friend-request -> X3DH session -> bidirectional messaging ---

#[test]
fn test_full_friend_request_to_messaging_pipeline() {
    // 1. Alice and Bob generate Signal identities
    let (alice_signal_secret, _alice_signal_public) = gen_signal_identity();
    let alice_onion = "alice1234567890.onion";
    let bob_onion = "bob1234567890.onion";

    // 2. Bob generates PreKeyBundle (simulates friend-request accept)
    let (bob_signal_secret, bob_signal_public) = gen_signal_identity();
    let (bob_bundle, bob_private) =
        PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

    // 3. Bob stores PreKeyPrivateMaterial in app_settings (as the real flow does)
    let (_tmp_bob, bob_db) = temp_db();
    set_app_setting(
        &bob_db,
        &format!("prekey_identity:{}", alice_onion),
        &STANDARD.encode(bob_private.identity_secret),
    )
    .unwrap();
    set_app_setting(
        &bob_db,
        &format!("prekey_spk:{}", alice_onion),
        &STANDARD.encode(bob_private.signed_prekey_secret),
    )
    .unwrap();
    if let Some(opk) = bob_private.prekey_secret {
        set_app_setting(
            &bob_db,
            &format!("prekey_opk:{}", alice_onion),
            &STANDARD.encode(opk),
        )
        .unwrap();
    }

    // 4. Alice receives accept, creates session via from_prekey_bundle_real
    let (mut alice_session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
        bob_onion.to_string(),
        &bob_bundle,
        &bob_private, // unused but required by API
        &alice_signal_secret,
    )
    .unwrap();

    // 5. Alice encrypts first message (PreKey message)
    let alice_plaintext = make_payload("Hello Bob, this is Alice!", "text");
    let (alice_header, alice_ciphertext, is_prekey) =
        alice_session.encrypt(&alice_plaintext).unwrap();
    assert!(is_prekey, "First message from initiator must be a PreKey message");

    // 6. Bob loads stored private material and creates session from X3DH init data
    let loaded_identity: [u8; 32] = STANDARD
        .decode(
            get_app_setting(&bob_db, &format!("prekey_identity:{}", alice_onion))
                .unwrap()
                .unwrap(),
        )
        .unwrap()
        .try_into()
        .unwrap();
    let loaded_spk: [u8; 32] = STANDARD
        .decode(
            get_app_setting(&bob_db, &format!("prekey_spk:{}", alice_onion))
                .unwrap()
                .unwrap(),
        )
        .unwrap()
        .try_into()
        .unwrap();
    let loaded_opk: Option<[u8; 32]> =
        get_app_setting(&bob_db, &format!("prekey_opk:{}", alice_onion))
            .unwrap()
            .map(|v| STANDARD.decode(v).unwrap().try_into().unwrap());

    let reconstructed_private = PreKeyPrivateMaterial {
        identity_secret: loaded_identity,
        signed_prekey_secret: loaded_spk,
        prekey_secret: loaded_opk,
    };

    // Bob needs Alice's identity and ephemeral public keys (sent via X3DH init data)
    let alice_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&alice_signal_secret);
    let (mut bob_session, _bob_ad) = SignalSession::from_prekey_message_real(
        alice_onion.to_string(),
        &reconstructed_private,
        &alice_identity_encoded,
        &ephemeral_public,
    )
    .unwrap();

    // 7. Bob decrypts Alice's message
    let bob_decrypted = bob_session
        .decrypt(&alice_header, &alice_ciphertext)
        .unwrap();
    let alice_payload: PlaintextPayload = serde_json::from_slice(&bob_decrypted).unwrap();
    assert_eq!(alice_payload.content, "Hello Bob, this is Alice!");

    // 8. Bob encrypts a reply (NOT a PreKey message)
    let bob_plaintext = make_payload("Hey Alice, got your message!", "text");
    let (bob_header, bob_ciphertext, bob_is_prekey) =
        bob_session.encrypt(&bob_plaintext).unwrap();
    assert!(
        !bob_is_prekey,
        "Reply from recipient must NOT be a PreKey message"
    );

    // 9. Alice decrypts Bob's reply
    let alice_decrypted = alice_session
        .decrypt(&bob_header, &bob_ciphertext)
        .unwrap();
    let bob_payload: PlaintextPayload = serde_json::from_slice(&alice_decrypted).unwrap();
    assert_eq!(bob_payload.content, "Hey Alice, got your message!");
}

// --- Test 2: Handshake enables acceptor to send first ----------------------

#[test]
fn test_handshake_enables_acceptor_to_send_first() {
    // Alice (acceptor) generates bundle; Bob (requester) will initiate session
    let (alice_signal_secret, alice_signal_public) = gen_signal_identity();
    let (alice_bundle, alice_private) =
        PreKeyBundle::generate_real(&alice_signal_secret, &alice_signal_public).unwrap();

    // Bob generates a Signal identity for X3DH
    let (bob_signal_secret, _bob_signal_public) = gen_signal_identity();

    // 1. Bob creates session from Alice's bundle
    let (mut bob_session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
        "alice.onion".into(),
        &alice_bundle,
        &alice_private, // unused but required by API
        &bob_signal_secret,
    )
    .unwrap();

    // 2. Bob encrypts a handshake message
    let handshake_payload = make_payload("", "handshake");
    let (hs_header, hs_ciphertext, is_prekey) =
        bob_session.encrypt(&handshake_payload).unwrap();
    assert!(is_prekey, "Handshake must be a PreKey message");

    // 3. Alice receives handshake, creates session from the PreKey message
    let bob_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&bob_signal_secret);
    let (mut alice_session, _alice_ad) = SignalSession::from_prekey_message_real(
        "bob.onion".into(),
        &alice_private,
        &bob_identity_encoded,
        &ephemeral_public,
    )
    .unwrap();

    // 4. Alice decrypts handshake to verify it works
    let handshake_decrypted = alice_session
        .decrypt(&hs_header, &hs_ciphertext)
        .unwrap();
    let handshake: PlaintextPayload = serde_json::from_slice(&handshake_decrypted).unwrap();
    assert_eq!(handshake.message_type, "handshake");

    // 5. Alice sends a REAL message first (the acceptor can now speak)
    let alice_plaintext = make_payload("Hi Bob, I accepted your request!", "text");
    let (alice_header, alice_ct, alice_is_prekey) =
        alice_session.encrypt(&alice_plaintext).unwrap();
    assert!(
        !alice_is_prekey,
        "Alice's reply should NOT be PreKey (she has no ephemeral)"
    );

    // 6. Bob decrypts Alice's message
    let bob_decrypted = bob_session.decrypt(&alice_header, &alice_ct).unwrap();
    let alice_msg: PlaintextPayload = serde_json::from_slice(&bob_decrypted).unwrap();
    assert_eq!(alice_msg.content, "Hi Bob, I accepted your request!");
    assert_eq!(alice_msg.message_type, "text");

    // 7. Bob sends a follow-up
    let bob_plaintext = make_payload("Great, we're connected!", "text");
    let (bob_header, bob_ct, bob_is_prekey) = bob_session.encrypt(&bob_plaintext).unwrap();
    assert!(!bob_is_prekey);

    let alice_decrypted = alice_session.decrypt(&bob_header, &bob_ct).unwrap();
    let bob_msg: PlaintextPayload = serde_json::from_slice(&alice_decrypted).unwrap();
    assert_eq!(bob_msg.content, "Great, we're connected!");
}

// --- Test 3: Session persistence across multiple messages ------------------

#[test]
fn test_session_persistence_across_multiple_messages() {
    let (mut alice_session, mut bob_session) = setup_session_pair();

    // Alice sends 3 messages
    let msgs_to_send = ["Message 1", "Message 2", "Message 3"];
    let mut headers = Vec::new();
    let mut ciphertexts = Vec::new();
    let mut prekey_flags = Vec::new();
    for msg in &msgs_to_send {
        let payload = make_payload(msg, "text");
        let (h, ct, is_pk) = alice_session.encrypt(&payload).unwrap();
        headers.push(h);
        ciphertexts.push(ct);
        prekey_flags.push(is_pk);
    }
    assert!(prekey_flags[0], "First message should be PreKey");
    assert!(!prekey_flags[1], "Second message should NOT be PreKey");
    assert!(!prekey_flags[2], "Third message should NOT be PreKey");

    // Bob decrypts all 3
    for (i, ((h, ct), _is_pk)) in headers
        .iter()
        .zip(ciphertexts.iter())
        .zip(prekey_flags.iter())
        .enumerate()
    {
        let decrypted = bob_session.decrypt(h, ct).unwrap();
        let payload: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(payload.content, msgs_to_send[i]);
    }

    // Serialize both sessions to DB, then reload
    let (_tmp, db) = temp_db();
    let store = SessionStore::new(&db);

    store.store_session(&alice_session).unwrap();
    store.store_session(&bob_session).unwrap();

    let mut alice_restored = store.load_session("bob.onion").unwrap().unwrap();
    let mut bob_restored = store.load_session("alice.onion").unwrap().unwrap();

    // Bob sends 2 messages to Alice via restored sessions
    let bob_msgs = ["Reply 1 from Bob", "Reply 2 from Bob"];
    for msg in &bob_msgs {
        let payload = make_payload(msg, "text");
        let (h, ct, is_pk) = bob_restored.encrypt(&payload).unwrap();
        assert!(!is_pk, "Bob's messages should never be PreKey");

        let decrypted = alice_restored.decrypt(&h, &ct).unwrap();
        let p: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(p.content, *msg);
    }

    // Verify all ciphertexts are unique (different nonces -> different output)
    for i in 0..ciphertexts.len() {
        for j in (i + 1)..ciphertexts.len() {
            assert_ne!(
                ciphertexts[i], ciphertexts[j],
                "Ciphertexts must be unique (nonce reuse!)"
            );
        }
    }
}

// --- Test 4: PreKey material storage and cleanup ---------------------------

#[test]
fn test_prekey_material_storage_and_cleanup() {
    let peer_onion = "peer1234567890.onion";

    // 1. Generate bundle + private material
    let (signal_secret, signal_public) = gen_signal_identity();
    let (bundle, private_material) =
        PreKeyBundle::generate_real(&signal_secret, &signal_public).unwrap();

    // 2. Store identity_secret in app_settings
    let (_tmp, db) = temp_db();
    let key = format!("prekey_identity:{}", peer_onion);
    let stored_value = STANDARD.encode(private_material.identity_secret);
    set_app_setting(&db, &key, &stored_value).unwrap();

    // Also store SPK and OPK
    set_app_setting(
        &db,
        &format!("prekey_spk:{}", peer_onion),
        &STANDARD.encode(private_material.signed_prekey_secret),
    )
    .unwrap();
    if let Some(opk) = private_material.prekey_secret {
        set_app_setting(
            &db,
            &format!("prekey_opk:{}", peer_onion),
            &STANDARD.encode(opk),
        )
        .unwrap();
    }

    // 3. Verify it loads back and matches
    let loaded = get_app_setting(&db, &key).unwrap().unwrap();
    assert_eq!(loaded, stored_value);

    let decoded = STANDARD.decode(&loaded).unwrap();
    assert_eq!(decoded.len(), 32);
    assert_eq!(&decoded[..], &private_material.identity_secret[..]);

    // 4. Create a session from the stored material
    //    First, simulate Alice sending a PreKey message
    let (alice_signal_secret, _alice_signal_public) = gen_signal_identity();
    let (mut alice_session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
        peer_onion.to_string(),
        &bundle,
        &private_material, // unused
        &alice_signal_secret,
    )
    .unwrap();
    let (header, ciphertext, _) = alice_session.encrypt(b"test message").unwrap();

    // Reconstruct private material from stored bytes
    let mut identity_secret = [0u8; 32];
    identity_secret.copy_from_slice(&decoded);
    let reconstructed = PreKeyPrivateMaterial {
        identity_secret,
        signed_prekey_secret: private_material.signed_prekey_secret,
        prekey_secret: private_material.prekey_secret,
    };

    let alice_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&alice_signal_secret);
    let (mut session, _ad) = SignalSession::from_prekey_message_real(
        "alice.onion".into(),
        &reconstructed,
        &alice_identity_encoded,
        &ephemeral_public,
    )
    .unwrap();

    let decrypted = session.decrypt(&header, &ciphertext).unwrap();
    assert_eq!(&decrypted[..], b"test message");

    // 5. Delete the stored material
    db.connection()
        .execute("DELETE FROM app_settings WHERE key = ?1", [&key])
        .unwrap();

    // 6. Verify it's gone
    let after_delete = get_app_setting(&db, &key).unwrap();
    assert!(after_delete.is_none(), "PreKey material should be deleted");
}

// --- Test 5: Encrypted messages survive queue serialization ----------------

#[test]
fn test_message_queue_with_encrypted_messages() {
    let (mut alice_session, mut bob_session) = setup_session_pair();

    // 1. Alice encrypts a message
    let plaintext = make_payload("Secret message through the queue", "text");
    let (header, ciphertext, is_prekey) = alice_session.encrypt(&plaintext).unwrap();

    // 2. Wrap in TextMessage and enqueue
    let wire_msg = wrap_text_message("alice.onion", "bob.onion", &header, &ciphertext, is_prekey);
    let (_tmp, db) = temp_db();
    let queue = MessageQueue::new();
    let queue_id = queue.enqueue(&db, "bob.onion", &wire_msg, "normal").unwrap();
    assert!(queue_id > 0);

    // 3. Retrieve from queue
    let pending = queue.get_pending_messages(&db, i64::MAX).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].peer_onion, "bob.onion");

    // 4. Extract header and ciphertext from the dequeued message
    let dequeued = &pending[0].message;
    let (recovered_header_b64, recovered_ct_b64, recovered_signal_type) = match dequeued {
        Message::TextMessage(tm) => (&tm.signal_header, &tm.signal_ciphertext, &tm.signal_type),
        other => panic!("Expected TextMessage, got {:?}", other),
    };

    let recovered_header = STANDARD.decode(recovered_header_b64).unwrap();
    let recovered_ct = STANDARD.decode(recovered_ct_b64).unwrap();
    let is_prekey_from_wire = matches!(recovered_signal_type, SignalMessageType::PrekeyMessage);
    assert_eq!(is_prekey_from_wire, is_prekey);
    assert_eq!(recovered_ct, ciphertext, "Ciphertext must survive queue round-trip");
    assert_eq!(recovered_header, header, "Header must survive queue round-trip");

    // 5. Bob decrypts
    let decrypted = bob_session
        .decrypt(&recovered_header, &recovered_ct)
        .unwrap();
    let payload: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
    assert_eq!(payload.content, "Secret message through the queue");

    // 6. Mark delivered and verify queue is empty
    queue.mark_delivered(&db, queue_id).unwrap();
    let remaining = queue.get_pending_messages(&db, i64::MAX).unwrap();
    assert!(remaining.is_empty(), "Queue should be empty after delivery");
}

// --- Test 6: Friend request signature creation and verification ------------

#[test]
fn test_friend_request_signature_verification() {
    let alice_identity = IdentityKeypair::generate().unwrap();
    let alice_onion = "alice-test.onion"; // Any onion works — TOFU verifies against included pubkey
    let friend_code = "happy-1234-tiger-5678";

    // 1. Alice creates a signed friend request
    let request =
        FriendRequestHandler::create_request(&alice_identity, alice_onion, friend_code).unwrap();

    assert_eq!(request.from_onion, alice_onion);
    assert_eq!(request.from_friendcode, friend_code);
    assert!(!request.signature.is_empty());

    // 2. Verify signature passes validation (now a static method)
    assert!(
        FriendRequestHandler::validate_request(&request).unwrap(),
        "Valid signature should pass verification"
    );

    // 3. Tamper with the message (change from_onion)
    let mut tampered = request.clone();
    tampered.from_onion = "eve-evil.onion".to_string();

    assert!(
        !FriendRequestHandler::validate_request(&tampered).unwrap(),
        "Tampered from_onion should fail verification"
    );

    // 4. Tamper with the friend code
    let mut tampered_code = request.clone();
    tampered_code.from_friendcode = "evil-9999-hacker-0000".to_string();
    assert!(
        !FriendRequestHandler::validate_request(&tampered_code).unwrap(),
        "Tampered friend code should fail verification"
    );

    // 5. Tamper with the timestamp
    let mut tampered_ts = request.clone();
    tampered_ts.timestamp += 1;
    assert!(
        !FriendRequestHandler::validate_request(&tampered_ts).unwrap(),
        "Tampered timestamp should fail verification"
    );

    // 6. Tamper with the signature directly
    let mut tampered_sig = request.clone();
    let mut sig_bytes = STANDARD.decode(&tampered_sig.signature).unwrap();
    sig_bytes[0] ^= 0xFF; // flip bits
    tampered_sig.signature = STANDARD.encode(&sig_bytes);
    assert!(
        !FriendRequestHandler::validate_request(&tampered_sig).unwrap(),
        "Corrupted signature should fail verification"
    );
}
