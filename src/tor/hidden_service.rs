use crate::error::{Result, TorrentChatError};
use crate::tor::client::TorClient;
use crate::crypto::IdentityKeypair;
use std::net::SocketAddr;

/// Tor hidden service for receiving connections
pub struct HiddenService {
    onion_address: String,
    local_addr: SocketAddr,
}

impl HiddenService {
    /// Create hidden service with identity
    pub async fn new(
        tor_client: &TorClient,
        identity: &IdentityKeypair,
        port: u16,
    ) -> Result<Self> {
        let onion_address = identity.to_onion_address();

        // For MVP, we'll use arti's client-side connections
        // Full hidden service hosting requires arti's onion service APIs
        // which are still experimental. For local testing, we can use
        // localhost forwarding.

        let local_addr: SocketAddr = format!("127.0.0.1:{}", port)
            .parse()
            .map_err(|e| TorrentChatError::Network(format!("Invalid port: {}", e)))?;

        Ok(HiddenService {
            onion_address,
            local_addr,
        })
    }

    /// Get .onion address
    pub fn address(&self) -> &str {
        &self.onion_address
    }

    /// Get local listening address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop hidden service (placeholder)
    pub fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_hidden_service_persistent_address() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        // First launch - generate identity
        let identity1 = crate::crypto::IdentityKeypair::load_or_generate(&db).unwrap();
        let onion1 = identity1.to_onion_address();

        // Second launch - load same identity
        let identity2 = crate::crypto::IdentityKeypair::load_or_generate(&db).unwrap();
        let onion2 = identity2.to_onion_address();

        assert_eq!(onion1, onion2);
    }

    #[tokio::test]
    async fn test_service_creation() {
        let tor_client = crate::tor::client::TorClient::new().await.unwrap();
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let service = HiddenService::new(&tor_client, &identity, 8080).await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_service_address() {
        let tor_client = crate::tor::client::TorClient::new().await.unwrap();
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let service = HiddenService::new(&tor_client, &identity, 8080).await.unwrap();
        assert!(service.address().ends_with(".onion"));
    }

    #[tokio::test]
    async fn test_service_stop() {
        let tor_client = crate::tor::client::TorClient::new().await.unwrap();
        let identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let mut service = HiddenService::new(&tor_client, &identity, 8080).await.unwrap();
        let result = service.stop();
        assert!(result.is_ok());
    }
}
