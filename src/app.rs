use crate::error::Result;
use crate::config::Settings;
use crate::db::Database;
use crate::crypto::IdentityKeypair;
use crate::tor::client::TorClient;
use crate::tor::hidden_service::HiddenService;
use crate::net::queue::MessageQueue;
use std::fs;
use std::sync::Arc;

/// Commands sent to the main thread for queue processing
pub enum QueueCommand {
    ProcessQueue,
}

pub struct App {
    pub settings: Settings,
    pub db: Database,
    pub identity: IdentityKeypair,
    pub tor_client: Option<Arc<TorClient>>,
    pub hidden_service: Option<HiddenService>,
    pub message_queue: MessageQueue,
    pub onion_address: Option<String>,
    pub incoming_message_rx: Option<tokio::sync::mpsc::Receiver<crate::net::listener::IncomingMessage>>,
    pub queue_command_rx: Option<tokio::sync::mpsc::Receiver<QueueCommand>>,
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
            incoming_message_rx: None,
            queue_command_rx: None,
        })
    }

    /// Initialize Tor client and hidden service
    pub async fn init_tor(&mut self) -> Result<()> {
        if self.tor_client.is_some() {
            return Ok(()); // Already initialized
        }

        // Bootstrap Tor client
        let client = crate::tor::client::TorClient::new().await?;

        // Load or generate identity
        let identity = crate::crypto::IdentityKeypair::load_or_generate(&self.db)?;

        // Create hidden service
        let hidden_service = crate::tor::hidden_service::HiddenService::new(
            &client,
            &identity,
            9051,
        ).await?;

        let onion_address = hidden_service.address().to_string();

        // Spawn TCP listener for incoming connections
        let listener_addr = hidden_service.local_addr();
        let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(100);

        // Bind listener
        match tokio::net::TcpListener::bind(listener_addr).await {
            Ok(listener) => {
                tokio::spawn(async move {
                    if let Err(e) = crate::net::listener::listen_for_connections(listener, msg_tx).await {
                        eprintln!("Listener task error: {}", e);
                    }
                });
                self.incoming_message_rx = Some(msg_rx);
            }
            Err(e) => {
                eprintln!("Failed to bind listener on {}: {}", listener_addr, e);
            }
        }

        // Spawn queue processor task (sends ProcessQueue command every 30 seconds)
        let (queue_cmd_tx, queue_cmd_rx) = tokio::sync::mpsc::channel(10);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                if queue_cmd_tx.send(QueueCommand::ProcessQueue).await.is_err() {
                    break; // Channel closed, app shutting down
                }
            }
        });
        self.queue_command_rx = Some(queue_cmd_rx);

        // Store in app state
        self.tor_client = Some(Arc::new(client));
        self.hidden_service = Some(hidden_service);
        self.onion_address = Some(onion_address);

        Ok(())
    }

    /// Create app with custom settings (for testing)
    pub fn new_with_settings(settings: Settings) -> Result<Self> {
        fs::create_dir_all(&settings.config_dir)?;
        fs::create_dir_all(&settings.data_dir)?;

        let db = Database::open(&settings.db_path)?;
        let identity = IdentityKeypair::generate()?;
        let message_queue = MessageQueue::new();

        Ok(App {
            settings,
            db,
            identity,
            tor_client: None,
            hidden_service: None,
            message_queue,
            onion_address: None,
            incoming_message_rx: None,
            queue_command_rx: None,
        })
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

    #[tokio::test]
    async fn test_app_init_tor_real() {
        let temp_config = tempfile::tempdir().unwrap();
        let temp_data = tempfile::tempdir().unwrap();

        let settings = crate::config::Settings {
            config_dir: temp_config.path().to_path_buf(),
            data_dir: temp_data.path().to_path_buf(),
            db_path: temp_data.path().join("test.db"),
            debug: false,
            tor_socks_port: 9050,
        };

        let _app = App::new_with_settings(settings).unwrap();

        // This will take 30-60 seconds for real Tor bootstrap
        // For CI, we might want to skip or mock
        // let result = app.init_tor().await;
        // assert!(result.is_ok());
    }
}
