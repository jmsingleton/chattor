use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use crate::error::{Result, TorrentChatError};
use crate::protocol::message::Message;

/// Send message with length prefix (works with any async stream)
pub async fn send_message<S>(stream: &mut S, message: &Message) -> Result<()>
where
    S: AsyncWrite + Unpin
{
    // Serialize to JSON
    let json = serde_json::to_vec(message)
        .map_err(|e| TorrentChatError::Network(format!("Failed to serialize: {}", e)))?;

    if json.len() > 10_000_000 {
        return Err(TorrentChatError::Network("Message too large".into()));
    }

    // Write length prefix (4 bytes, big-endian)
    let len = (json.len() as u32).to_be_bytes();
    stream.write_all(&len).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to write length: {}", e)))?;

    // Write message payload
    stream.write_all(&json).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to write payload: {}", e)))?;

    // Flush to ensure sent
    stream.flush().await
        .map_err(|e| TorrentChatError::Network(format!("Failed to flush: {}", e)))?;

    Ok(())
}

/// Receive message with length prefix (works with any async stream)
pub async fn receive_message<S>(stream: &mut S) -> Result<Message>
where
    S: AsyncRead + Unpin
{
    // Read length prefix (4 bytes, big-endian)
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to read length: {}", e)))?;

    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 10_000_000 {
        return Err(TorrentChatError::Network("Message too large".into()));
    }

    // Read message payload
    let mut json_bytes = vec![0u8; len];
    stream.read_exact(&mut json_bytes).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to read payload: {}", e)))?;

    // Deserialize message
    let message: Message = serde_json::from_slice(&json_bytes)
        .map_err(|e| TorrentChatError::Network(format!("Failed to parse message: {}", e)))?;

    Ok(message)
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
            let msg = receive_message(&mut stream).await.unwrap();

            // Echo back
            send_message(&mut stream, &msg).await.unwrap();
        });

        // Client
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

        let original = Message::TextMessage(TextMessage {
            from_onion: "alice.onion".into(),
            to_onion: "bob.onion".into(),
            signal_ciphertext: "encrypted_test_data".into(),
            signal_type: SignalMessageType::Message,
            timestamp: 12345,
            message_id: uuid::Uuid::new_v4(),
        });

        send_message(&mut stream, &original).await.unwrap();
        let received = receive_message(&mut stream).await.unwrap();

        assert_eq!(format!("{:?}", original), format!("{:?}", received));
    }

    #[tokio::test]
    async fn test_generic_framing_with_different_streams() {
        use uuid::Uuid;

        let (mut client, mut server) = tokio::io::duplex(1024);

        let message = Message::TextMessage(TextMessage {
            from_onion: "alice.onion".into(),
            to_onion: "bob.onion".into(),
            signal_ciphertext: "test_ciphertext".into(),
            signal_type: SignalMessageType::Message,
            timestamp: 12345,
            message_id: Uuid::new_v4(),
        });

        let message_clone = message.clone();

        // Send on one end
        tokio::spawn(async move {
            send_message(&mut client, &message_clone).await.unwrap();
        });

        // Receive on other end
        let received = receive_message(&mut server).await.unwrap();

        // Verify
        assert_eq!(format!("{:?}", message), format!("{:?}", received));
    }
}
