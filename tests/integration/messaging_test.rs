//! Integration Tests for Phase 2
//!
//! Tests that verify components work together correctly.

use chattor::db::Database;
use chattor::crypto::signal::SignalSession;
use chattor::net::queue::MessageQueue;
use chattor::tor::address::onion_to_friend_code;
use chattor::protocol::friend_code::friend_code_to_onion;


fn setup_test_db() -> Database {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(chattor::db::schema::CREATE_TABLES).unwrap();

    // Create test data
    conn.execute(
        "INSERT INTO friends (onion_address, friend_code, display_name, added_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        ["alice.onion", "ALICE-1234-TEST-5678", "Alice", "1234567890", "active"],
    ).unwrap();

    conn.execute(
        "INSERT INTO conversations (friend_id, is_ephemeral, created_at)
         VALUES (1, 0, 1234567890)",
        [],
    ).unwrap();

    Database::from_connection(conn)
}

#[test]
fn test_address_mapping_roundtrip() {
    // Generate a valid v3 .onion from a known identity
    let identity = chattor::crypto::IdentityKeypair::generate().unwrap();
    let onion = identity.to_onion_address();

    // Encode to friend code
    let friend_code = onion_to_friend_code(&onion).unwrap();

    // Verify format: 8 groups of 4 words
    assert!(friend_code.contains('-'));
    let groups: Vec<&str> = friend_code.split(' ').collect();
    assert_eq!(groups.len(), 8);

    // Decode back to .onion — should be reversible
    let result = friend_code_to_onion(&friend_code).unwrap();
    assert_eq!(result, onion);
}

#[test]
fn test_message_queue_integration() {
    use chattor::protocol::message::{Message, FriendRequestMessage};

    let db = setup_test_db();
    let queue = MessageQueue::new();

    // Create a protocol message to enqueue
    let msg = Message::FriendRequest(FriendRequestMessage {
        from_onion: "alice.onion".to_string(),
        from_friendcode: "ALICE-1234".to_string(),
        timestamp: 1234567890,
        signature: "sig123".to_string(),
    });

    let queue_id = queue.enqueue(&db, "alice.onion", &msg, "normal").unwrap();
    assert!(queue_id > 0);

    // Retrieve queued messages (use far-future timestamp so it's due)
    let pending = queue.get_pending_messages(&db, i64::MAX).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].peer_onion, "alice.onion");
    assert_eq!(pending[0].priority, "normal");

    // Mark as delivered
    queue.mark_delivered(&db, queue_id).unwrap();
    let pending = queue.get_pending_messages(&db, i64::MAX).unwrap();
    assert_eq!(pending.len(), 0);
}

#[test]
fn test_signal_session_creation() {
    use chattor::crypto::signal::PreKeyBundle;

    /// Helper: generate an X25519 Signal identity keypair for tests.
    fn gen_signal_identity() -> ([u8; 32], [u8; 32]) {
        let kp = libsignal_protocol::vxeddsa::gen_keypair();
        let raw_pub = libsignal_protocol::utils::decode_public_key(&kp.public).unwrap();
        (kp.secret, raw_pub)
    }

    // Create real sessions for Alice (sender) and Bob (receiver)
    let (alice_signal_secret, _alice_signal_public) = gen_signal_identity();
    let (bob_signal_secret, bob_signal_public) = gen_signal_identity();
    let (bob_bundle, bob_private) =
        PreKeyBundle::generate_real(&bob_signal_secret, &bob_signal_public).unwrap();

    // Alice creates session with Bob's bundle
    let (mut alice_session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
        "bob.onion".into(),
        &bob_bundle,
        &bob_private,
        &alice_signal_secret,
    ).unwrap();
    assert_eq!(alice_session.remote_onion, "bob.onion");

    // Alice encrypts a message
    let plaintext = b"Hello, Bob!";
    let (header, ciphertext, is_prekey) = alice_session.encrypt(plaintext).unwrap();
    assert!(is_prekey); // First message is PreKey type

    // Bob creates session from Alice's X3DH init data and decrypts
    let alice_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&alice_signal_secret);
    let (mut bob_session, _bob_ad) = SignalSession::from_prekey_message_real(
        "alice.onion".into(),
        &bob_private,
        &alice_identity_encoded,
        &ephemeral_public,
    ).unwrap();
    let decrypted = bob_session.decrypt(&header, &ciphertext).unwrap();
    assert_eq!(plaintext, &decrypted[..]);
}

