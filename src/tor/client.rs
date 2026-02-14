//! Tor Client Wrapper
//!
//! Wraps the Arti Tor client for network connections.

use crate::error::{Result, TorrentChatError};
use arti_client::{TorClient as ArtiTorClient, TorClientConfig};
use std::sync::Arc;

/// Tor client for managing connections
pub struct TorClient {
    client: Arc<ArtiTorClient<tor_rtcompat::PreferredRuntime>>,
}

impl TorClient {
    /// Create and bootstrap a new Tor client
    pub async fn new() -> Result<Self> {
        let config = TorClientConfig::default();
        let client = ArtiTorClient::create_bootstrapped(config)
            .await
            .map_err(|e| TorrentChatError::Tor(format!("Failed to bootstrap Tor: {}", e)))?;

        Ok(TorClient {
            client: Arc::new(client),
        })
    }

    /// Check if Tor client is bootstrapped
    pub fn is_bootstrapped(&self) -> bool {
        // Arti client is bootstrapped after creation
        true
    }

    /// Get reference to inner arti client
    pub fn inner(&self) -> &Arc<ArtiTorClient<tor_rtcompat::PreferredRuntime>> {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires real Tor network connection
    async fn test_tor_client_bootstrap() {
        let client = TorClient::new().await.unwrap();
        assert!(client.is_bootstrapped());
    }
}
