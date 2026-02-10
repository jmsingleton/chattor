//! Integration Tests for Phase 2
//!
//! Tests that verify components work together correctly.

use torrent_chat::db::Database;
use torrent_chat::crypto::signal::SignalSession;
use torrent_chat::net::queue::MessageQueue;
use torrent_chat::tor::address::{onion_to_friend_code, friend_code_to_onion};
use std::collections::HashMap;

fn setup_test_db() -> Database {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(torrent_chat::db::CREATE_TABLES).unwrap();

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
    // Test that friend code generation is deterministic
    let onion = "3g2upl4pq6kufc4m2kyd56yz3b4qbeteqbqndzvt3sp6hhfjdkhqiiqd.onion";
    let friend_code = onion_to_friend_code(onion).unwrap();

    // Verify format
    assert!(friend_code.contains('-'));
    let parts: Vec<&str> = friend_code.split('-').collect();
    assert_eq!(parts.len(), 4);

    // Test reverse lookup
    let mut mapping = HashMap::new();
    mapping.insert(friend_code.clone(), onion.to_string());

    let result = friend_code_to_onion(&friend_code, &mapping).unwrap();
    assert_eq!(result, onion);
}

#[test]
fn test_message_queue_integration() {
    use torrent_chat::protocol::message::{Message, FriendRequestMessage};

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
    use torrent_chat::crypto::signal::PreKeyBundle;

    // Test Signal session stub from PreKey bundle
    let bundle = PreKeyBundle::generate().unwrap();
    let mut session = SignalSession::from_prekey_bundle("bob.onion".to_string(), &bundle).unwrap();
    assert_eq!(session.remote_onion, "bob.onion");

    // Test encrypt/decrypt stubs
    let plaintext = b"Hello, Bob!";
    let (ciphertext, _is_prekey) = session.encrypt(plaintext).unwrap();
    let decrypted = session.decrypt(&ciphertext).unwrap();
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
    use torrent_chat::protocol::message::{Message, FriendRequestMessage};

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
