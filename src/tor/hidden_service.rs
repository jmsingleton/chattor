use crate::error::{ChattorError, Result};
use crate::tor::client::TorClient;
use safelog::DisplayRedacted as _;
use std::sync::Arc;
use tor_hsservice::config::OnionServiceConfigBuilder;
use tor_hsservice::{HsNickname, RendRequest, RunningOnionService};

/// Tor hidden service for receiving connections via onion routing
pub struct HiddenService {
    onion_address: String,
    _service: Arc<RunningOnionService>,
}

impl HiddenService {
    /// Launch a real arti onion service.
    ///
    /// Returns the HiddenService handle and a stream of incoming rendezvous
    /// requests. The caller should feed the stream into `listen_for_tor_connections()`.
    pub async fn launch(
        tor_client: &TorClient,
    ) -> Result<(Self, impl futures::Stream<Item = RendRequest>)> {
        let nickname: HsNickname = "chattor"
            .parse()
            .map_err(|e| ChattorError::Tor(format!("Invalid service nickname: {}", e)))?;

        let mut builder = OnionServiceConfigBuilder::default();
        builder.nickname(nickname);
        let config = builder.build().map_err(|e| {
            ChattorError::Tor(format!("Failed to build onion service config: {}", e))
        })?;

        let (service, rend_requests) = tor_client
            .inner()
            .launch_onion_service(config)
            .map_err(|e| ChattorError::Tor(format!("Failed to launch onion service: {}", e)))?
            .ok_or_else(|| ChattorError::Tor("Onion service is disabled in config".into()))?;

        let onion_address = service
            .onion_address()
            .map(|id| id.display_unredacted().to_string())
            .unwrap_or_else(|| "pending.onion".to_string());

        Ok((
            HiddenService {
                onion_address,
                _service: service,
            },
            rend_requests,
        ))
    }

    /// Get .onion address
    pub fn address(&self) -> &str {
        &self.onion_address
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore] // Requires real Tor network connection
    async fn test_hidden_service_launch() {
        let tor_client = crate::tor::client::TorClient::new().await.unwrap();
        let result = super::HiddenService::launch(&tor_client).await;
        assert!(result.is_ok());
        let (hs, _stream) = result.unwrap();
        assert!(hs.address().contains(".onion") || hs.address() == "pending.onion");
    }
}
