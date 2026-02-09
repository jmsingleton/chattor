use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use crate::error::Result;
use crate::protocol::message::Message;
use std::io;

/// Message received from peer
pub struct IncomingMessage {
    pub message: Message,
    pub remote_addr: String,
}

/// Listen for incoming TCP connections
pub async fn listen_for_connections(
    listener: TcpListener,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, addr.to_string(), tx).await {
                        eprintln!("Connection handler error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Handle single incoming connection
async fn handle_connection(
    mut stream: TcpStream,
    remote_addr: String,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Read length prefix (4 bytes, big-endian)
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await
        .map_err(|e| crate::error::TorrentChatError::Network(format!("Failed to read length: {}", e)))?;

    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 10_000_000 { // 10MB max
        return Err(crate::error::TorrentChatError::Network("Message too large".into()));
    }

    // Read message payload
    let mut json_bytes = vec![0u8; len];
    stream.read_exact(&mut json_bytes).await
        .map_err(|e| crate::error::TorrentChatError::Network(format!("Failed to read payload: {}", e)))?;

    // Deserialize message
    let message: Message = serde_json::from_slice(&json_bytes)
        .map_err(|e| crate::error::TorrentChatError::Network(format!("Failed to parse message: {}", e)))?;

    // Send to app
    tx.send(IncomingMessage { message, remote_addr }).await
        .map_err(|e| crate::error::TorrentChatError::Network(format!("Failed to send to app: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_listener_accepts_connections() {
        use tokio::io::AsyncWriteExt;
        use crate::protocol::message::{Message, DeliveryReceiptMessage};
        use uuid::Uuid;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        // Spawn listener
        tokio::spawn(async move {
            listen_for_connections(listener, tx).await
        });

        // Connect as client and send a message
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

        // Create a test message
        let msg = Message::DeliveryReceipt(DeliveryReceiptMessage {
            message_id: Uuid::new_v4(),
            timestamp: 1234567890,
        });

        let json = serde_json::to_vec(&msg).unwrap();
        let len = json.len() as u32;

        // Send length prefix
        stream.write_all(&len.to_be_bytes()).await.unwrap();
        // Send payload
        stream.write_all(&json).await.unwrap();
        stream.flush().await.unwrap();

        // Verify connection accepted and message received
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv()
        ).await;

        assert!(result.is_ok());
        let incoming = result.unwrap().unwrap();
        assert!(matches!(incoming.message, Message::DeliveryReceipt(_)));
    }
}
