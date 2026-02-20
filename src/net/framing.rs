use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use crate::error::{Result, ChattorError};
use crate::protocol::message::{MessageEnvelope, PROTOCOL_VERSION};

/// Send a `MessageEnvelope` with a 4-byte big-endian length prefix.
pub async fn send_message<S>(stream: &mut S, envelope: &MessageEnvelope) -> Result<()>
where
    S: AsyncWrite + Unpin
{
    // Serialize to JSON
    let json = serde_json::to_vec(envelope)
        .map_err(|e| ChattorError::Network(format!("Failed to serialize: {}", e)))?;

    if json.len() > 10_000_000 {
        return Err(ChattorError::Network("Message too large".into()));
    }

    // Write length prefix (4 bytes, big-endian)
    let len = (json.len() as u32).to_be_bytes();
    stream.write_all(&len).await
        .map_err(|e| ChattorError::Network(format!("Failed to write length: {}", e)))?;

    // Write message payload
    stream.write_all(&json).await
        .map_err(|e| ChattorError::Network(format!("Failed to write payload: {}", e)))?;

    // Flush to ensure sent
    stream.flush().await
        .map_err(|e| ChattorError::Network(format!("Failed to flush: {}", e)))?;

    Ok(())
}

/// Receive a `MessageEnvelope` with a 4-byte big-endian length prefix.
///
/// Returns an error if the envelope's protocol version is not supported.
pub async fn receive_message<S>(stream: &mut S) -> Result<MessageEnvelope>
where
    S: AsyncRead + Unpin
{
    // Read length prefix (4 bytes, big-endian)
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await
        .map_err(|e| ChattorError::Network(format!("Failed to read length: {}", e)))?;

    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 10_000_000 {
        return Err(ChattorError::Network("Message too large".into()));
    }

    // Read message payload
    let mut json_bytes = vec![0u8; len];
    stream.read_exact(&mut json_bytes).await
        .map_err(|e| ChattorError::Network(format!("Failed to read payload: {}", e)))?;

    // Deserialize envelope
    let envelope: MessageEnvelope = serde_json::from_slice(&json_bytes)
        .map_err(|e| ChattorError::Network(format!("Failed to parse message: {}", e)))?;

    // Validate protocol version
    if envelope.version != PROTOCOL_VERSION {
        return Err(ChattorError::Network(format!(
            "Unsupported protocol version {} (expected {})",
            envelope.version, PROTOCOL_VERSION
        )));
    }

    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::message::*;

    #[tokio::test]
    async fn test_send_receive_roundtrip() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Server task
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let envelope = receive_message(&mut stream).await.unwrap();

            // Echo back
            send_message(&mut stream, &envelope).await.unwrap();
        });

        // Client
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

        let original = Message::TextMessage(TextMessage {
            from_onion: "alice.onion".into(),
            to_onion: "bob.onion".into(),
            signal_header: "header_data".into(),
            signal_ciphertext: "encrypted_test_data".into(),
            signal_type: SignalMessageType::Message,
            timestamp: 12345,
            message_id: uuid::Uuid::new_v4(),
            x3dh_init: None,
        });

        let envelope = MessageEnvelope::new(original.clone());
        send_message(&mut stream, &envelope).await.unwrap();
        let received = receive_message(&mut stream).await.unwrap();

        assert_eq!(received.version, PROTOCOL_VERSION);
        assert_eq!(format!("{:?}", original), format!("{:?}", received.payload));
    }

    #[tokio::test]
    async fn test_generic_framing_with_different_streams() {
        use uuid::Uuid;

        let (mut client, mut server) = tokio::io::duplex(1024);

        let message = Message::TextMessage(TextMessage {
            from_onion: "alice.onion".into(),
            to_onion: "bob.onion".into(),
            signal_header: "test_header".into(),
            signal_ciphertext: "test_ciphertext".into(),
            signal_type: SignalMessageType::Message,
            timestamp: 12345,
            message_id: Uuid::new_v4(),
            x3dh_init: None,
        });

        let envelope = MessageEnvelope::new(message.clone());

        // Send on one end
        tokio::spawn(async move {
            send_message(&mut client, &envelope).await.unwrap();
        });

        // Receive on other end
        let received = receive_message(&mut server).await.unwrap();

        // Verify
        assert_eq!(received.version, PROTOCOL_VERSION);
        assert_eq!(format!("{:?}", message), format!("{:?}", received.payload));
    }

    #[tokio::test]
    async fn test_receive_rejects_unsupported_version() {
        let (mut client, mut server) = tokio::io::duplex(1024);

        // Manually construct an envelope with a bad version
        let bad_envelope = serde_json::json!({
            "version": 99,
            "payload": {
                "type": "presence",
                "from_onion": "test.onion",
                "presence_type": "Heartbeat",
                "timestamp": 1000
            }
        });
        let json = serde_json::to_vec(&bad_envelope).unwrap();

        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let len = (json.len() as u32).to_be_bytes();
            client.write_all(&len).await.unwrap();
            client.write_all(&json).await.unwrap();
            client.flush().await.unwrap();
        });

        let result = receive_message(&mut server).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unsupported protocol version 99"));
    }
}
