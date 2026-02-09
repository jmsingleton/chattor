use crate::error::Result;
use crate::config::Settings;
use crate::db::Database;
use crate::crypto::IdentityKeypair;
use crate::tor::client::TorClient;
use crate::tor::hidden_service::HiddenService;
use crate::net::queue::MessageQueue;
use std::fs;
use std::sync::Arc;

pub struct App {
    pub settings: Settings,
    pub db: Database,
    pub identity: IdentityKeypair,
    pub tor_client: Option<Arc<TorClient>>,
    pub hidden_service: Option<HiddenService>,
    pub message_queue: MessageQueue,
    pub onion_address: Option<String>,
}

impl App {
    pub fn new() -> Result<Self> {
        // Load settings
        let settings = Settings::default()?;

        // Ensure directories exist
        fs::create_dir_all(&settings.config_dir)?;
        fs::create_dir_all(&settings.data_dir)?;

        // Open database
        let db = Database::open(&settings.db_path)?;

        // Generate or load identity
        // TODO: In future, load from database if exists
        let identity = IdentityKeypair::generate()?;

        // Initialize Phase 2 components
        let message_queue = MessageQueue::new();
        let tor_client = None; // Will be initialized when Tor is enabled
        let hidden_service = None;
        let onion_address = None;

        Ok(App {
            settings,
            db,
            identity,
            tor_client,
            hidden_service,
            message_queue,
            onion_address,
        })
    }

    /// Initialize Tor client
    pub async fn init_tor(&mut self) -> Result<()> {
        if self.tor_client.is_some() {
            return Ok(()); // Already initialized
        }

        let client = TorClient::new().await?;
        self.tor_client = Some(Arc::new(client));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_app_creation_with_temp_dirs() {
        let temp_dir = TempDir::new().unwrap();

        // Override HOME for test
        std::env::set_var("HOME", temp_dir.path());

        let app = App::new();
        assert!(app.is_ok());
    }

    #[test]
    fn test_app_has_phase2_components() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let app = App::new().unwrap();

        // Verify Phase 2 components exist
        assert!(app.tor_client.is_none()); // Not initialized by default
        assert!(app.hidden_service.is_none());
        assert!(app.onion_address.is_none());
        // message_queue exists (can't easily test without calling methods)
    }

    #[tokio::test]
    async fn test_init_tor() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let mut app = App::new().unwrap();

        // Initialize Tor
        let result = app.init_tor().await;
        assert!(result.is_ok());
        assert!(app.tor_client.is_some());

        // Calling again should be no-op
        let result2 = app.init_tor().await;
        assert!(result2.is_ok());
    }
}
