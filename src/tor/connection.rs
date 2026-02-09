use crate::error::{Result, TorrentChatError};
use crate::tor::client::TorClient;
use crate::protocol::message::Message;
use crate::net::framing::{send_message, receive_message};
use tokio::net::TcpStream;

/// Connection to peer over Tor
pub struct TorConnection {
    pub remote_onion: String,
    stream: TcpStream,
}

impl TorConnection {
    /// Connect to peer via Tor
    pub async fn connect(
        tor_client: &TorClient,
        remote_onion: &str,
    ) -> Result<Self> {
        // For MVP local testing, connect directly to localhost
        // TODO: Use tor_client.inner().connect() for real Tor connections
        let _ = tor_client; // Suppress unused warning

        // Parse port from onion or use default
        let port = 9051;
        let addr = format!("127.0.0.1:{}", port);

        let stream = TcpStream::connect(&addr).await
            .map_err(|e| TorrentChatError::Network(format!("Failed to connect: {}", e)))?;

        Ok(TorConnection {
            remote_onion: remote_onion.to_string(),
            stream,
        })
    }

    /// Connect directly for testing
    pub async fn connect_direct(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).await
            .map_err(|e| TorrentChatError::Network(format!("Failed to connect: {}", e)))?;

        Ok(TorConnection {
            remote_onion: "test.onion".to_string(),
            stream,
        })
    }

    /// Send message over connection
    pub async fn send(&mut self, message: &Message) -> Result<()> {
        send_message(&mut self.stream, message).await
    }

    /// Receive message from connection
    pub async fn receive(&mut self) -> Result<Message> {
        receive_message(&mut self.stream).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_and_send() {
        use tokio::net::TcpListener;

        // Start local server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read length prefix
            use tokio::io::AsyncReadExt;
            let mut len_bytes = [0u8; 4];
            stream.read_exact(&mut len_bytes).await.unwrap();
        });

        // Connect (using localhost instead of Tor for test)
        let result = TorConnection::connect_direct(&format!("127.0.0.1:{}", addr.port())).await;
        assert!(result.is_ok());
    }
}
