pub mod event_loop;
pub mod pid;
pub mod rpc;
pub mod socket;
pub mod tasks;

use crate::app::App;
use crate::config::Settings;
use crate::error::Result;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// Start the daemon: bootstrap Tor, spawn background tasks, run event loop.
pub async fn run(settings: Settings) -> Result<()> {
    let app = Arc::new(Mutex::new(App::new_with_settings(settings.clone())?));

    // Ensure identity exists
    {
        let mut app_lock = app.lock().await;
        if app_lock.identity.is_none() {
            let identity = crate::crypto::IdentityKeypair::generate()?;
            identity.save_to_db(&app_lock.db)?;
            app_lock.identity = Some(identity);
        }
    }

    // PID file
    let pid_path = settings.data_dir.join("chattor.pid");
    pid::acquire(&pid_path)?;

    // Generate auth token for RPC authentication
    let token_path = settings.data_dir.join("daemon.token");
    let auth_token = {
        use rand::Rng;
        let token: [u8; 32] = rand::thread_rng().gen();
        let token_hex = hex::encode(token);
        std::fs::write(&token_path, &token_hex)
            .map_err(|e| crate::error::ChattorError::Io(e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| crate::error::ChattorError::Io(e))?;
        }
        token_hex
    };

    // Bootstrap Tor
    eprintln!("Bootstrapping Tor...");
    {
        let mut app_lock = app.lock().await;
        app_lock.init_tor().await?;
    }

    let onion = {
        let app_lock = app.lock().await;
        app_lock.onion_address.clone().unwrap_or_default()
    };
    eprintln!("Tor ready: {}", onion);

    // Pool watch channel for heartbeat
    let (pool_tx, pool_rx) = tokio::sync::watch::channel(None);
    {
        let app_lock = app.lock().await;
        if let Some(ref pool) = app_lock.connection_pool {
            let _ = pool_tx.send(Some(Arc::clone(pool)));
        }
    }

    // Spawn background tasks
    tasks::spawn_all(Arc::clone(&app), pool_rx).await;

    // Create presence map (shared between socket server and event loop)
    let presence_map = crate::presence::new_presence_map();

    // Broadcast channel for streaming incoming message events to listen clients
    let (msg_broadcast_tx, _) = broadcast::channel::<String>(100);

    // Start Unix socket server
    let socket_path = settings.data_dir.join("chattor.sock");
    let _socket_handle = socket::start(
        &socket_path,
        Arc::clone(&app),
        presence_map.clone(),
        msg_broadcast_tx.clone(),
        auth_token,
    )
    .await;
    eprintln!("Listening on {}", socket_path.display());

    // Run event loop (blocks until shutdown signal)
    let result = event_loop::run(Arc::clone(&app), presence_map, msg_broadcast_tx).await;

    // Cleanup socket, token, and PID file
    std::fs::remove_file(&socket_path).ok();
    std::fs::remove_file(&token_path).ok();
    pid::release(&pid_path);
    result
}
