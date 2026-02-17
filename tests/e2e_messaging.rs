//! End-to-end tests for the complete friend-request → X3DH session → messaging pipeline.
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
use chattor::net::MessageQueue;
use chattor::protocol::friend_request::FriendRequestHandler;
use chattor::protocol::message::*;
use tempfile::NamedTempFile;
use uuid::Uuid;

// ─── Helpers ──────────────────────────────────────────────────────────────────

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

/// Wrap ciphertext + is_prekey into a wire-format TextMessage.
fn wrap_text_message(
    from: &str,
    to: &str,
    ciphertext: &[u8],
    is_prekey: bool,
) -> Message {
    Message::TextMessage(TextMessage {
        from_onion: from.to_string(),
        to_onion: to.to_string(),
        signal_ciphertext: STANDARD.encode(ciphertext),
        signal_type: if is_prekey {
            SignalMessageType::PrekeyMessage
        } else {
            SignalMessageType::Message
        },
        timestamp: now_ts(),
        message_id: Uuid::new_v4(),
    })
}

// ─── Test 1: Full friend-request → X3DH session → bidirectional messaging ────

#[test]
fn test_full_friend_request_to_messaging_pipeline() {
    // 1. Alice and Bob generate identities
    let alice_identity = IdentityKeypair::generate().unwrap();
    let bob_identity = IdentityKeypair::generate().unwrap();
    let alice_onion = "alice1234567890.onion";
    let bob_onion = "bob1234567890.onion";

    // 2. Bob generates PreKeyBundle (simulates friend-request accept)
    let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_identity).unwrap();

    // 3. Bob stores PreKeyPrivateMaterial in app_settings (as the real flow does)
    let (_tmp_bob, bob_db) = temp_db();
    let private_key = STANDARD.encode(bob_private.identity_secret);
    let spk_key = STANDARD.encode(bob_private.signed_prekey_secret);
    let opk_key = bob_private.prekey_secret.map(|k| STANDARD.encode(k));
    set_app_setting(&bob_db, &format!("prekey_identity:{}", alice_onion), &private_key).unwrap();
    set_app_setting(&bob_db, &format!("prekey_spk:{}", alice_onion), &spk_key).unwrap();
    if let Some(ref opk) = opk_key {
        set_app_setting(&bob_db, &format!("prekey_opk:{}", alice_onion), opk).unwrap();
    }

    // 4. Alice receives accept, creates session via from_prekey_bundle_real
    //    Note: _remote_private and _local_identity are unused in the simplified X3DH,
    //    but we pass them for API fidelity.
    let mut alice_session = SignalSession::from_prekey_bundle_real(
        bob_onion.to_string(),
        &bob_bundle,
        &bob_private, // unused by simplified X3DH, but passed for API shape
        &alice_identity,
    )
    .unwrap();

    // 5. Alice encrypts first message (PreKey message with ephemeral)
    let alice_plaintext = make_payload("Hello Bob, this is Alice!", "text");
    let (alice_ciphertext, is_prekey) = alice_session.encrypt(&alice_plaintext).unwrap();
    assert!(is_prekey, "First message from initiator must be a PreKey message");

    // 6. Bob loads stored private material and creates session from PreKey message
    let loaded_identity = STANDARD.decode(
        get_app_setting(&bob_db, &format!("prekey_identity:{}", alice_onion))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let loaded_spk = STANDARD.decode(
        get_app_setting(&bob_db, &format!("prekey_spk:{}", alice_onion))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let loaded_opk = get_app_setting(&bob_db, &format!("prekey_opk:{}", alice_onion))
        .unwrap()
        .map(|v| {
            let bytes = STANDARD.decode(v).unwrap();
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        });

    let reconstructed_private = PreKeyPrivateMaterial {
        identity_secret: {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&loaded_identity);
            arr
        },
        signed_prekey_secret: {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&loaded_spk);
            arr
        },
        prekey_secret: loaded_opk,
    };

    let mut bob_session = SignalSession::from_prekey_message_real(
        alice_onion.to_string(),
        &alice_ciphertext,
        &bob_bundle,
        &reconstructed_private,
        &bob_identity,
    )
    .unwrap();

    // 7. Bob decrypts Alice's message
    let bob_decrypted = bob_session.decrypt(&alice_ciphertext, true).unwrap();
    let alice_payload: PlaintextPayload = serde_json::from_slice(&bob_decrypted).unwrap();
    assert_eq!(alice_payload.content, "Hello Bob, this is Alice!");

    // 8. Bob encrypts a reply (NOT a PreKey message — no ephemeral)
    let bob_plaintext = make_payload("Hey Alice, got your message!", "text");
    let (bob_ciphertext, bob_is_prekey) = bob_session.encrypt(&bob_plaintext).unwrap();
    assert!(
        !bob_is_prekey,
        "Reply from recipient must NOT be a PreKey message"
    );

    // 9. Alice decrypts Bob's reply
    let alice_decrypted = alice_session.decrypt(&bob_ciphertext, false).unwrap();
    let bob_payload: PlaintextPayload = serde_json::from_slice(&alice_decrypted).unwrap();
    assert_eq!(bob_payload.content, "Hey Alice, got your message!");
}

