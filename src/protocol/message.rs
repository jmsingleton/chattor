use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current protocol version.  Bumped whenever the wire format changes in a
/// backward-incompatible way so that peers running different versions can
/// detect the mismatch early instead of silently mis-parsing messages.
pub const PROTOCOL_VERSION: u8 = 2;

/// Versioned envelope that wraps every `Message` on the wire.
///
/// All framing I/O serializes and deserializes `MessageEnvelope` rather than
/// bare `Message` values, giving us a clean place to negotiate or reject
/// incompatible protocol versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageEnvelope {
    pub version: u8,
    pub payload: Message,
}

impl MessageEnvelope {
    /// Create a new envelope stamped with the current protocol version.
    pub fn new(payload: Message) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "friend_request")]
    FriendRequest(FriendRequestMessage),

    #[serde(rename = "friend_request_accept")]
    FriendRequestAccept(FriendRequestAcceptMessage),

    #[serde(rename = "friend_request_reject")]
    FriendRequestReject(FriendRequestRejectMessage),

    #[serde(rename = "message")]
    TextMessage(TextMessage),

    #[serde(rename = "delivery_receipt")]
    DeliveryReceipt(DeliveryReceiptMessage),

    #[serde(rename = "read_receipt")]
    ReadReceipt(DeliveryReceiptMessage),

    #[serde(rename = "channel_subscribe")]
    ChannelSubscribe(ChannelSubscribeMessage),

    #[serde(rename = "channel_unsubscribe")]
    ChannelUnsubscribe(ChannelUnsubscribeMessage),

    #[serde(rename = "channel_post")]
    ChannelPost(ChannelPostMessage),

    #[serde(rename = "channel_sync_request")]
    ChannelSyncRequest(ChannelSyncRequestMessage),

    #[serde(rename = "channel_sync_response")]
    ChannelSyncResponse(ChannelSyncResponseMessage),

    #[serde(rename = "channel_post_receipt")]
    ChannelPostReceipt(ChannelPostReceiptMessage),

    #[serde(rename = "presence")]
    Presence(PresenceMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FriendRequestMessage {
    pub from_onion: String,
    pub from_friendcode: String,
    pub timestamp: i64,
    pub signature: String,  // base64 ed25519 signature
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FriendRequestAcceptMessage {
    pub from_onion: String,
    pub to_onion: String,
    pub signal_prekey_bundle: String,  // Serialized PreKey bundle
    pub timestamp: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FriendRequestRejectMessage {
    pub from_onion: String,
    pub to_onion: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextMessage {
    pub from_onion: String,
    pub to_onion: String,
    pub signal_header: String,      // base64-encoded encrypted Double Ratchet header
    pub signal_ciphertext: String,  // base64 encrypted payload
    pub signal_type: SignalMessageType,
    pub timestamp: i64,
    pub message_id: Uuid,
    /// X3DH initiator data, only present on PreKey messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x3dh_init: Option<X3DHInitData>,
}

/// X3DH key exchange initiator data, sent with the first (PreKey) message.
///
/// Contains the sender's identity and ephemeral public keys so the
/// receiver can run x3dh_responder() and establish a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct X3DHInitData {
    /// base64-encoded 33-byte (0x05-prefixed) X25519 identity public key
    pub sender_identity_key: String,
    /// base64-encoded 33-byte (0x05-prefixed) X25519 ephemeral public key
    pub sender_ephemeral_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SignalMessageType {
    PrekeyMessage,
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeliveryReceiptMessage {
    pub message_id: Uuid,
    pub timestamp: i64,
}

/// Plaintext payload before encryption
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlaintextPayload {
    pub content: String,
    pub sent_at: i64,
    pub message_type: String,  // "text", "typing_indicator", etc.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ephemeral_ttl: Option<i64>,  // seconds until deletion after read
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Public,
    FriendsOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelPostMessage {
    pub publisher_onion: String,
    pub channel_type: ChannelType,
    pub post_id: Uuid,
    pub content: String,
    pub created_at: i64,
    pub signature: String,
}

impl ChannelPostMessage {
    /// Reconstruct the bytes that were signed when this post was published.
    /// Must match the format used by `PublishChannelPost` in main.rs.
    fn signed_data(&self) -> String {
        format!("{}{}{}", self.post_id, self.content, self.created_at)
    }

    /// Verify the Ed25519 signature on this post against the publisher's
    /// v3 .onion address (which encodes their Ed25519 verifying key).
    ///
    /// Returns Ok(true) only if the signature is authentic. All decode or
    /// length failures yield Ok(false) — the post is rejected, not surfaced
    /// as an error.
    pub fn verify_signature(&self) -> bool {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        use base64::Engine;

        let pubkey_bytes = match crate::protocol::friend_code::onion_to_pubkey(&self.publisher_onion) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let verifying_key = match VerifyingKey::from_bytes(&pubkey_bytes) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(&self.signature) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_array: [u8; 64] = match sig_bytes.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let signature = Signature::from_bytes(&sig_array);

        verifying_key.verify(self.signed_data().as_bytes(), &signature).is_ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSubscribeMessage {
    pub subscriber_onion: String,
    pub channel_type: ChannelType,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelUnsubscribeMessage {
    pub subscriber_onion: String,
    pub channel_type: ChannelType,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSyncRequestMessage {
    pub subscriber_onion: String,
    pub channel_type: ChannelType,
    pub since_timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSyncResponseMessage {
    pub publisher_onion: String,
    pub channel_type: ChannelType,
    pub posts: Vec<ChannelPostMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelPostReceiptMessage {
    pub post_id: Uuid,
    pub reader_onion: String,
    pub timestamp: i64,
}

/// Type of presence update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PresenceType {
    Heartbeat,
    TypingStarted,
    TypingStopped,
}

/// Lightweight presence message (not encrypted — Tor provides transport security)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresenceMessage {
    pub from_onion: String,
    pub presence_type: PresenceType,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friend_request_serialization() {
        let msg = Message::FriendRequest(FriendRequestMessage {
            from_onion: "alice.onion".to_string(),
            from_friendcode: "happy-1234-tiger-5678".to_string(),
            timestamp: 1234567890,
            signature: "sig123".to_string(),
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("friend_request"));
        assert!(json.contains("alice.onion"));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_text_message_serialization() {
        let msg = Message::TextMessage(TextMessage {
            from_onion: "alice.onion".to_string(),
            to_onion: "bob.onion".to_string(),
            signal_header: "header_data".to_string(),
            signal_ciphertext: "encrypted".to_string(),
            signal_type: SignalMessageType::Message,
            timestamp: 1234567890,
            message_id: Uuid::new_v4(),
            x3dh_init: None,
        });

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_delivery_receipt_serialization() {
        let msg = Message::DeliveryReceipt(DeliveryReceiptMessage {
            message_id: Uuid::new_v4(),
            timestamp: 1234567890,
        });

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_read_receipt_serialization() {
        let msg = Message::ReadReceipt(DeliveryReceiptMessage {
            message_id: Uuid::new_v4(),
            timestamp: 1234567890,
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("read_receipt"));
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_plaintext_payload() {
        let payload = PlaintextPayload {
            content: "Hello, Bob!".to_string(),
            sent_at: 1234567890,
            message_type: "text".to_string(),
            ephemeral_ttl: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: PlaintextPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, deserialized);
    }

    #[test]
    fn test_channel_post_serialization() {
        let msg = Message::ChannelPost(ChannelPostMessage {
            publisher_onion: "alice.onion".into(),
            channel_type: ChannelType::Public,
            post_id: Uuid::new_v4(),
            content: "Hello world!".into(),
            created_at: 1234567890,
            signature: "sig123".into(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("channel_post"));
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    /// Mirror of the production publish path in main.rs — sign
    /// `post_id || content || created_at` with the publisher's identity key
    /// and base64-encode the signature.
    fn sign_post(identity: &crate::crypto::IdentityKeypair, post: &mut ChannelPostMessage) {
        use base64::Engine;
        let data = format!("{}{}{}", post.post_id, post.content, post.created_at);
        post.signature = base64::engine::general_purpose::STANDARD
            .encode(identity.sign(data.as_bytes()).to_bytes());
    }

    #[test]
    fn test_channel_post_signature_verifies_real_publisher() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let mut post = ChannelPostMessage {
            publisher_onion: identity.to_onion_address(),
            channel_type: ChannelType::Public,
            post_id: Uuid::new_v4(),
            content: "Hello!".into(),
            created_at: 1_700_000_000,
            signature: String::new(),
        };
        sign_post(&identity, &mut post);

        assert!(post.verify_signature(), "Genuine signature should verify");
    }

    #[test]
    fn test_channel_post_signature_rejects_publisher_spoof() {
        // Alice signs; an attacker rewrites publisher_onion to Bob's address.
        let alice = crate::crypto::IdentityKeypair::generate().unwrap();
        let bob = crate::crypto::IdentityKeypair::generate().unwrap();

        let mut post = ChannelPostMessage {
            publisher_onion: alice.to_onion_address(),
            channel_type: ChannelType::Public,
            post_id: Uuid::new_v4(),
            content: "I am Bob, trust me".into(),
            created_at: 1_700_000_000,
            signature: String::new(),
        };
        sign_post(&alice, &mut post);
        post.publisher_onion = bob.to_onion_address();

        assert!(!post.verify_signature(), "Spoofed publisher_onion must not verify");
    }

    #[test]
    fn test_channel_post_signature_rejects_tampered_content() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let mut post = ChannelPostMessage {
            publisher_onion: identity.to_onion_address(),
            channel_type: ChannelType::Public,
            post_id: Uuid::new_v4(),
            content: "original".into(),
            created_at: 1_700_000_000,
            signature: String::new(),
        };
        sign_post(&identity, &mut post);
        post.content = "tampered".into();

        assert!(!post.verify_signature(), "Tampered content must invalidate signature");
    }

    #[test]
    fn test_channel_post_signature_rejects_malformed_signature() {
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let post = ChannelPostMessage {
            publisher_onion: identity.to_onion_address(),
            channel_type: ChannelType::Public,
            post_id: Uuid::new_v4(),
            content: "hello".into(),
            created_at: 1_700_000_000,
            signature: "not-base64!!!".into(),
        };
        assert!(!post.verify_signature(), "Garbage signature must not verify");
    }

    #[test]
    fn test_channel_subscribe_serialization() {
        let msg = Message::ChannelSubscribe(ChannelSubscribeMessage {
            subscriber_onion: "bob.onion".into(),
            channel_type: ChannelType::Public,
            timestamp: 1234567890,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_channel_sync_request_serialization() {
        let msg = Message::ChannelSyncRequest(ChannelSyncRequestMessage {
            subscriber_onion: "bob.onion".into(),
            channel_type: ChannelType::FriendsOnly,
            since_timestamp: 1234567890,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_channel_sync_response_serialization() {
        let msg = Message::ChannelSyncResponse(ChannelSyncResponseMessage {
            publisher_onion: "alice.onion".into(),
            channel_type: ChannelType::Public,
            posts: vec![
                ChannelPostMessage {
                    publisher_onion: "alice.onion".into(),
                    channel_type: ChannelType::Public,
                    post_id: Uuid::new_v4(),
                    content: "Post 1".into(),
                    created_at: 1000,
                    signature: "sig1".into(),
                },
            ],
        });
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_channel_unsubscribe_serialization() {
        let msg = Message::ChannelUnsubscribe(ChannelUnsubscribeMessage {
            subscriber_onion: "bob.onion".into(),
            channel_type: ChannelType::FriendsOnly,
            timestamp: 1234567890,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("channel_unsubscribe"));
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_channel_post_receipt_serialization() {
        let msg = Message::ChannelPostReceipt(ChannelPostReceiptMessage {
            post_id: Uuid::new_v4(),
            reader_onion: "bob.onion".into(),
            timestamp: 1234567890,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_presence_message_serialization() {
        let msg = Message::Presence(PresenceMessage {
            from_onion: "test.onion".to_string(),
            presence_type: PresenceType::Heartbeat,
            timestamp: 1000,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_presence_typing_roundtrip() {
        for pt in [PresenceType::Heartbeat, PresenceType::TypingStarted, PresenceType::TypingStopped] {
            let msg = Message::Presence(PresenceMessage {
                from_onion: "peer.onion".to_string(),
                presence_type: pt.clone(),
                timestamp: 42,
            });
            let bytes = serde_json::to_vec(&msg).unwrap();
            let decoded: Message = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(msg, decoded);
        }
    }

    #[test]
    fn test_message_envelope_serialization_roundtrip() {
        let inner = Message::FriendRequest(FriendRequestMessage {
            from_onion: "alice.onion".to_string(),
            from_friendcode: "happy-1234-tiger-5678".to_string(),
            timestamp: 1234567890,
            signature: "sig123".to_string(),
        });

        let envelope = MessageEnvelope::new(inner.clone());
        assert_eq!(envelope.version, PROTOCOL_VERSION);
        assert_eq!(envelope.payload, inner);

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"version\":2"));

        let deserialized: MessageEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope, deserialized);
    }

    #[test]
    fn test_message_envelope_preserves_all_message_types() {
        // Verify enveloping works with every variant
        let messages = vec![
            Message::DeliveryReceipt(DeliveryReceiptMessage {
                message_id: Uuid::new_v4(),
                timestamp: 100,
            }),
            Message::Presence(PresenceMessage {
                from_onion: "peer.onion".to_string(),
                presence_type: PresenceType::Heartbeat,
                timestamp: 200,
            }),
        ];

        for msg in messages {
            let env = MessageEnvelope::new(msg.clone());
            let bytes = serde_json::to_vec(&env).unwrap();
            let decoded: MessageEnvelope = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(decoded.version, PROTOCOL_VERSION);
            assert_eq!(decoded.payload, msg);
        }
    }
}
