mod app;
mod cli;
mod client;
mod config;
mod crypto;
mod daemon;
mod db;
mod error;
mod handlers;
mod mcp;
mod net;
mod notifications;
mod presence;
mod protocol;
mod tor;
mod ui;

use clap::Parser;
use cli::{Cli, Command};
use error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Build settings with optional directory overrides
    let mut settings = config::Settings::default()?;
    if let Some(ref dir) = cli.config_dir {
        settings.config_dir = std::path::PathBuf::from(dir);
    }
    if let Some(ref dir) = cli.data_dir {
        settings.data_dir = std::path::PathBuf::from(dir);
        settings.db_path = settings.data_dir.join("messages.db");
    }
    settings.debug = cli.debug;

    match cli.command {
        // No subcommand or explicit `tui` — run the interactive TUI
        None => run_tui(settings, None).await,
        Some(Command::Tui { theme }) => run_tui(settings, theme).await,

        // Headless daemon
        Some(Command::Daemon) => daemon::run(settings).await,

        // CLI client commands — talk to the daemon via Unix socket
        Some(Command::Status) => {
            let result =
                client::rpc_call(&settings.data_dir, "status", serde_json::json!({})).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Identity) => {
            let result =
                client::rpc_call(&settings.data_dir, "identity", serde_json::json!({})).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Friends { action }) => {
            use cli::FriendsAction;
            let (method, params) = match action {
                FriendsAction::List => ("friends_list", serde_json::json!({})),
                FriendsAction::Add { code } => ("friends_add", serde_json::json!({ "code": code })),
                FriendsAction::Remove { onion } => {
                    ("friends_remove", serde_json::json!({ "onion": onion }))
                }
                FriendsAction::Requests => ("friends_requests", serde_json::json!({})),
                FriendsAction::Accept { id } => ("friends_accept", serde_json::json!({ "id": id })),
                FriendsAction::Reject { id } => ("friends_reject", serde_json::json!({ "id": id })),
            };
            let result = client::rpc_call(&settings.data_dir, method, params).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Send { peer, message }) => {
            let result = client::rpc_call(
                &settings.data_dir,
                "send_message",
                serde_json::json!({ "peer": peer, "message": message }),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Recv { peer }) => {
            let result = client::rpc_call(
                &settings.data_dir,
                "recv_messages",
                serde_json::json!({ "peer": peer }),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Listen) => {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

            let socket_path = settings.data_dir.join("chattor.sock");
            let stream = tokio::net::UnixStream::connect(&socket_path)
                .await
                .map_err(|_| {
                    error::ChattorError::Network(
                        "Cannot connect to daemon. Is it running? Start with: chattor daemon"
                            .into(),
                    )
                })?;
            let (reader, mut writer) = stream.into_split();
            let request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "listen",
                "params": {}
            });
            let line = format!("{}\n", serde_json::to_string(&request).unwrap());
            writer.write_all(line.as_bytes()).await?;
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{}", line);
            }
            Ok(())
        }
        Some(Command::Channels { action }) => {
            use cli::ChannelsAction;
            let (method, params) = match action {
                ChannelsAction::List => ("channels_list", serde_json::json!({})),
                ChannelsAction::Publish {
                    channel_type,
                    message,
                } => (
                    "channels_publish",
                    serde_json::json!({ "channel_type": channel_type, "message": message }),
                ),
                ChannelsAction::Subscribe { onion } => {
                    ("channels_subscribe", serde_json::json!({ "onion": onion }))
                }
                ChannelsAction::Feed { channel } => {
                    ("channels_feed", serde_json::json!({ "channel_id": channel }))
                }
            };
            let result = client::rpc_call(&settings.data_dir, method, params).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Ephemeral { peer, ttl }) => {
            let result = client::rpc_call(
                &settings.data_dir,
                "ephemeral_set",
                serde_json::json!({ "peer": peer, "ttl": ttl }),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Notifications { state }) => {
            let result = client::rpc_call(
                &settings.data_dir,
                "notifications_toggle",
                serde_json::json!({ "enabled": state == "on" }),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Mcp) => mcp::server::run(settings.data_dir).await,
    }
}