// ─── Test 2: Handshake enables acceptor to send first ─────────────────────────

#[test]
fn test_handshake_enables_acceptor_to_send_first() {
    let alice_identity = IdentityKeypair::generate().unwrap();
    let bob_identity = IdentityKeypair::generate().unwrap();

    // Alice (acceptor) generates bundle; Bob (requester) will initiate session
    let (alice_bundle, alice_private) = PreKeyBundle::generate_real(&alice_identity).unwrap();

    // 1. Bob creates session from Alice's bundle
    let mut bob_session = SignalSession::from_prekey_bundle_real(
        "alice.onion".into(),
        &alice_bundle,
        &alice_private,
        &bob_identity,
    )
    .unwrap();

    // 2. Bob encrypts a handshake message (special message_type for filtering)
    let handshake_payload = make_payload("", "handshake");
    let (handshake_ct, is_prekey) = bob_session.encrypt(&handshake_payload).unwrap();
    assert!(is_prekey, "Handshake must be a PreKey message");

    // 3. Alice receives handshake, creates session from the PreKey message
    let mut alice_session = SignalSession::from_prekey_message_real(
        "bob.onion".into(),
        &handshake_ct,
        &alice_bundle,
        &alice_private,
        &alice_identity,
    )
    .unwrap();

    // 4. Alice decrypts handshake to verify it works
    let handshake_decrypted = alice_session.decrypt(&handshake_ct, true).unwrap();
    let handshake: PlaintextPayload = serde_json::from_slice(&handshake_decrypted).unwrap();
    assert_eq!(handshake.message_type, "handshake");

    // 5. Alice sends a REAL message first (the acceptor can now speak)
    let alice_plaintext = make_payload("Hi Bob, I accepted your request!", "text");
    let (alice_ct, alice_is_prekey) = alice_session.encrypt(&alice_plaintext).unwrap();
    assert!(
        !alice_is_prekey,
        "Alice's reply should NOT be PreKey (she has no ephemeral)"
    );

    // 6. Bob decrypts Alice's message
    let bob_decrypted = bob_session.decrypt(&alice_ct, false).unwrap();
    let alice_msg: PlaintextPayload = serde_json::from_slice(&bob_decrypted).unwrap();
    assert_eq!(alice_msg.content, "Hi Bob, I accepted your request!");
    assert_eq!(alice_msg.message_type, "text");

    // 7. Bob sends a follow-up
    let bob_plaintext = make_payload("Great, we're connected!", "text");
    let (bob_ct, bob_is_prekey) = bob_session.encrypt(&bob_plaintext).unwrap();
    assert!(!bob_is_prekey);

    let alice_decrypted = alice_session.decrypt(&bob_ct, false).unwrap();
    let bob_msg: PlaintextPayload = serde_json::from_slice(&alice_decrypted).unwrap();
    assert_eq!(bob_msg.content, "Great, we're connected!");
}

// ─── Test 3: Session persistence across multiple messages ─────────────────────

