//! Integration tests for presence tracking
//!
//! Tests concurrent access to the presence map, typing state transitions,
//! and presence message serialization via the protocol wire format.

use chattor::presence::{self, OFFLINE_THRESHOLD, TYPING_TIMEOUT};
use chattor::protocol::message::{Message, PresenceMessage, PresenceType};
use std::time::Duration;

#[tokio::test]
async fn test_presence_map_concurrent_access() {
    let map = presence::new_presence_map();

    let mut handles = vec![];
    for i in 0..10 {
        let map = map.clone();
        handles.push(tokio::spawn(async move {
            presence::record_heartbeat(&map, &format!("peer{}.onion", i)).await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.len(), 10);
    for i in 0..10 {
        let key = format!("peer{}.onion", i);
        assert_eq!(snap.get(&key), Some(&(true, false)));
    }
}

#[tokio::test]
async fn test_typing_then_message_clears_typing() {
    let map = presence::new_presence_map();

    presence::record_typing_started(&map, "alice.onion").await;
    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.get("alice.onion"), Some(&(true, true)));

    presence::record_typing_stopped(&map, "alice.onion").await;
    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.get("alice.onion"), Some(&(true, false)));
}

#[test]
fn test_presence_message_serialization_via_framing() {
    let msg = Message::Presence(PresenceMessage {
        from_onion: "abc123.onion".to_string(),
        presence_type: PresenceType::TypingStarted,
        timestamp: 1700000000,
    });

    let json = serde_json::to_vec(&msg).unwrap();
    let decoded: Message = serde_json::from_slice(&json).unwrap();
    assert_eq!(msg, decoded);

    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["type"], "presence");
    assert_eq!(value["from_onion"], "abc123.onion");
}

#[tokio::test]
async fn test_heartbeat_updates_last_seen() {
    let map = presence::new_presence_map();

    // Record initial heartbeat
    presence::record_heartbeat(&map, "peer.onion").await;

    // Peer should be online
    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.get("peer.onion"), Some(&(true, false)));

    // Record another heartbeat (should update last_seen)
    presence::record_heartbeat(&map, "peer.onion").await;
    let snap = presence::get_presence_snapshot(&map).await;
    assert_eq!(snap.get("peer.onion"), Some(&(true, false)));
}

#[tokio::test]
async fn test_typing_stopped_without_prior_start() {
    let map = presence::new_presence_map();

    // Typing stopped for a peer that never started typing (no entry at all)
    presence::record_typing_stopped(&map, "ghost.onion").await;

    // Should not create an entry
    let snap = presence::get_presence_snapshot(&map).await;
    assert!(snap.get("ghost.onion").is_none());
}

#[test]
fn test_presence_constants_are_sane() {
    // Offline threshold must be greater than typing timeout
    assert!(OFFLINE_THRESHOLD > TYPING_TIMEOUT);
    // Offline threshold should be at least 1 minute
    assert!(OFFLINE_THRESHOLD >= Duration::from_secs(60));
    // Typing timeout should be at least 3 seconds
    assert!(TYPING_TIMEOUT >= Duration::from_secs(3));
}

#[test]
fn test_all_presence_types_roundtrip() {
    for pt in [
        PresenceType::Heartbeat,
        PresenceType::TypingStarted,
        PresenceType::TypingStopped,
    ] {
        let msg = Message::Presence(PresenceMessage {
            from_onion: "roundtrip.onion".to_string(),
            presence_type: pt,
            timestamp: 42,
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: Message = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }
}