#[test]
fn test_database_schema_complete() {
    let db = setup_test_db();
    let conn = db.connection();

    // Verify all Phase 2 tables exist
    let tables = vec![
        "message_queue",
        "signal_sessions",
        "blocked_onions",
        "messages_fts",
    ];

    for table in tables {
        let result: Result<String, _> = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |row| row.get(0),
        );
        assert!(result.is_ok(), "Table {} should exist", table);
    }
}

#[test]
fn test_protocol_message_serialization() {
    use chattor::protocol::message::{Message, FriendRequestMessage};

    let friend_request = FriendRequestMessage {
        from_onion: "alice.onion".to_string(),
        from_friendcode: "ALICE-1234".to_string(),
        timestamp: 1234567890,
        signature: "sig123".to_string(),
    };

    let message = Message::FriendRequest(friend_request);

    // Test serialization
    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("friend_request"));

    // Test deserialization
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, Message::FriendRequest(_)));
}

#[test]
fn test_full_conversation_flow() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let db = chattor::db::Database::open(temp.path()).unwrap();

    // Add a friend
    db.connection().execute(
        "INSERT INTO friends (onion_address, display_name, added_at, status)
         VALUES ('bob.onion', 'Bob', 1000, 'active')",
        [],
    ).unwrap();

    // Get friends
    let friends = chattor::db::queries::get_friends_with_unread(&db).unwrap();
    assert_eq!(friends.len(), 1);
    assert_eq!(friends[0].display_name, Some("Bob".to_string()));

    // Create conversation
    let conv_id = chattor::db::queries::get_or_create_conversation(&db, friends[0].friend_id).unwrap();

    // Send a message
    chattor::db::queries::store_outgoing_message(&db, conv_id, "me.onion", "Hello Bob!", "msg-001").unwrap();

    // Receive a reply
    chattor::db::queries::store_incoming_message(&db, conv_id, "bob.onion", "Hey there!", "msg-002").unwrap();

    // Check messages
    let messages = chattor::db::queries::get_messages(&db, conv_id, 50, 0).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "Hello Bob!");
    assert_eq!(messages[0].status, "sent");
    assert_eq!(messages[1].content, "Hey there!");
    assert_eq!(messages[1].status, "received");

    // Check unread (bob's message is unread)
    let friends = chattor::db::queries::get_friends_with_unread(&db).unwrap();
    assert_eq!(friends[0].unread_count, 1);

    // Mark read
    chattor::db::queries::mark_conversation_read(&db, conv_id).unwrap();
    let friends = chattor::db::queries::get_friends_with_unread(&db).unwrap();
    assert_eq!(friends[0].unread_count, 0);
}

#[test]
fn test_find_friend_for_incoming_message() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let db = chattor::db::Database::open(temp.path()).unwrap();

    // No friends yet
    assert_eq!(chattor::db::queries::find_friend_by_onion(&db, "unknown.onion").unwrap(), None);

    // Add friend
    db.connection().execute(
        "INSERT INTO friends (onion_address, display_name, added_at, status)
         VALUES ('alice.onion', 'Alice', 1000, 'active')",
        [],
    ).unwrap();

    // Now findable
    assert!(chattor::db::queries::find_friend_by_onion(&db, "alice.onion").unwrap().is_some());
}