#[test]
fn test_session_persistence_across_multiple_messages() {
    let alice_identity = IdentityKeypair::generate().unwrap();
    let bob_identity = IdentityKeypair::generate().unwrap();

    let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_identity).unwrap();

    // Establish sessions
    let mut alice_session = SignalSession::from_prekey_bundle_real(
        "bob.onion".into(),
        &bob_bundle,
        &bob_private,
        &alice_identity,
    )
    .unwrap();

    // Alice sends 3 messages
    let msgs_to_send = ["Message 1", "Message 2", "Message 3"];
    let mut ciphertexts = Vec::new();
    let mut prekey_flags = Vec::new();
    for msg in &msgs_to_send {
        let payload = make_payload(msg, "text");
        let (ct, is_pk) = alice_session.encrypt(&payload).unwrap();
        ciphertexts.push(ct);
        prekey_flags.push(is_pk);
    }
    assert!(prekey_flags[0], "First message should be PreKey");
    assert!(!prekey_flags[1], "Second message should NOT be PreKey");
    assert!(!prekey_flags[2], "Third message should NOT be PreKey");

    // Bob creates session from first PreKey message and decrypts all 3
    let mut bob_session = SignalSession::from_prekey_message_real(
        "alice.onion".into(),
        &ciphertexts[0],
        &bob_bundle,
        &bob_private,
        &bob_identity,
    )
    .unwrap();

    for (i, (ct, is_pk)) in ciphertexts.iter().zip(prekey_flags.iter()).enumerate() {
        let decrypted = bob_session.decrypt(ct, *is_pk).unwrap();
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
        let (ct, is_pk) = bob_restored.encrypt(&payload).unwrap();
        assert!(!is_pk, "Bob's messages should never be PreKey");

        let decrypted = alice_restored.decrypt(&ct, false).unwrap();
        let p: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(p.content, *msg);
    }

    // Verify all ciphertexts are unique (different nonces → different output)
    let all_cts: Vec<&[u8]> = ciphertexts.iter().map(|c| c.as_slice()).collect();
    for i in 0..all_cts.len() {
        for j in (i + 1)..all_cts.len() {
            assert_ne!(all_cts[i], all_cts[j], "Ciphertexts must be unique (nonce reuse!)");
        }
    }
}

// ─── Test 4: PreKey material storage and cleanup ──────────────────────────────

#[test]
fn test_prekey_material_storage_and_cleanup() {
    let identity = IdentityKeypair::generate().unwrap();
    let peer_onion = "peer1234567890.onion";

    // 1. Generate bundle + private material
    let (bundle, private_material) = PreKeyBundle::generate_real(&identity).unwrap();

    // 2. Store identity_secret in app_settings
    let (_tmp, db) = temp_db();
    let key = format!("prekey_private:{}", peer_onion);
    let stored_value = STANDARD.encode(private_material.identity_secret);
    set_app_setting(&db, &key, &stored_value).unwrap();

    // 3. Verify it loads back and matches
    let loaded = get_app_setting(&db, &key).unwrap().unwrap();
    assert_eq!(loaded, stored_value);

    let decoded = STANDARD.decode(&loaded).unwrap();
    assert_eq!(decoded.len(), 32);
    assert_eq!(&decoded[..], &private_material.identity_secret[..]);

    // 4. Create a session from the stored material
    //    First, simulate Alice sending a PreKey message
    let alice_identity = IdentityKeypair::generate().unwrap();
    let mut alice_session = SignalSession::from_prekey_bundle_real(
        peer_onion.to_string(),
        &bundle,
        &private_material,
        &alice_identity,
    )
    .unwrap();
    let (ciphertext, _) = alice_session.encrypt(b"test message").unwrap();

    // Reconstruct private material from stored bytes
    let mut identity_secret = [0u8; 32];
    identity_secret.copy_from_slice(&decoded);
    let reconstructed = PreKeyPrivateMaterial {
        identity_secret,
        signed_prekey_secret: private_material.signed_prekey_secret,
        prekey_secret: private_material.prekey_secret,
    };

    let mut session = SignalSession::from_prekey_message_real(
        "alice.onion".into(),
        &ciphertext,
        &bundle,
        &reconstructed,
        &identity,
    )
    .unwrap();

    let decrypted = session.decrypt(&ciphertext, true).unwrap();
    assert_eq!(&decrypted[..], b"test message");

    // 5. Delete the stored material
    db.connection()
        .execute("DELETE FROM app_settings WHERE key = ?1", [&key])
        .unwrap();

    // 6. Verify it's gone
    let after_delete = get_app_setting(&db, &key).unwrap();
    assert!(after_delete.is_none(), "PreKey material should be deleted");
}

// ─── Test 5: Encrypted messages survive queue serialization ───────────────────

