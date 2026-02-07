use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub signal_ciphertext: String,  // base64 encrypted payload
    pub signal_type: SignalMessageType,
    pub timestamp: i64,
    pub message_id: Uuid,
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
            signal_ciphertext: "encrypted".to_string(),
            signal_type: SignalMessageType::Message,
            timestamp: 1234567890,
            message_id: Uuid::new_v4(),
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
    fn test_plaintext_payload() {
        let payload = PlaintextPayload {
            content: "Hello, Bob!".to_string(),
            sent_at: 1234567890,
            message_type: "text".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: PlaintextPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, deserialized);
    }
}