#[test]
fn test_channel_post_flow() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let db = chattor::db::Database::open(temp.path()).unwrap();

    // Initialize channels (creates Public and Friends Only channels)
    chattor::db::queries::initialize_channels(&db).unwrap();

    // Publish posts to the public channel for this user.
    let me = "me.onion";
    for i in 0..5 {
        chattor::db::queries::store_channel_post(
            &db, me, "public", &format!("Post {}", i),
            &format!("post-{}", i), (1000 + i) as i64, "sig"
        ).unwrap();
    }

    // Retrieve posts (newest first)
    let posts = chattor::db::queries::get_channel_posts(&db, me, "public", 50).unwrap();
    assert_eq!(posts.len(), 5);
    assert_eq!(posts[0].content, "Post 4"); // newest first

    // Get posts since timestamp (oldest first, for sync)
    let since_posts = chattor::db::queries::get_channel_posts_since(&db, me, "public", 1002).unwrap();
    assert_eq!(since_posts.len(), 2); // posts 3 and 4 (created_at > 1002)

    // Subscriber management
    chattor::db::queries::add_channel_subscriber(&db, "bob.onion", "public").unwrap();
    let subs = chattor::db::queries::get_channel_subscribers(&db, "public").unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0], "bob.onion");

    // Unsubscribe
    chattor::db::queries::remove_channel_subscriber(&db, "bob.onion", "public").unwrap();
    let subs = chattor::db::queries::get_channel_subscribers(&db, "public").unwrap();
    assert_eq!(subs.len(), 0);

    // Subscription management
    chattor::db::queries::add_channel_subscription(&db, "alice.onion", "public").unwrap();
    let subscriptions = chattor::db::queries::get_channel_subscriptions(&db).unwrap();
    assert_eq!(subscriptions.len(), 1);
    assert_eq!(subscriptions[0].publisher_onion, "alice.onion");

    // Update sync time
    chattor::db::queries::update_subscription_sync_time(&db, "alice.onion", "public", 5000).unwrap();
    let subscriptions = chattor::db::queries::get_channel_subscriptions(&db).unwrap();
    assert_eq!(subscriptions[0].last_sync_at, Some(5000));

    // Read receipts
    chattor::db::queries::store_channel_post_receipt(&db, "post-1", "bob.onion", 2000).unwrap();
    chattor::db::queries::store_channel_post_receipt(&db, "post-1", "charlie.onion", 2001).unwrap();
    let count = chattor::db::queries::get_channel_post_read_count(&db, "post-1").unwrap();
    assert_eq!(count, 2);

    // Retention enforcement (per publisher_onion + channel_type)
    for i in 5..110 {
        chattor::db::queries::store_channel_post(
            &db, me, "public", &format!("Post {}", i),
            &format!("post-{}", i), (1000 + i) as i64, "sig"
        ).unwrap();
    }
    let deleted = chattor::db::queries::enforce_channel_retention(&db, me, "public").unwrap();
    assert!(deleted > 0);
    let remaining = chattor::db::queries::get_channel_posts(&db, me, "public", 200).unwrap();
    assert_eq!(remaining.len(), 100);
}

#[test]
fn test_channel_protocol_messages() {
    use chattor::protocol::message::*;

    // Test ChannelPost serialization
    let post = Message::ChannelPost(ChannelPostMessage {
        publisher_onion: "alice.onion".to_string(),
        channel_type: ChannelType::Public,
        post_id: uuid::Uuid::nil(),
        content: "Hello world".to_string(),
        created_at: 1000,
        signature: "sig".to_string(),
    });
    let json = serde_json::to_string(&post).unwrap();
    let deser: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(deser, Message::ChannelPost(_)));

    // Test ChannelSyncRequest
    let sync = Message::ChannelSyncRequest(ChannelSyncRequestMessage {
        subscriber_onion: "bob.onion".to_string(),
        channel_type: ChannelType::FriendsOnly,
        since_timestamp: 500,
    });
    let json = serde_json::to_string(&sync).unwrap();
    let deser: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(deser, Message::ChannelSyncRequest(_)));
}