/// Run the interactive TUI application.
///
/// This contains all the original main() logic: terminal setup, Tor bootstrap
/// animation, identity generation, background tasks, and the main event loop.
async fn run_tui(settings: config::Settings, theme_name: Option<String>) -> Result<()> {
    use crate::crypto::IdentityKeypair;
    use app::App;
    use base64::Engine;
    use crossterm::{
        event::{self, DisableMouseCapture, EnableMouseCapture, Event},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use handlers::channels::collect_sync_requests;
    use handlers::friend_request::{
        handle_accept_friend_request, handle_reject_friend_request, handle_send_friend_request,
        SendResult,
    };
    use handlers::messaging::{handle_incoming_message, process_message_queue, try_send_direct};
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::io;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use ui::{AppAction, AppState, RenderContext};

    let app = Arc::new(Mutex::new(App::new_with_settings(settings)?));

    // Watch channel to broadcast connection pool to background tasks
    let (pool_tx, pool_rx) =
        tokio::sync::watch::channel::<Option<Arc<crate::net::pool::ConnectionPool>>>(None);

    // Load theme
    let theme = {
        let app_lock = app.lock().await;
        let config_path = app_lock.settings.config_dir.join("theme.toml");
        drop(app_lock);
        ui::theme::load_theme(theme_name.as_deref(), &config_path)
    };

    // Set up terminal FIRST so we can render immediately
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- First-Run Identity Generation ---
    let needs_identity = {
        let app_lock = app.lock().await;
        app_lock.identity.is_none()
    };

    if needs_identity {
        let identity = IdentityKeypair::generate()?;
        let mut app_lock = app.lock().await;
        identity.save_to_db(&app_lock.db)?;
        app_lock.identity = Some(identity);
        drop(app_lock);
    }

    // --- Bootstrap Phase ---
    // Create watch channel for Tor init progress
    let (bootstrap_tx, mut bootstrap_rx) =
        tokio::sync::watch::channel(ui::BootstrapUpdate::Progress(0));

    // Spawn Tor init in background, communicating via watch channel
    let app_tor = Arc::clone(&app);
    let pool_tx_init = pool_tx.clone();
    tokio::spawn(async move {
        let mut app_lock = app_tor.lock().await;
        match app_lock.init_tor().await {
            Ok(()) => {
                // Broadcast pool to background tasks
                if let Some(ref pool) = app_lock.connection_pool {
                    let _ = pool_tx_init.send(Some(Arc::clone(pool)));
                }
                let _ = bootstrap_tx.send(ui::BootstrapUpdate::Connected);
            }
            Err(e) => {
                let _ = bootstrap_tx.send(ui::BootstrapUpdate::Failed(format!("{}", e)));
            }
        }
    });

    // Run bootstrap animation loop
    let mut continued_offline = false;
    let mut phase = ui::BootstrapPhase::new();
    let bootstrap_start = std::time::Instant::now();
    let bootstrap_timeout = std::time::Duration::from_secs(60);

    loop {
        // Render current bootstrap frame
        match &phase {
            ui::BootstrapPhase::Connecting {
                frame,
                tick,
                progress,
            } => {
                let f = *frame;
                let t = *tick;
                let p = *progress;
                terminal.draw(|fr| {
                    ui::render_connecting(fr, f, t, p, &theme);
                })?;
            }
            ui::BootstrapPhase::Failed { ref error, .. } => {
                let err = error.clone();
                terminal.draw(|fr| {
                    ui::render_failure(fr, &err, &theme);
                })?;
            }
            ui::BootstrapPhase::Done => {
                break;
            }
        }

        // Check for timeout (only during connecting)
        if matches!(phase, ui::BootstrapPhase::Connecting { .. })
            && bootstrap_start.elapsed() > bootstrap_timeout
        {
            phase.fail("connection timed out after 60 seconds".to_string());
            continue;
        }

        // Check for updates from Tor init task
        if bootstrap_rx.has_changed().unwrap_or(true) {
            let update = bootstrap_rx.borrow_and_update().clone();
            match update {
                ui::BootstrapUpdate::Progress(p) => {
                    phase.set_progress(p);
                }
                ui::BootstrapUpdate::Connected => {
                    phase.done();
                    continue;
                }
                ui::BootstrapUpdate::Failed(e) => {
                    phase.fail(e);
                }
            }
        }

        // Handle key events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = ui::handle_bootstrap_key(&phase, key) {
                    match action {
                        ui::BootstrapAction::Quit => {
                            // Clean up and exit
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                LeaveAlternateScreen,
                                DisableMouseCapture
                            )?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        ui::BootstrapAction::ContinueOffline => {
                            continued_offline = true;
                            break;
                        }
                        ui::BootstrapAction::Retry => {
                            phase = ui::BootstrapPhase::new();
                            let (new_tx, new_rx) =
                                tokio::sync::watch::channel(ui::BootstrapUpdate::Progress(0));
                            bootstrap_rx = new_rx;
                            let app_retry = Arc::clone(&app);
                            let pool_tx_retry = pool_tx.clone();
                            tokio::spawn(async move {
                                let mut app_lock = app_retry.lock().await;
                                match app_lock.init_tor().await {
                                    Ok(()) => {
                                        // Broadcast pool to background tasks
                                        if let Some(ref pool) = app_lock.connection_pool {
                                            let _ = pool_tx_retry.send(Some(Arc::clone(pool)));
                                        }
                                        let _ = new_tx.send(ui::BootstrapUpdate::Connected);
                                    }
                                    Err(e) => {
                                        let _ = new_tx
                                            .send(ui::BootstrapUpdate::Failed(format!("{}", e)));
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }

        // Advance animation tick
        phase.advance_tick();
    }

    // --- Main App Phase ---
    // Spawn periodic channel sync task (every 5 minutes)
    // Queues sync requests in the message queue for delivery by the queue processor
    let app_sync = Arc::clone(&app);
    tokio::spawn(async move {
        // Initial sync after 10 seconds
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        loop {
            {
                let app_lock = app_sync.lock().await;
                if let Ok(requests) = collect_sync_requests(&app_lock) {
                    for (peer_onion, sync_msg) in requests {
                        app_lock
                            .message_queue
                            .enqueue(&app_lock.db, &peer_onion, &sync_msg, "low")
                            .ok();
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        }
    });

    // Spawn heartbeat task — sends presence updates to connected peers
    // Uses watch channel for pool access (no app lock needed during sends)
    let pool_rx_heartbeat = pool_rx.clone();
    let app_heartbeat = Arc::clone(&app);
    tokio::spawn(async move {
        // Wait for Tor to initialize before starting heartbeats
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        // Capture own_onion once (static after Tor init)
        let own_onion = {
            let app_lock = app_heartbeat.lock().await;
            app_lock.onion_address.clone().unwrap_or_default()
        };

        loop {
            // Get pool from watch channel (no app lock needed)
            let pool = {
                let rx = pool_rx_heartbeat.borrow();
                rx.clone()
            };

            if let Some(pool) = pool {
                let peers = pool.connected_peers();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                // Send heartbeats concurrently via JoinSet (no app lock held)
                let mut tasks = tokio::task::JoinSet::new();
                for peer in peers {
                    let msg = crate::protocol::message::Message::Presence(
                        crate::protocol::message::PresenceMessage {
                            from_onion: own_onion.clone(),
                            presence_type: crate::protocol::message::PresenceType::Heartbeat,
                            timestamp: now,
                        },
                    );
                    let pool = Arc::clone(&pool);
                    tasks.spawn(async move {
                        let _ = pool.send(&peer, &msg).await;
                    });
                }
                // Wait for all sends to complete
                while tasks.join_next().await.is_some() {}
            }

            tokio::time::sleep(crate::presence::HEARTBEAT_INTERVAL).await;
        }
    });

    // Initialize state machine
    let mut app_state = AppState::default();

    // Initialize presence tracker (in-memory only)
    let presence_map = presence::new_presence_map();

    let mut last_typing_sent: Option<std::time::Instant> = None;
    let mut was_typing = false;
    let mut status_flash: Option<(std::time::Instant, String)> = None;

    let mut dirty = true; // Start dirty to force initial render
    let mut last_cleanup = std::time::Instant::now();
    let cleanup_interval = std::time::Duration::from_secs(30);
    let mut presence_tick = std::time::Instant::now();
    let presence_tick_interval = std::time::Duration::from_secs(1);

    // Cached render data (persists across non-dirty frames)
    let mut cached_friends: Vec<db::queries::FriendEntry> = Vec::new();
    let mut cached_pending_count: i64 = 0;
    let mut cached_own_onion: Option<String> = None;
    let mut cached_friend_code: Option<String> = None;
    let mut cached_tor_connected: bool = false;
    let mut cached_messages: Vec<db::queries::ChatMessage> = Vec::new();
    let mut cached_conversation_ttl: Option<i64> = None;
    let mut cached_channel_subs: Vec<db::queries::ChannelSubscription> = Vec::new();
    let mut cached_channel_posts: Vec<db::queries::ChannelPost> = Vec::new();
    let mut cached_read_counts: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    let mut cached_friend_count: usize = 0;

    // Main event loop
    let result = loop {
        // Periodic presence tick (1s) for typing indicator expiry
        if presence_tick.elapsed() > presence_tick_interval {
            dirty = true;
            presence_tick = std::time::Instant::now();
        }

        // Expire status flash
        if status_flash
            .as_ref()
            .is_some_and(|(t, _)| t.elapsed() >= std::time::Duration::from_secs(2))
        {
            status_flash = None;
            dirty = true;
        }

        // Only re-query DB when state has changed
        if dirty {
            let app_lock = app.lock().await;

            cached_friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
            cached_pending_count =
                db::queries::get_pending_request_count(&app_lock.db).unwrap_or(0);
            cached_own_onion = app_lock.onion_address.clone();
            cached_friend_code = cached_own_onion
                .as_deref()
                .and_then(|o| crate::tor::address::onion_to_friend_code(o).ok());
            cached_tor_connected = app_lock.tor_client.is_some();

            // Throttle cleanup to every 30s instead of every 100ms
            if last_cleanup.elapsed() > cleanup_interval {
                db::queries::cleanup_expired_messages(&app_lock.db).ok();
                last_cleanup = std::time::Instant::now();
            }

            // Load messages and ephemeral TTL for selected conversation
            let (messages, conversation_ephemeral_ttl) = if let AppState::Normal {
                conversation_id: Some(conv_id),
                ..
            } = &app_state
            {
                let msgs =
                    db::queries::get_messages(&app_lock.db, *conv_id, 100, 0).unwrap_or_default();
                let ttl = db::queries::get_conversation_ephemeral_ttl(&app_lock.db, *conv_id)
                    .unwrap_or(None);
                (msgs, ttl)
            } else {
                (Vec::new(), None)
            };
            cached_messages = messages;
            cached_conversation_ttl = conversation_ephemeral_ttl;

            // Load channel data
            cached_channel_subs =
                db::queries::get_channel_subscriptions(&app_lock.db).unwrap_or_default();

            let (channel_posts, channel_post_read_counts) = if let AppState::ViewingChannel {
                ref channel_type,
                is_own,
                ..
            } = &app_state
            {
                let channel_id = if *is_own {
                    if channel_type == "public" {
                        1
                    } else {
                        2
                    }
                } else {
                    0 // remote posts stored with channel_id 0
                };
                let posts = db::queries::get_channel_posts(&app_lock.db, channel_id, 100)
                    .unwrap_or_default();
                let mut counts = std::collections::HashMap::new();
                if *is_own && !posts.is_empty() {
                    let post_ids: Vec<&str> = posts.iter().map(|p| p.post_id.as_str()).collect();
                    counts =
                        db::queries::get_channel_post_read_counts_batch(&app_lock.db, &post_ids)
                            .unwrap_or_default();
                }
                (posts, counts)
            } else {
                (Vec::new(), std::collections::HashMap::new())
            };
            cached_channel_posts = channel_posts;
            cached_read_counts = channel_post_read_counts;

            cached_friend_count = cached_friends.len();

            // Release lock before rendering
            drop(app_lock);
            dirty = false;
        }

        let presence_snapshot = presence::get_presence_snapshot(&presence_map).await;

        let ctx = RenderContext {
            friends: cached_friends.clone(),
            messages: cached_messages.clone(),
            own_onion: cached_own_onion.clone(),
            friend_code: cached_friend_code.clone(),
            tor_connected: cached_tor_connected,
            pending_request_count: cached_pending_count,
            conversation_ephemeral_ttl: cached_conversation_ttl,
            channel_subscriptions: cached_channel_subs.clone(),
            channel_posts: cached_channel_posts.clone(),
            channel_post_read_counts: cached_read_counts.clone(),
            theme: theme.clone(),
            presence: presence_snapshot,
            status_flash: status_flash
                .as_ref()
                .filter(|(t, _)| t.elapsed() < std::time::Duration::from_secs(2))
                .map(|(_, msg)| msg.clone()),
            continued_offline,
        };

        // Render current state
        if let Err(e) = terminal.draw(|f| {
            ui::render_app(f, &app_state, &ctx);
        }) {
            break Err(e.into());
        }

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    dirty = true; // Any key press invalidates cached state
                    let was_setting_ephemeral =
                        matches!(app_state, AppState::SettingEphemeral { .. });
                    match app_state.handle_key(key, cached_friend_count)? {
                        Some(AppAction::SendFriendRequest(code)) => {
                            let app_lock = app.lock().await;

                            match handle_send_friend_request(&app_lock, &code).await {
                                Ok(SendResult::SentImmediately) => {
                                    app_state = AppState::default();
                                }
                                Ok(SendResult::Queued) => {
                                    // Show queued status briefly, then return to normal
                                    app_state = AppState::default();
                                }
                                Err(e) => {
                                    app_state = AppState::AddingFriend {
                                        input: code,
                                        cursor: 0,
                                        error: Some(format!("Failed: {}", e)),
                                    };
                                }
                            }
                            drop(app_lock);
                        }
                        Some(AppAction::AcceptFriendRequest(id)) => {
                            let return_to_list =
                                matches!(app_state, AppState::ViewingFriendRequests { .. });
                            let app_lock = app.lock().await;

                            match handle_accept_friend_request(&app_lock, id) {
                                Ok(_) => {
                                    status_flash = Some((
                                        std::time::Instant::now(),
                                        "Friend request accepted".to_string(),
                                    ));
                                }
                                Err(e) => {
                                    status_flash = Some((
                                        std::time::Instant::now(),
                                        crate::ui::error::format_error_for_user(&e),
                                    ));
                                }
                            }

                            if return_to_list {
                                let requests =
                                    db::queries::get_pending_friend_requests(&app_lock.db)
                                        .unwrap_or_default();
                                if requests.is_empty() {
                                    app_state = AppState::default();
                                } else {
                                    app_state = AppState::ViewingFriendRequests {
                                        requests,
                                        selected_idx: 0,
                                    };
                                }
                            } else {
                                app_state = AppState::default();
                            }
                            drop(app_lock);
                        }
                        Some(AppAction::RejectFriendRequest(id)) => {
                            let return_to_list =
                                matches!(app_state, AppState::ViewingFriendRequests { .. });
                            let app_lock = app.lock().await;

                            match handle_reject_friend_request(&app_lock, id) {
                                Ok(_) => {
                                    status_flash = Some((
                                        std::time::Instant::now(),
                                        "Friend request rejected".to_string(),
                                    ));
                                }
                                Err(e) => {
                                    status_flash = Some((
                                        std::time::Instant::now(),
                                        crate::ui::error::format_error_for_user(&e),
                                    ));
                                }
                            }

                            if return_to_list {
                                let requests =
                                    db::queries::get_pending_friend_requests(&app_lock.db)
                                        .unwrap_or_default();
                                if requests.is_empty() {
                                    app_state = AppState::default();
                                } else {
                                    app_state = AppState::ViewingFriendRequests {
                                        requests,
                                        selected_idx: 0,
                                    };
                                }
                            } else {
                                app_state = AppState::default();
                            }
                            drop(app_lock);
                        }
                        Some(AppAction::ViewFriendRequests) => {
                            let app_lock = app.lock().await;
                            let requests = db::queries::get_pending_friend_requests(&app_lock.db)
                                .unwrap_or_default();
                            drop(app_lock);
                            if requests.is_empty() {
                                status_flash = Some((
                                    std::time::Instant::now(),
                                    "No pending friend requests".to_string(),
                                ));
                            } else {
                                app_state = AppState::ViewingFriendRequests {
                                    requests,
                                    selected_idx: 0,
                                };
                            }
                        }
                        Some(AppAction::ViewMyIdentity) => {
                            let app_lock = app.lock().await;
                            if let Some(onion) = &app_lock.onion_address {
                                let friend_code = crate::tor::address::onion_to_friend_code(onion)
                                    .unwrap_or_else(|_| "unknown".to_string());
                                app_state = AppState::ViewingMyIdentity {
                                    friend_code,
                                    onion_address: onion.clone(),
                                    copied_field: None,
                                };
                            } else {
                                // Tor not ready yet - can't show identity
                                app_state = AppState::ViewingMyIdentity {
                                    friend_code: "(Waiting for Tor...)".to_string(),
                                    onion_address: "(Waiting for Tor...)".to_string(),
                                    copied_field: None,
                                };
                            }
                            drop(app_lock);
                        }
                        Some(AppAction::SelectFriend(idx)) => {
                            let app_lock = app.lock().await;
                            let friends = db::queries::get_friends_with_unread(&app_lock.db)
                                .unwrap_or_default();
                            if let Some(friend) = friends.get(idx) {
                                let conv_id = db::queries::get_or_create_conversation(
                                    &app_lock.db,
                                    friend.friend_id,
                                )
                                .unwrap_or(0);

                                if conv_id > 0 {
                                    db::queries::mark_conversation_read(&app_lock.db, conv_id).ok();
                                    db::queries::activate_ephemeral_timers(&app_lock.db, conv_id)
                                        .ok();

                                    // Send read receipts for unread messages from peer
                                    let own_onion =
                                        app_lock.onion_address.clone().unwrap_or_default();
                                    if let Ok(unreceipted) =
                                        db::queries::get_unreceipted_message_ids(
                                            &app_lock.db,
                                            conv_id,
                                            &own_onion,
                                        )
                                    {
                                        for (msg_id, sender_onion) in &unreceipted {
                                            if let Ok(uuid) = uuid::Uuid::parse_str(msg_id) {
                                                let receipt =
                                                    protocol::message::DeliveryReceiptMessage {
                                                        message_id: uuid,
                                                        timestamp: std::time::SystemTime::now()
                                                            .duration_since(std::time::UNIX_EPOCH)
                                                            .unwrap_or_default()
                                                            .as_secs()
                                                            as i64,
                                                    };
                                                let receipt_msg =
                                                    protocol::message::Message::ReadReceipt(
                                                        receipt,
                                                    );
                                                app_lock
                                                    .message_queue
                                                    .enqueue(
                                                        &app_lock.db,
                                                        sender_onion,
                                                        &receipt_msg,
                                                        "low",
                                                    )
                                                    .ok();
                                            }
                                            // Mark the message as read locally
                                            db::queries::update_message_status(
                                                &app_lock.db,
                                                msg_id,
                                                "read",
                                            )
                                            .ok();
                                        }
                                    }
                                }

                                if let AppState::Normal {
                                    conversation_id, ..
                                } = &mut app_state
                                {
                                    *conversation_id = Some(conv_id);
                                }
                            }
                            drop(app_lock);
                        }
                        Some(AppAction::SendMessage(content)) => {
                            let app_lock = app.lock().await;

                            if let AppState::Normal {
                                conversation_id: Some(conv_id),
                                selected_friend_idx: Some(idx),
                                ..
                            } = &app_state
                            {
                                let conv_id = *conv_id;
                                let idx = *idx;

                                // Get friend info
                                let friends = db::queries::get_friends_with_unread(&app_lock.db)
                                    .unwrap_or_default();
                                if let Some(friend) = friends.get(idx) {
                                    let peer_onion = friend.onion_address.clone();
                                    let own_onion =
                                        app_lock.onion_address.clone().unwrap_or_default();
                                    let msg_id = uuid::Uuid::new_v4().to_string();

                                    // Check ephemeral TTL for this conversation
                                    let conv_ttl = db::queries::get_conversation_ephemeral_ttl(
                                        &app_lock.db,
                                        conv_id,
                                    )
                                    .unwrap_or(None);

                                    // Store locally first
                                    db::queries::store_outgoing_message_with_ttl(
                                        &app_lock.db,
                                        conv_id,
                                        &own_onion,
                                        &content,
                                        &msg_id,
                                        conv_ttl,
                                    )
                                    .ok();

                                    // Encrypt the message using Signal session
                                    let encrypted_msg = {
                                        let store = crypto::SessionStore::new(&app_lock.db);
                                        let session = store.load_session(&peer_onion);
                                        match session {
                                            Ok(Some(mut session)) => {
                                                let payload = protocol::message::PlaintextPayload {
                                                    content: content.clone(),
                                                    sent_at: std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_secs()
                                                        as i64,
                                                    message_type: "text".to_string(),
                                                    ephemeral_ttl: conv_ttl,
                                                };
                                                let plaintext = serde_json::to_vec(&payload).ok();
                                                match plaintext {
                                                    Some(pt) => match session.encrypt(&pt) {
                                                        Ok((header, ciphertext, is_prekey)) => {
                                                            store.store_session(&session).ok();
                                                            Some(protocol::message::TextMessage {
                                                                    from_onion: own_onion.clone(),
                                                                    to_onion: peer_onion.clone(),
                                                                    signal_header: base64::engine::general_purpose::STANDARD.encode(&header),
                                                                    signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
                                                                    signal_type: if is_prekey {
                                                                        protocol::message::SignalMessageType::PrekeyMessage
                                                                    } else {
                                                                        protocol::message::SignalMessageType::Message
                                                                    },
                                                                    timestamp: payload.sent_at,
                                                                    message_id: uuid::Uuid::parse_str(&msg_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                                                                    x3dh_init: None,
                                                                })
                                                        }
                                                        Err(_) => None,
                                                    },
                                                    None => None,
                                                }
                                            }
                                            _ => None,
                                        }
                                    };

                                    if let Some(text_msg) = encrypted_msg {
                                        let msg = protocol::message::Message::TextMessage(
                                            text_msg.clone(),
                                        );
                                        // Try to send directly, queue on failure
                                        match try_send_direct(&app_lock, &peer_onion, &msg).await {
                                            Ok(_) => {
                                                db::queries::update_message_status(
                                                    &app_lock.db,
                                                    &msg_id,
                                                    "sent",
                                                )
                                                .ok();
                                            }
                                            Err(_) => {
                                                app_lock
                                                    .message_queue
                                                    .enqueue(
                                                        &app_lock.db,
                                                        &peer_onion,
                                                        &msg,
                                                        "normal",
                                                    )
                                                    .ok();
                                                db::queries::update_message_status(
                                                    &app_lock.db,
                                                    &msg_id,
                                                    "queued",
                                                )
                                                .ok();
                                            }
                                        }
                                    } else {
                                        status_flash = Some((
                                            std::time::Instant::now(),
                                            "Message failed \u{2014} no encryption session"
                                                .to_string(),
                                        ));
                                        db::queries::update_message_status(
                                            &app_lock.db,
                                            &msg_id,
                                            "failed",
                                        )
                                        .ok();
                                    }
                                }
                            }

                            drop(app_lock);
                            was_typing = false;
                            last_typing_sent = None;
                        }
                        Some(AppAction::SetEphemeralTtl(conv_id, ttl)) => {
                            let app_lock = app.lock().await;
                            db::queries::set_conversation_ephemeral_ttl(&app_lock.db, conv_id, ttl)
                                .ok();
                            drop(app_lock);
                        }
                        Some(AppAction::PublishChannelPost(content, channel_type)) => {
                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                            let channel_id = if channel_type == "public" { 1 } else { 2 };
                            let post_id = uuid::Uuid::new_v4().to_string();
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64;

                            // Sign the post
                            let sign_data = format!("{}{}{}", post_id, content, now);
                            let identity = match app_lock.identity.as_ref() {
                                Some(id) => id,
                                None => {
                                    eprintln!(
                                        "Cannot publish channel post: identity not initialized"
                                    );
                                    drop(app_lock);
                                    continue;
                                }
                            };
                            let signature = base64::engine::general_purpose::STANDARD
                                .encode(identity.sign(sign_data.as_bytes()).to_bytes());

                            // Store locally
                            db::queries::store_channel_post(
                                &app_lock.db,
                                channel_id,
                                &content,
                                &post_id,
                                now,
                                &signature,
                            )
                            .ok();

                            // Enforce retention
                            db::queries::enforce_channel_retention(&app_lock.db, channel_id).ok();

                            // Push to online subscribers
                            let channel_type_enum = if channel_type == "public" {
                                protocol::message::ChannelType::Public
                            } else {
                                protocol::message::ChannelType::FriendsOnly
                            };

                            let post_msg = protocol::message::Message::ChannelPost(
                                protocol::message::ChannelPostMessage {
                                    publisher_onion: own_onion,
                                    channel_type: channel_type_enum,
                                    post_id: uuid::Uuid::parse_str(&post_id)
                                        .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                                    content,
                                    created_at: now,
                                    signature,
                                },
                            );

                            let subscribers =
                                db::queries::get_channel_subscribers(&app_lock.db, &channel_type)
                                    .unwrap_or_default();
                            for sub_onion in subscribers {
                                app_lock
                                    .message_queue
                                    .enqueue(&app_lock.db, &sub_onion, &post_msg, "normal")
                                    .ok();
                            }

                            drop(app_lock);
                        }
                        Some(AppAction::SubscribeToChannel(publisher_onion)) => {
                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();

                            // Store subscription locally
                            db::queries::add_channel_subscription(
                                &app_lock.db,
                                &publisher_onion,
                                "public",
                            )
                            .ok();

                            // Send subscribe message to publisher
                            let sub_msg = protocol::message::Message::ChannelSubscribe(
                                protocol::message::ChannelSubscribeMessage {
                                    subscriber_onion: own_onion,
                                    channel_type: protocol::message::ChannelType::Public,
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs()
                                        as i64,
                                },
                            );
                            app_lock
                                .message_queue
                                .enqueue(&app_lock.db, &publisher_onion, &sub_msg, "normal")
                                .ok();

                            drop(app_lock);
                            app_state = AppState::default();
                        }
                        Some(AppAction::SelectChannel(publisher_onion, channel_type, is_own)) => {
                            app_state = AppState::ViewingChannel {
                                publisher_onion,
                                channel_type,
                                is_own,
                                input: String::new(),
                                cursor: 0,
                                scroll_offset: 0,
                            };
                        }
                        Some(AppAction::ViewOwnChannel) => {
                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                            drop(app_lock);
                            app_state = AppState::ViewingChannel {
                                publisher_onion: own_onion,
                                channel_type: "public".to_string(),
                                is_own: true,
                                input: String::new(),
                                cursor: 0,
                                scroll_offset: 0,
                            };
                        }
                        Some(AppAction::ToggleNotifications) => {
                            let app_lock = app.lock().await;
                            let new_state = notifications::toggle(&app_lock.db);
                            drop(app_lock);
                            status_flash = Some((
                                std::time::Instant::now(),
                                if new_state {
                                    "Notifications: ON".to_string()
                                } else {
                                    "Notifications: OFF".to_string()
                                },
                            ));
                        }
                        Some(AppAction::SendPresence(_)) => {} // Reserved for future use
                        Some(AppAction::Quit) => break Ok(()),
                        None => {} // Just state change
                    }

                    // When entering the ephemeral modal, highlight the current TTL
                    if !was_setting_ephemeral {
                        if let AppState::SettingEphemeral {
                            conversation_id: conv_id,
                            ref mut selected_idx,
                        } = app_state
                        {
                            let app_lock = app.lock().await;
                            let current_ttl =
                                db::queries::get_conversation_ephemeral_ttl(&app_lock.db, conv_id)
                                    .unwrap_or(None);
                            drop(app_lock);
                            *selected_idx = match current_ttl {
                                None => 0,         // Off
                                Some(300) => 1,    // 5 minutes
                                Some(3600) => 2,   // 1 hour
                                Some(86400) => 3,  // 24 hours
                                Some(604800) => 4, // 7 days
                                Some(_) => 0,      // Unknown -> Off
                            };
                        }
                    }

                    // Typing indicator detection
                    if let AppState::Normal {
                        input_focused: true,
                        ref input,
                        selected_friend_idx: Some(idx),
                        ..
                    } = &app_state
                    {
                        let is_typing_now = !input.is_empty();
                        let should_send_started = is_typing_now
                            && (!was_typing
                                || last_typing_sent
                                    .map_or(true, |t| t.elapsed() >= presence::TYPING_DEBOUNCE));
                        let should_send_stopped = !is_typing_now && was_typing;

                        if should_send_started || should_send_stopped {
                            let app_lock = app.lock().await;
                            let friends = db::queries::get_friends_with_unread(&app_lock.db)
                                .unwrap_or_default();
                            if let Some(friend) = friends.get(*idx) {
                                let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64;

                                let presence_type = if should_send_started {
                                    protocol::message::PresenceType::TypingStarted
                                } else {
                                    protocol::message::PresenceType::TypingStopped
                                };

                                let msg = protocol::message::Message::Presence(
                                    protocol::message::PresenceMessage {
                                        from_onion: own_onion,
                                        presence_type,
                                        timestamp: now,
                                    },
                                );

                                // Best-effort send (don't queue typing indicators)
                                if let Some(ref pool) = app_lock.connection_pool {
                                    let _ = pool.send(&friend.onion_address, &msg).await;
                                }
                            }
                            drop(app_lock);

                            if should_send_started {
                                last_typing_sent = Some(std::time::Instant::now());
                            }
                        }
                        was_typing = is_typing_now;
                    }
                }
                Event::Mouse(_) => {} // Reserved for future mouse interactions
                _ => {}               // Resize and other events
            }
        }

        // Check for incoming messages from listener
        {
            let mut app_lock = app.lock().await;

            // Drain incoming messages into a local vec to avoid borrow conflict
            let mut incoming_messages = Vec::new();
            if let Some(rx) = &mut app_lock.incoming_message_rx {
                while let Ok(incoming) = rx.try_recv() {
                    incoming_messages.push(incoming);
                }
            }

            let had_incoming = !incoming_messages.is_empty();

            // Process collected messages
            for incoming in incoming_messages {
                if let Err(e) = handle_incoming_message(&app_lock, incoming, &presence_map).await {
                    eprintln!("Failed to handle incoming message: {}", e);
                }
            }

            if had_incoming {
                dirty = true; // Incoming messages invalidate cached state
            }
        }

        // Check for queue processing commands
        let should_process = {
            let mut app_lock = app.lock().await;
            if let Some(rx) = &mut app_lock.queue_command_rx {
                rx.try_recv().is_ok()
            } else {
                false
            }
        };

        if should_process {
            let app_lock = app.lock().await;
            if let Err(e) = process_message_queue(&app_lock).await {
                eprintln!("Queue processing error: {}", e);
            }
            dirty = true; // Queue processing may change state
        }
    };

    // Cleanup
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}
