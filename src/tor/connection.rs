use crate::error::{Result, TorrentChatError};
use crate::tor::client::TorClient;
use crate::protocol::message::{Message, MessageEnvelope};
use crate::net::framing::send_message;
use arti_client::DataStream;
use tracing::info;

/// Port used for chattor peer-to-peer communication over Tor
pub const CHATTOR_PORT: u16 = 9735;

/// Connection to peer over Tor
pub struct TorConnection {
    stream: DataStream,
}

impl TorConnection {
    /// Connect to peer via Tor (real DataStream)
    pub async fn connect(
        tor_client: &TorClient,
        remote_onion: &str,
    ) -> Result<Self> {
        use arti_client::StreamPrefs;

        let stream = tor_client.inner()
            .connect_with_prefs((remote_onion, CHATTOR_PORT), &StreamPrefs::default())
            .await
            .map_err(|e| TorrentChatError::Tor(format!("Failed to connect to {}: {}", remote_onion, e)))?;

        info!("Connected to {} via Tor", remote_onion);

        Ok(TorConnection { stream })
    }

    /// Send message over connection, wrapped in a versioned envelope.
    pub async fn send(&mut self, message: &Message) -> Result<()> {
        let envelope = MessageEnvelope::new(message.clone());
        send_message(&mut self.stream, &envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chattor_port_constant() {
        assert_eq!(CHATTOR_PORT, 9735);
    }

    #[tokio::test]
    #[ignore] // Requires Tor daemon
    async fn test_real_tor_connection() {
        let tor_client = crate::tor::client::TorClient::new().await.unwrap();
        let result = TorConnection::connect(&tor_client, "test.onion").await;
        assert!(result.is_err());
    }
}
