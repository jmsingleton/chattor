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
pub async fn run(settings: Settings, passphrase_fd: Option<i32>) -> Result<()> {
    // Derive encryption key
    std::fs::create_dir_all(&settings.data_dir)?;
    let salt_path = settings.data_dir.join("db.salt");
    let salt = crate::db::encryption::load_or_create_salt(&salt_path)?;

    let passphrase = zeroize::Zeroizing::new(match passphrase_fd {
        Some(fd) => read_passphrase_from_fd(fd)?,
        None => rpassword::prompt_password_stderr("Database passphrase: ")
            .map_err(crate::error::ChattorError::Io)?,
    });
    if passphrase.trim().is_empty() {
        return Err(crate::error::ChattorError::Crypto(
            "Passphrase cannot be empty".into(),
        ));
    }
    if passphrase.trim().len() < 8 {
        return Err(crate::error::ChattorError::Crypto(
            "Passphrase must be at least 8 characters".into(),
        ));
    }

    let db_key = zeroize::Zeroizing::new(
        if crate::db::encryption::is_unencrypted(&settings.db_path) {
            let key = crate::db::encryption::derive_key(passphrase.as_bytes(), &salt)?;
            eprintln!("Migrating existing database to encrypted format...");
            let tmp_path = settings.db_path.with_extension("db.enc");
            crate::db::Database::migrate_to_encrypted(&settings.db_path, &tmp_path, &key)?;
            // Verify the new encrypted DB is readable before replacing the original
            crate::db::Database::open_encrypted(&tmp_path, &key)?;
            let backup_path = settings.db_path.with_extension("db.bak");
            std::fs::rename(&settings.db_path, &backup_path)
                .map_err(crate::error::ChattorError::Io)?;
            std::fs::rename(&tmp_path, &settings.db_path)
                .map_err(crate::error::ChattorError::Io)?;
            eprintln!(
                "Database migration complete (backup at {}).",
                backup_path.display()
            );
            key
        } else {
            crate::db::encryption::derive_key(passphrase.as_bytes(), &salt)?
        },
    );

    let app = Arc::new(Mutex::new(App::new_with_settings(
        settings.clone(),
        Some(&db_key),
    )?));

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
        std::fs::write(&token_path, &token_hex).map_err(crate::error::ChattorError::Io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))
                .map_err(crate::error::ChattorError::Io)?;
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

fn read_passphrase_from_fd(fd: i32) -> Result<String> {
    if fd < 3 {
        return Err(crate::error::ChattorError::Crypto(
            "passphrase-fd must be >= 3 (cannot use stdin/stdout/stderr)".into(),
        ));
    }
    use std::io::Read;
    use std::os::unix::io::FromRawFd;
    // SAFETY: caller guarantees fd is a valid, open file descriptor >= 3.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut passphrase = String::new();
    file.read_to_string(&mut passphrase)
        .map_err(crate::error::ChattorError::Io)?;
    Ok(passphrase.trim().to_string())
}
