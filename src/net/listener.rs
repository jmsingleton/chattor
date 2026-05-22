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

/// Maximum time a single rendezvous accept may spend reading the framed
/// envelope before we abandon it. Closes a slow-loris vector: without a
/// timeout an attacker could dribble bytes (or nothing at all) on each
/// stream and park every semaphore permit indefinitely, starving
/// well-behaved peers. 30s is generous — even a slow Tor circuit usually
/// delivers our small JSON envelopes well inside that.
const RENDEZVOUS_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);


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

            match stream_request.accept(Connected::new_empty()).await {
                Ok(mut data_stream) => {
                    // Bound the time spent reading the framed envelope.
                    // tokio::time::timeout drops the read future on
                    // expiry, releasing the semaphore permit (via the
                    // outer task exit) and discarding the stream.
                    let read_fut = crate::net::framing::receive_message(&mut data_stream);
                    match tokio::time::timeout(RENDEZVOUS_READ_TIMEOUT, read_fut).await {
                        Ok(Ok(envelope)) => {
                            let _ = tx.send(IncomingMessage {
                                message: envelope.payload,
                                remote_addr: "tor-rendezvous".to_string(),
                            }).await;
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(error = %e, "tor connection framing error");
                        }
                        Err(_elapsed) => {
                            tracing::warn!(
                                "tor rendezvous read exceeded {:?}, dropping stream",
                                RENDEZVOUS_READ_TIMEOUT,
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to accept tor stream");
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
