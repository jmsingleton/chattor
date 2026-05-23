use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::Semaphore;
use std::sync::Arc;
use crate::error::Result;
use crate::protocol::message::Message;
use tor_hsservice::{handle_rend_requests, RendRequest};
use tor_cell::relaycell::msg::Connected;
use futures::StreamExt;

/// Maximum number of in-flight rendezvous accept tasks. Bounds memory and
/// CPU usage when a peer (or many peers) open connections faster than the
/// dispatcher can drain them. Picked high enough not to throttle normal
/// behaviour (5 req/s sustained × 20 peers = 100), low enough to cap blast
/// radius on flood. New rendezvous wait on the semaphore rather than being
/// dropped — Tor will apply backpressure naturally if accepts stall.
const MAX_INFLIGHT_RENDEZVOUS: usize = 256;

/// Maximum end-to-end time we'll spend on a single rendezvous from
/// `stream_request.accept` through `framing::receive_message`. Both halves
/// must finish inside this budget or we abandon the stream — bounds the
/// slow-loris vector against both `accept` (an attacker can stall the
/// stream handshake) and the framing read (dribble bytes forever). 30s
/// is generous given the tiny JSON envelopes we transfer.
const RENDEZVOUS_ACCEPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);


/// Message received from peer
pub struct IncomingMessage {
    pub message: Message,
    #[allow(dead_code)]
    pub remote_addr: String,
}

/// Listen for incoming TCP connections
#[allow(dead_code)]
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
                        tracing::warn!(error = %e, "connection handler error");
                    }
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to accept connection");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Handle single incoming connection
#[allow(dead_code)]
async fn handle_connection(
    mut stream: TcpStream,
    remote_addr: String,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    // Use shared framing module instead of duplicating read logic
    let envelope = crate::net::framing::receive_message(&mut stream).await?;

    // Send to app (unwrap envelope payload)
    tx.send(IncomingMessage { message: envelope.payload, remote_addr }).await
        .map_err(|e| crate::error::ChattorError::Network(format!("Failed to send to app: {}", e)))?;

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

    // Per-process cap on concurrent accept tasks. Combined with the per-peer
    // rate limiter at the dispatcher, this gates global resource use even
    // when a single peer (or many peers) open streams in bursts.
    let semaphore = Arc::new(Semaphore::new(MAX_INFLIGHT_RENDEZVOUS));

    while let Some(stream_request) = stream_requests.next().await {
        // Wait for a permit before accepting. Tor applies backpressure
        // naturally when accepts stall — this is preferable to dropping
        // rendezvous requests outright.
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                // Semaphore closed — this only happens if we explicitly close
                // it, which we don't. Treat as a hard listener error.
                tracing::error!("rendezvous semaphore closed unexpectedly");
                return Ok(());
            }
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            // Permit is dropped when this task exits, freeing the slot.
            let _permit = permit;

            // Wrap both halves — stream acceptance AND the framing read —
            // in a single outer timeout. Splitting them would leave the
            // accept side unbounded; an attacker can stall there before
            // the read-side timer ever starts.
            let pipeline = async {
                let mut data_stream = stream_request
                    .accept(Connected::new_empty())
                    .await
                    .map_err(|e| crate::error::ChattorError::Tor(
                        format!("accept failed: {}", e)
                    ))?;
                crate::net::framing::receive_message(&mut data_stream).await
            };
            match tokio::time::timeout(RENDEZVOUS_ACCEPT_TIMEOUT, pipeline).await {
                Ok(Ok(envelope)) => {
                    let _ = tx.send(IncomingMessage {
                        message: envelope.payload,
                        remote_addr: "tor-rendezvous".to_string(),
                    }).await;
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "tor rendezvous error");
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        "tor rendezvous exceeded {:?}, dropping stream",
                        RENDEZVOUS_ACCEPT_TIMEOUT,
                    );
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
        use crate::protocol::message::{Message, MessageEnvelope, DeliveryReceiptMessage};
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

        // Create a test message wrapped in an envelope
        let msg = Message::DeliveryReceipt(DeliveryReceiptMessage {
            message_id: Uuid::new_v4(),
            timestamp: 1234567890,
        });
        let envelope = MessageEnvelope::new(msg);

        let json = serde_json::to_vec(&envelope).unwrap();
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