#[test]
fn test_message_queue_with_encrypted_messages() {
    let alice_identity = IdentityKeypair::generate().unwrap();
    let bob_identity = IdentityKeypair::generate().unwrap();

    let (bob_bundle, bob_private) = PreKeyBundle::generate_real(&bob_identity).unwrap();

    // 1. Alice creates session and encrypts a message
    let mut alice_session = SignalSession::from_prekey_bundle_real(
        "bob.onion".into(),
        &bob_bundle,
        &bob_private,
        &alice_identity,
    )
    .unwrap();

    let plaintext = make_payload("Secret message through the queue", "text");
    let (ciphertext, is_prekey) = alice_session.encrypt(&plaintext).unwrap();

    // 2. Wrap in TextMessage and enqueue
    let wire_msg = wrap_text_message("alice.onion", "bob.onion", &ciphertext, is_prekey);
    let (_tmp, db) = temp_db();
    let queue = MessageQueue::new();
    let queue_id = queue.enqueue(&db, "bob.onion", &wire_msg, "normal").unwrap();
    assert!(queue_id > 0);

    // 3. Retrieve from queue
    let pending = queue.get_pending_messages(&db, i64::MAX).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].peer_onion, "bob.onion");

    // 4. Extract ciphertext from the dequeued message
    let dequeued = &pending[0].message;
    let (recovered_ct_b64, recovered_signal_type) = match dequeued {
        Message::TextMessage(tm) => (&tm.signal_ciphertext, &tm.signal_type),
        other => panic!("Expected TextMessage, got {:?}", other),
    };

    let recovered_ct = STANDARD.decode(recovered_ct_b64).unwrap();
    let is_prekey_from_wire = matches!(recovered_signal_type, SignalMessageType::PrekeyMessage);
    assert_eq!(is_prekey_from_wire, is_prekey);
    assert_eq!(recovered_ct, ciphertext, "Ciphertext must survive queue round-trip");

    // 5. Bob creates session and decrypts
    let mut bob_session = SignalSession::from_prekey_message_real(
        "alice.onion".into(),
        &recovered_ct,
        &bob_bundle,
        &bob_private,
        &bob_identity,
    )
    .unwrap();

    let decrypted = bob_session.decrypt(&recovered_ct, is_prekey_from_wire).unwrap();
    let payload: PlaintextPayload = serde_json::from_slice(&decrypted).unwrap();
    assert_eq!(payload.content, "Secret message through the queue");

    // 6. Mark delivered and verify queue is empty
    queue.mark_delivered(&db, queue_id).unwrap();
    let remaining = queue.get_pending_messages(&db, i64::MAX).unwrap();
    assert!(remaining.is_empty(), "Queue should be empty after delivery");
}

// ─── Test 6: Friend request signature creation and verification ───────────────

#[test]
fn test_friend_request_signature_verification() {
    let alice_identity = IdentityKeypair::generate().unwrap();
    let alice_onion = alice_identity.to_onion_address();
    let friend_code = "happy-1234-tiger-5678";

    // 1. Alice creates a signed friend request
    let request =
        FriendRequestHandler::create_request(&alice_identity, &alice_onion, friend_code).unwrap();

    assert_eq!(request.from_onion, alice_onion);
    assert_eq!(request.from_friendcode, friend_code);
    assert!(!request.signature.is_empty());

    // 2. Verify signature passes validation
    let (_tmp, db) = temp_db();
    let handler = FriendRequestHandler::new(db);
    assert!(
        handler.validate_request(&request).unwrap(),
        "Valid signature should pass verification"
    );

    // 3. Tamper with the message (change from_onion to a different identity)
    let eve_identity = IdentityKeypair::generate().unwrap();
    let mut tampered = request.clone();
    tampered.from_onion = eve_identity.to_onion_address();

    assert!(
        !handler.validate_request(&tampered).unwrap(),
        "Tampered from_onion should fail verification"
    );

    // 4. Tamper with the friend code
    let mut tampered_code = request.clone();
    tampered_code.from_friendcode = "evil-9999-hacker-0000".to_string();
    assert!(
        !handler.validate_request(&tampered_code).unwrap(),
        "Tampered friend code should fail verification"
    );

    // 5. Tamper with the timestamp
    let mut tampered_ts = request.clone();
    tampered_ts.timestamp += 1;
    assert!(
        !handler.validate_request(&tampered_ts).unwrap(),
        "Tampered timestamp should fail verification"
    );

    // 6. Tamper with the signature directly
    let mut tampered_sig = request.clone();
    let mut sig_bytes = STANDARD.decode(&tampered_sig.signature).unwrap();
    sig_bytes[0] ^= 0xFF; // flip bits
    tampered_sig.signature = STANDARD.encode(&sig_bytes);
    assert!(
        !handler.validate_request(&tampered_sig).unwrap(),
        "Corrupted signature should fail verification"
    );
}
