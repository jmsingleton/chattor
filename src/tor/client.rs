//! Tor Client Wrapper
//!
//! Wraps the Arti Tor client for network connections.

use crate::error::{Result, TorrentChatError};
use arti_client::{TorClient as ArtiTorClient, TorClientConfig};
use std::path::Path;
use std::sync::Arc;

/// Tor client for managing connections
pub struct TorClient {
    client: Arc<ArtiTorClient<tor_rtcompat::PreferredRuntime>>,
}

impl TorClient {
    /// Create and bootstrap with persistent state directory (for real usage)
    pub async fn new_with_data_dir(data_dir: &Path) -> Result<Self> {
        let state_dir = data_dir.join("arti");
        let cache_dir = data_dir.join("arti-cache");

        std::fs::create_dir_all(&state_dir).map_err(|e| {
            TorrentChatError::Tor(format!("Failed to create arti state dir: {}", e))
        })?;
        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            TorrentChatError::Tor(format!("Failed to create arti cache dir: {}", e))
        })?;

        // Arti requires 700 permissions on state/cache dirs (contains onion service keys)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(&state_dir, perms.clone()).map_err(|e| {
                TorrentChatError::Tor(format!("Failed to set arti state dir permissions: {}", e))
            })?;
            std::fs::set_permissions(&cache_dir, perms).map_err(|e| {
                TorrentChatError::Tor(format!("Failed to set arti cache dir permissions: {}", e))
            })?;
        }

        let config =
            arti_client::config::TorClientConfigBuilder::from_directories(&state_dir, &cache_dir)
                .build()
                .map_err(|e| {
                    TorrentChatError::Tor(format!("Failed to build Tor config: {}", e))
                })?;

        let client = ArtiTorClient::create_bootstrapped(config)
            .await
            .map_err(|e| TorrentChatError::Tor(format!("Failed to bootstrap Tor: {}", e)))?;

        Ok(TorClient {
            client: Arc::new(client),
        })
    }

    /// Create and bootstrap with default config (for backward compat / tests)
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
