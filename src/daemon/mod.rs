pub mod event_loop;
pub mod pid;
pub mod rpc;
pub mod socket;
pub mod tasks;

use crate::app::App;
use crate::config::Settings;
use crate::error::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

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

    // Start Unix socket server
    let socket_path = settings.data_dir.join("chattor.sock");
    let _socket_handle = socket::start(&socket_path, Arc::clone(&app), presence_map.clone()).await;
    eprintln!("Listening on {}", socket_path.display());

    // Run event loop (blocks until shutdown signal)
    let result = event_loop::run(Arc::clone(&app), presence_map).await;

    // Cleanup socket and PID file
    std::fs::remove_file(&socket_path).ok();
    pid::release(&pid_path);
    result
}
