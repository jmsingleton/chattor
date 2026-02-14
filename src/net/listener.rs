use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use crate::error::Result;
use crate::protocol::message::Message;
use tor_hsservice::{handle_rend_requests, RendRequest};
use tor_cell::relaycell::msg::Connected;
use futures::StreamExt;


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
    // Use shared framing module instead of duplicating read logic
    let message = crate::net::framing::receive_message(&mut stream).await?;

    // Send to app
    tx.send(IncomingMessage { message, remote_addr }).await
        .map_err(|e| crate::error::TorrentChatError::Network(format!("Failed to send to app: {}", e)))?;

    Ok(())
}

/// Listen for incoming connections via Tor onion service rendezvous.
///
/// Takes the RendRequest stream from `HiddenService::launch()` and
/// converts each into a framed message using the same protocol as TCP.
pub async fn listen_for_tor_connections(
    rend_requests: impl futures::Stream<Item = RendRequest> + Send + 'static,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    let stream_requests = handle_rend_requests(rend_requests);
    futures::pin_mut!(stream_requests);

    while let Some(stream_request) = stream_requests.next().await {
        let tx = tx.clone();
        tokio::spawn(async move {
            match stream_request.accept(Connected::new_empty()).await {
                Ok(mut data_stream) => {
                    match crate::net::framing::receive_message(&mut data_stream).await {
                        Ok(message) => {
                            let _ = tx.send(IncomingMessage {
                                message,
                                remote_addr: "tor-rendezvous".to_string(),
                            }).await;
                        }
                        Err(e) => {
                            eprintln!("Tor connection framing error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to accept Tor stream: {}", e);
                }
            }
        });
    }

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
