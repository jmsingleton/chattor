mod error;
mod app;
mod cli;
mod config;
mod crypto;
mod db;
mod tor;
mod protocol;
mod net;
mod notifications;
mod presence;
mod ui;

use clap::Parser;
use cli::Cli;
use error::Result;
use app::App;
use ui::{AppState, AppAction, RenderContext};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::time::Duration;
use std::io;
use crate::crypto::IdentityKeypair;
use crate::error::LogErr;
use std::sync::Arc;
use tokio::sync::Mutex;
use base64::Engine;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize application with optional CLI directory overrides
    let app = {
        let mut settings = config::Settings::default()?;
        if let Some(ref dir) = cli.config_dir {
            settings.config_dir = std::path::PathBuf::from(dir);
        }
        if let Some(ref dir) = cli.data_dir {
            settings.data_dir = std::path::PathBuf::from(dir);
            settings.db_path = settings.data_dir.join("messages.db");
        }
        Arc::new(Mutex::new(App::new_with_settings(settings)?))
    };

    // Set up tracing. The TUI takes over the terminal in raw alt-screen mode,
    // so stderr is invisible to the user until the app exits. Route all
    // tracing output to `data_dir/chattor.log` instead. Level is INFO by
    // default, DEBUG when `--debug` is passed. Failure to open the log file
    // is non-fatal — the app still runs, just without logging.
    init_tracing(&app.lock().await.settings.data_dir, cli.debug);

    // Load theme
    let theme = {
        let app_lock = app.lock().await;
        let config_path = app_lock.settings.config_dir.join("theme.toml");
        drop(app_lock);
        ui::theme::load_theme(cli.theme.as_deref(), &config_path)
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
    let (bootstrap_tx, mut bootstrap_rx) = tokio::sync::watch::channel(
        ui::BootstrapUpdate::Progress(0)
    );

    // Spawn Tor init in background, communicating via watch channel
    let app_tor = Arc::clone(&app);
    tokio::spawn(async move {
        let mut app_lock = app_tor.lock().await;
        match app_lock.init_tor().await {
            Ok(()) => {
                let _ = bootstrap_tx.send(ui::BootstrapUpdate::Connected);
            }
            Err(e) => {
                let _ = bootstrap_tx.send(ui::BootstrapUpdate::Failed(format!("{}", e)));
            }
        }
    });

    // Run bootstrap animation loop
    let mut phase = ui::BootstrapPhase::new();
    let bootstrap_start = std::time::Instant::now();
    let bootstrap_timeout = std::time::Duration::from_secs(60);

    loop {
        // Render current bootstrap frame
        match &phase {
            ui::BootstrapPhase::Connecting { frame, tick, progress } => {
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
                            break;
                        }
                        ui::BootstrapAction::Retry => {
                            phase = ui::BootstrapPhase::new();
                            let (new_tx, new_rx) = tokio::sync::watch::channel(
                                ui::BootstrapUpdate::Progress(0)
                            );
                            bootstrap_rx = new_rx;
                            let app_retry = Arc::clone(&app);
                            tokio::spawn(async move {
                                let mut app_lock = app_retry.lock().await;
                                match app_lock.init_tor().await {
                                    Ok(()) => {
                                        let _ = new_tx.send(ui::BootstrapUpdate::Connected);
                                    }
                                    Err(e) => {
                                        let _ = new_tx.send(ui::BootstrapUpdate::Failed(
                                            format!("{}", e)
                                        ));
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
                        app_lock.message_queue.enqueue(&app_lock.db, &peer_onion, &sync_msg, "low").ok();
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        }
    });

    // Spawn heartbeat task — sends presence updates to connected peers
    let app_heartbeat = Arc::clone(&app);
    tokio::spawn(async move {
        // Wait for Tor to initialize before starting heartbeats
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        loop {
            {
                let app_lock = app_heartbeat.lock().await;
                if let Some(ref pool) = app_lock.connection_pool {
                    let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                    let peers = pool.connected_peers();
                    drop(app_lock);

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    for peer in peers {
                        let msg = protocol::message::Message::Presence(
                            protocol::message::PresenceMessage {
                                from_onion: own_onion.clone(),
                                presence_type: protocol::message::PresenceType::Heartbeat,
                                timestamp: now,
                            }
                        );
                        // Best-effort: don't retry or queue heartbeats
                        let app_lock = app_heartbeat.lock().await;
                        if let Some(ref pool) = app_lock.connection_pool {
                            let _ = pool.send(&peer, &msg).await;
                        }
                    }
                }
            }
            tokio::time::sleep(presence::HEARTBEAT_INTERVAL).await;
        }
    });

    // Initialize state machine
    let mut app_state = AppState::default();

    // Initialize presence tracker (in-memory only)
    let presence_map = presence::new_presence_map();

    let mut last_typing_sent: Option<std::time::Instant> = None;
    let mut was_typing = false;
    let mut notification_flash: Option<(std::time::Instant, &str)> = None;

    // Main event loop
    let result = loop {
        // Lock app to build render context
        let app_lock = app.lock().await;

        let friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
        let pending_request_count = db::queries::get_pending_request_count(&app_lock.db).unwrap_or(0);
        let own_onion = app_lock.onion_address.clone();
        let friend_code = own_onion.as_deref().and_then(|o| {
            crate::tor::address::onion_to_friend_code(o).ok()
        });
        let tor_connected = app_lock.tor_client.is_some();

        // Cleanup expired ephemeral messages
        db::queries::cleanup_expired_messages(&app_lock.db).ok();

        // Load messages and ephemeral TTL for selected conversation
        let (messages, conversation_ephemeral_ttl) = if let AppState::Normal { conversation_id: Some(conv_id), .. } = &app_state {
            let msgs = db::queries::get_messages(&app_lock.db, *conv_id, 100, 0).unwrap_or_default();
            let ttl = db::queries::get_conversation_ephemeral_ttl(&app_lock.db, *conv_id).unwrap_or(None);
            (msgs, ttl)
        } else {
            (Vec::new(), None)
        };

        // Load channel data
        let channel_subscriptions = db::queries::get_channel_subscriptions(&app_lock.db).unwrap_or_default();

        let (channel_posts, channel_post_read_counts) = if let AppState::ViewingChannel {
            ref publisher_onion, ref channel_type, is_own, ..
        } = &app_state {
            // The feed is keyed by (publisher_onion, channel_type). For our
            // own channels, publisher_onion is already set to our address by
            // the action handler; for foreign feeds, it's the peer's address.
            let posts = db::queries::get_channel_posts(
                &app_lock.db, publisher_onion, channel_type, 100
            ).unwrap_or_default();
            let mut counts = std::collections::HashMap::new();
            if *is_own {
                for post in &posts {
                    let count = db::queries::get_channel_post_read_count(&app_lock.db, &post.post_id).unwrap_or(0);
                    counts.insert(post.post_id.clone(), count);
                }
            }
            (posts, counts)
        } else {
            (Vec::new(), std::collections::HashMap::new())
        };

        // Release lock before rendering
        drop(app_lock);

        let presence_snapshot = presence::get_presence_snapshot(&presence_map).await;

        let ctx = RenderContext {
            friends,
            messages,
            own_onion,
            friend_code,
            tor_connected,
            pending_request_count,
            conversation_ephemeral_ttl,
            channel_subscriptions,
            channel_posts,
            channel_post_read_counts,
            theme: theme.clone(),
            presence: presence_snapshot,
            notification_flash: notification_flash
                .as_ref()
                .filter(|(t, _)| t.elapsed() < std::time::Duration::from_secs(2))
                .map(|(_, msg)| msg.to_string()),
        };

        // Expire notification flash
        if notification_flash.as_ref().is_some_and(|(t, _)| t.elapsed() >= std::time::Duration::from_secs(2)) {
            notification_flash = None;
        }

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
                    // Bounds for directional navigation. The current friend
                    // and subscription counts come from the render-context
                    // snapshot built each tick.
                    let nav_ctx = ui::NavContext {
                        friends_count: ctx.friends.len(),
                        subscriptions_count: ctx.channel_subscriptions.len(),
                    };
                    match app_state.handle_key_with_context(key, nav_ctx)? {
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
                            let return_to_list = matches!(app_state, AppState::ViewingFriendRequests { .. });
                            let app_lock = app.lock().await;

                            match handle_accept_friend_request(&app_lock, id) {
                                Ok(_) => {}
                                Err(e) => tracing::error!(error = %e, "failed to accept friend request"),
                            }

                            if return_to_list {
                                let requests = db::queries::get_pending_friend_requests(&app_lock.db).unwrap_or_default();
                                if requests.is_empty() {
                                    app_state = AppState::default();
                                } else {
                                    app_state = AppState::ViewingFriendRequests { requests, selected_idx: 0 };
                                }
                            } else {
                                app_state = AppState::default();
                            }
                            drop(app_lock);
                        }
                        Some(AppAction::RejectFriendRequest(id)) => {
                            let return_to_list = matches!(app_state, AppState::ViewingFriendRequests { .. });
                            let app_lock = app.lock().await;

                            match handle_reject_friend_request(&app_lock, id) {
                                Ok(_) => {}
                                Err(e) => tracing::error!(error = %e, "failed to reject friend request"),
                            }

                            if return_to_list {
                                let requests = db::queries::get_pending_friend_requests(&app_lock.db).unwrap_or_default();
                                if requests.is_empty() {
                                    app_state = AppState::default();
                                } else {
                                    app_state = AppState::ViewingFriendRequests { requests, selected_idx: 0 };
                                }
                            } else {
                                app_state = AppState::default();
                            }
                            drop(app_lock);
                        }
                        Some(AppAction::ViewFriendRequests) => {
                            let app_lock = app.lock().await;
                            let requests = db::queries::get_pending_friend_requests(&app_lock.db).unwrap_or_default();
                            drop(app_lock);
                            if !requests.is_empty() {
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
                            let friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
                            if let Some(friend) = friends.get(idx) {
                                let conv_id = db::queries::get_or_create_conversation(
                                    &app_lock.db, friend.friend_id
                                ).unwrap_or(0);

                                if conv_id > 0 {
                                    db::queries::mark_conversation_read(&app_lock.db, conv_id)
                                        .log_err("mark_conversation_read");
                                    db::queries::activate_ephemeral_timers(&app_lock.db, conv_id)
                                        .log_err("activate_ephemeral_timers");

                                    // Send read receipts for unread messages from peer
                                    let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                                    if let Ok(unreceipted) = db::queries::get_unreceipted_message_ids(&app_lock.db, conv_id, &own_onion) {
                                        for (msg_id, sender_onion) in &unreceipted {
                                            if let Ok(uuid) = uuid::Uuid::parse_str(msg_id) {
                                                let receipt = protocol::message::DeliveryReceiptMessage {
                                                    message_id: uuid,
                                                    timestamp: std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_secs() as i64,
                                                };
                                                let receipt_msg = protocol::message::Message::ReadReceipt(receipt);
                                                app_lock.message_queue.enqueue(&app_lock.db, sender_onion, &receipt_msg, "low")
                                                    .log_err("enqueue read receipt");
                                            }
                                            // Mark the message as read locally
                                            db::queries::update_message_status(&app_lock.db, msg_id, "read")
                                                .log_err("update_message_status('read')");
                                        }
                                    }
                                }

                                if let AppState::Normal { conversation_id, .. } = &mut app_state {
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
                            } = &app_state {
                                let conv_id = *conv_id;
                                let idx = *idx;

                                // Get friend info
                                let friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
                                if let Some(friend) = friends.get(idx) {
                                    let peer_onion = friend.onion_address.clone();
                                    let own_onion = app_lock.onion_address.clone()
                                        .unwrap_or_default();
                                    let msg_id = uuid::Uuid::new_v4().to_string();

                                    // Check ephemeral TTL for this conversation
                                    let conv_ttl = db::queries::get_conversation_ephemeral_ttl(&app_lock.db, conv_id).unwrap_or(None);

                                    // Store locally first
                                    db::queries::store_outgoing_message_with_ttl(
                                        &app_lock.db, conv_id, &own_onion, &content, &msg_id, conv_ttl
                                    ).ok();

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
                                                        .as_secs() as i64,
                                                    message_type: "text".to_string(),
                                                    ephemeral_ttl: conv_ttl,
                                                };
                                                let plaintext = serde_json::to_vec(&payload).ok();
                                                match plaintext {
                                                    Some(pt) => {
                                                        match session.encrypt(&pt) {
                                                            Ok((header, ciphertext, is_prekey)) => {
                                                                store.store_session(&session)
                                                                    .log_err("store_session after encrypt");
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
                                                            Err(_) => None
                                                        }
                                                    }
                                                    None => None
                                                }
                                            }
                                            _ => None
                                        }
                                    };

                                    if let Some(text_msg) = encrypted_msg {
                                        let msg = protocol::message::Message::TextMessage(text_msg.clone());
                                        // Try to send directly, queue on failure
                                        match try_send_direct(&app_lock, &peer_onion, &msg).await {
                                            Ok(_) => {
                                                db::queries::update_message_status(&app_lock.db, &msg_id, "sent")
                                                    .log_err("update_message_status('sent')");
                                            }
                                            Err(_) => {
                                                app_lock.message_queue.enqueue(&app_lock.db, &peer_onion, &msg, "normal")
                                                    .log_err("enqueue text message on direct-send failure");
                                                db::queries::update_message_status(&app_lock.db, &msg_id, "queued")
                                                    .log_err("update_message_status('queued')");
                                            }
                                        }
                                    } else {
                                        tracing::warn!(peer = %peer_onion, "failed to encrypt message: no session");
                                        db::queries::update_message_status(&app_lock.db, &msg_id, "failed")
                                            .log_err("update_message_status('failed')");
                                    }
                                }
                            }

                            drop(app_lock);
                            was_typing = false;
                            last_typing_sent = None;
                        }
                        Some(AppAction::SetEphemeralTtl(conv_id, ttl)) => {
                            let app_lock = app.lock().await;
                            db::queries::set_conversation_ephemeral_ttl(&app_lock.db, conv_id, ttl).ok();
                            drop(app_lock);
                        }
                        Some(AppAction::PublishChannelPost(content, channel_type)) => {
                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                            let post_id = uuid::Uuid::new_v4().to_string();
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64;

                            // Sign the post. Bytes signed must be reproducible
                            // by subscribers from the wire fields alone, so we
                            // sign `post_id || content || created_at`.
                            let sign_data = format!("{}{}{}", post_id, content, now);
                            let signature = base64::engine::general_purpose::STANDARD.encode(
                                app_lock.identity.as_ref()
                                    .expect("identity set during init")
                                    .sign(sign_data.as_bytes()).to_bytes()
                            );

                            // Store locally under our own onion + channel_type.
                            db::queries::store_channel_post(
                                &app_lock.db, &own_onion, &channel_type,
                                &content, &post_id, now, &signature
                            ).log_err("store_channel_post (own publish)");

                            // Enforce retention for this feed.
                            db::queries::enforce_channel_retention(
                                &app_lock.db, &own_onion, &channel_type
                            ).log_err("enforce_channel_retention (own feed)");

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
                                    post_id: uuid::Uuid::parse_str(&post_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                                    content,
                                    created_at: now,
                                    signature,
                                }
                            );

                            let subscribers = db::queries::get_channel_subscribers(&app_lock.db, &channel_type).unwrap_or_default();
                            for sub_onion in subscribers {
                                app_lock.message_queue.enqueue(&app_lock.db, &sub_onion, &post_msg, "normal")
                                    .log_err("enqueue channel post to subscriber");
                            }

                            drop(app_lock);
                        }
                        Some(AppAction::SubscribeToChannel(publisher_onion)) => {
                            let app_lock = app.lock().await;
                            let own_onion = app_lock.onion_address.clone().unwrap_or_default();

                            // Store subscription locally
                            db::queries::add_channel_subscription(&app_lock.db, &publisher_onion, "public")
                                .log_err("add_channel_subscription");

                            // Send subscribe message to publisher
                            let sub_msg = protocol::message::Message::ChannelSubscribe(
                                protocol::message::ChannelSubscribeMessage {
                                    subscriber_onion: own_onion,
                                    channel_type: protocol::message::ChannelType::Public,
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs() as i64,
                                }
                            );
                            app_lock.message_queue.enqueue(&app_lock.db, &publisher_onion, &sub_msg, "normal")
                                .log_err("enqueue ChannelSubscribe");

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
                            notification_flash = Some((
                                std::time::Instant::now(),
                                if new_state { "Notifications: ON" } else { "Notifications: OFF" },
                            ));
                        }
                        Some(AppAction::SendPresence(_)) => {} // Reserved for future use
                        Some(AppAction::Quit) => break Ok(()),
                        None => {} // Just state change
                    }

                    // Typing indicator detection
                    if let AppState::Normal { input_focused: true, ref input, selected_friend_idx: Some(idx), .. } = &app_state {
                        let is_typing_now = !input.is_empty();
                        let should_send_started = is_typing_now && (!was_typing || last_typing_sent.map_or(true, |t| t.elapsed() >= presence::TYPING_DEBOUNCE));
                        let should_send_stopped = !is_typing_now && was_typing;

                        if should_send_started || should_send_stopped {
                            let app_lock = app.lock().await;
                            let friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
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
                                    }
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
                _ => {} // Resize and other events
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

            // Process collected messages, rate-limited per peer onion.
            // The token bucket lets each peer burst up to 20 messages and
            // sustain 5 req/s; messages above that rate are dropped with a
            // log. Receipts (no `from_onion`) bypass the check.
            for incoming in incoming_messages {
                if let Some(peer) = incoming.message.peer_onion() {
                    if !app_lock.inbound_rate_limiter.check(peer) {
                        tracing::warn!(peer = %peer, "rate-limited inbound message, dropping");
                        continue;
                    }
                }
                if let Err(e) = handle_incoming_message(&app_lock, incoming, &presence_map).await {
                    tracing::error!(error = %e, "failed to handle incoming message");
                }
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
                tracing::error!(error = %e, "queue processing error");
            }
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

/// Set up tracing to write to `data_dir/chattor.log`. INFO by default; DEBUG
/// when `debug` is true. Falls back to no-op on any I/O failure so the app
/// always starts.
fn init_tracing(data_dir: &std::path::Path, debug: bool) {
    use tracing_subscriber::filter::LevelFilter;

    if std::fs::create_dir_all(data_dir).is_err() {
        return;
    }
    let log_path = data_dir.join("chattor.log");
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(_) => return,
    };

    let level = if debug { LevelFilter::DEBUG } else { LevelFilter::INFO };

    // try_init returns Err if a global subscriber is already set (e.g. by a
    // test harness); ignore.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_max_level(level)
        .with_target(true)
        .with_ansi(false)
        .try_init();
}

/// Handle sending a friend request
async fn handle_send_friend_request(app: &App, peer_input: &str) -> Result<SendResult> {
    use crate::protocol::friend_request::FriendRequestHandler;

    let trimmed = peer_input.trim();

    // Accept both .onion addresses and friend codes (word sequences)
    let peer_onion = if trimmed.ends_with(".onion") {
        trimmed.to_string()
    } else {
        // Try to decode as a friend code (reversible word encoding of .onion)
        match crate::protocol::friend_code::friend_code_to_onion(trimmed) {
            Ok(onion) => onion,
            Err(_) => return Err(error::ChattorError::Tor(
                "Enter a .onion address or friend code (word sequence from their Identity)".into()
            )),
        }
    };
    let peer_onion = peer_onion.as_str();

    // Get our .onion address
    let own_onion = app.onion_address.as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized yet".into()))?;

    // Generate our own friend code to include in the request
    let own_friend_code = crate::tor::address::onion_to_friend_code(own_onion)
        .unwrap_or_else(|_| "unknown".to_string());

    // Create friend request message
    let request_msg = FriendRequestHandler::create_request(
        app.identity.as_ref().expect("identity set during init"),
        own_onion,
        &own_friend_code,
    )?;

    // Wrap in Message enum
    let message = protocol::message::Message::FriendRequest(request_msg);

    // Try direct send, queue on failure
    match try_send_direct(app, peer_onion, &message).await {
        Ok(_) => Ok(SendResult::SentImmediately),
        Err(_) => {
            // Queue for background delivery
            app.message_queue.enqueue(&app.db, peer_onion, &message, "high")?;
            Ok(SendResult::Queued)
        }
    }
}

/// Handle accepting a friend request
fn handle_accept_friend_request(app: &App, request_id: i64) -> Result<()> {
    use crate::crypto::PreKeyBundle;

    // Get our .onion address
    let own_onion = app.onion_address.as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized yet".into()))?;

    // Get the friend request from database
    let conn = app.db.connection();
    let (from_onion, _friend_code): (String, String) = conn.query_row(
        "SELECT from_onion, friend_code FROM friend_requests WHERE id = ?1",
        [request_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(|e| error::ChattorError::Database(format!("Failed to load request: {}", e)))?;

    // Generate PreKey bundle for the accept message.
    // Generate a dedicated X25519 Signal identity keypair for X3DH.
    // This is separate from the Ed25519 identity used for friend request signing.
    let identity = app.identity.as_ref().expect("identity set during init");
    let signal_identity = libsignal_protocol::vxeddsa::gen_keypair();
    let signal_identity_public_raw = libsignal_protocol::utils::decode_public_key(&signal_identity.public)
        .map_err(|_| error::ChattorError::Crypto("Failed to decode signal identity public key".into()))?;
    let (bundle, private_keys) = PreKeyBundle::generate_real(&signal_identity.secret, &signal_identity_public_raw)?;

    // Create accept message (inline to avoid Database clone issue)
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Sign message
    let data = format!("{}{}{}", own_onion, from_onion, timestamp);
    let signature = identity.sign(data.as_bytes());

    // Serialize bundle to JSON
    let bundle_json = serde_json::to_string(&bundle)
        .map_err(|e| error::ChattorError::Crypto(format!("Failed to serialize bundle: {}", e)))?;

    let accept_msg = protocol::message::FriendRequestAcceptMessage {
        from_onion: own_onion.to_string(),
        to_onion: from_onion.clone(),
        signal_prekey_bundle: bundle_json,
        timestamp,
        signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
    };

    // Store PreKey private material so we can create the Signal session later
    // when the peer sends their first PreKey message. We do NOT create the
    // session here — the shared secret requires the peer's ephemeral key,
    // which is embedded in their first encrypted message.
    let identity_b64 = base64::engine::general_purpose::STANDARD.encode(private_keys.identity_secret);
    let spk_b64 = base64::engine::general_purpose::STANDARD.encode(private_keys.signed_prekey_secret);
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (&format!("prekey_identity:{}", from_onion), &identity_b64),
    ).map_err(|e| error::ChattorError::Database(
        format!("Failed to store PreKey identity material: {}", e)
    ))?;
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (&format!("prekey_spk:{}", from_onion), &spk_b64),
    ).map_err(|e| error::ChattorError::Database(
        format!("Failed to store PreKey SPK material: {}", e)
    ))?;
    if let Some(opk_secret) = private_keys.prekey_secret {
        let opk_b64 = base64::engine::general_purpose::STANDARD.encode(opk_secret);
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
            (&format!("prekey_opk:{}", from_onion), &opk_b64),
        ).map_err(|e| error::ChattorError::Database(
            format!("Failed to store PreKey OPK material: {}", e)
        ))?;
    }
    // Also store the Signal identity secret for the initiator side
    // (needed when handle_incoming_accept creates the session)
    let signal_secret_b64 = base64::engine::general_purpose::STANDARD.encode(signal_identity.secret);
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (&format!("signal_identity_secret:{}", from_onion), &signal_secret_b64),
    ).map_err(|e| error::ChattorError::Database(
        format!("Failed to store Signal identity secret: {}", e)
    ))?;

    // Add friend to database
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Mark request as accepted FIRST (so UI updates immediately)
    conn.execute(
        "UPDATE friend_requests SET status = 'accepted' WHERE id = ?1",
        [request_id],
    ).map_err(|e| error::ChattorError::Database(format!("Failed to update request: {}", e)))?;

    // Use a truncated display name that's more readable
    let display_name = if from_onion.len() > 16 {
        format!("{}…", &from_onion[..16])
    } else {
        from_onion.clone()
    };

    conn.execute(
        "INSERT OR IGNORE INTO friends (onion_address, display_name, added_at, status)
         VALUES (?1, ?2, ?3, 'active')",
        (
            &from_onion,
            &display_name,
            timestamp,
        ),
    ).map_err(|e| error::ChattorError::Database(format!("Failed to add friend: {}", e)))?;

    // Auto-subscribe to their channels
    db::queries::add_channel_subscription(&app.db, &from_onion, "public")?;
    db::queries::add_channel_subscription(&app.db, &from_onion, "friends_only")?;

    // Queue the accept message for background delivery (don't try direct send —
    // it can block the UI for up to 30s waiting for a Tor circuit)
    let message = protocol::message::Message::FriendRequestAccept(accept_msg);
    app.message_queue.enqueue(&app.db, &from_onion, &message, "high")?;
    tracing::info!(request_id, "friend request accepted (queued for delivery)");

    Ok(())
}

/// Handle rejecting a friend request
fn handle_reject_friend_request(app: &App, request_id: i64) -> Result<()> {
    // Simply delete the request from the database
    let conn = app.db.connection();

    let rows_affected = conn.execute(
        "DELETE FROM friend_requests WHERE id = ?1",
        [request_id],
    ).map_err(|e| error::ChattorError::Database(format!("Failed to delete request: {}", e)))?;

    if rows_affected == 0 {
        tracing::warn!(request_id, "friend request not found");
    } else {
        tracing::info!(request_id, "friend request rejected");
    }

    Ok(())
}

/// Result of attempting to send a message
pub enum SendResult {
    SentImmediately,
    Queued,
}

/// Try to send a message directly to peer via the connection pool
async fn try_send_direct(
    app: &App,
    peer_onion: &str,
    message: &protocol::message::Message,
) -> Result<()> {
    let pool = app.connection_pool.as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Connection pool not initialized".into()))?;

    pool.send(peer_onion, message).await
}

/// Handle an incoming message from the listener
async fn handle_incoming_message(app: &App, incoming: net::listener::IncomingMessage, presence: &presence::PresenceMap) -> Result<()> {
    match &incoming.message {
        protocol::message::Message::FriendRequest(req) => {
            // Verify the Ed25519 signature before persisting anything. The
            // pubkey is derived from the sender's v3 .onion address, so a
            // forged from_onion (or any tampering with friend_code/timestamp)
            // fails verification.
            match protocol::friend_request::FriendRequestHandler::validate_request(req) {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(
                        from_onion = %req.from_onion,
                        "rejected friend request: invalid signature or stale timestamp"
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(
                        from_onion = %req.from_onion,
                        error = %e,
                        "rejected friend request: validation error"
                    );
                    return Ok(());
                }
            }

            // Store incoming friend request in database
            let conn = app.db.connection();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO friend_requests (from_onion, friend_code, received_at, status)
                 VALUES (?1, ?2, ?3, 'pending')",
                (&req.from_onion, &req.from_friendcode, now),
            ).map_err(|e| error::ChattorError::Database(
                format!("Failed to save friend request: {}", e)
            ))?;

            tracing::info!(from_onion = %req.from_onion, "received friend request");
        }
        protocol::message::Message::FriendRequestAccept(accept) => {
            handle_incoming_accept(app, accept)?;
        }
        protocol::message::Message::FriendRequestReject(reject) => {
            tracing::info!(from_onion = %reject.from_onion, "friend request rejected by peer");
        }
        protocol::message::Message::TextMessage(text_msg) => {
            let from_onion = &text_msg.from_onion;
            let msg_id = text_msg.message_id.to_string();
            let is_prekey = text_msg.signal_type == protocol::message::SignalMessageType::PrekeyMessage;

            // Decode header and ciphertext from wire format
            let store = crypto::SessionStore::new(&app.db);
            let header = base64::engine::general_purpose::STANDARD.decode(&text_msg.signal_header)
                .map_err(|e| error::ChattorError::Crypto(
                    format!("Failed to decode header: {}", e)
                ))?;
            let ciphertext = base64::engine::general_purpose::STANDARD.decode(&text_msg.signal_ciphertext)
                .map_err(|e| error::ChattorError::Crypto(
                    format!("Failed to decode ciphertext: {}", e)
                ))?;

            let payload = match store.load_session(from_onion)? {
                Some(mut session) => {
                    let plaintext = session.decrypt(&header, &ciphertext)?;
                    store.store_session(&session)?;
                    serde_json::from_slice::<protocol::message::PlaintextPayload>(&plaintext)
                        .map_err(|e| error::ChattorError::Crypto(
                            format!("Failed to parse payload: {}", e)
                        ))?
                }
                None if is_prekey => {
                    // No session yet — create one from stored PreKey private material.
                    // This happens when we accepted a friend request (stored our private
                    // keys) and the peer sends their first message as a PreKey message.

                    // Extract X3DH init data from the message
                    let x3dh_init = text_msg.x3dh_init.as_ref()
                        .ok_or_else(|| error::ChattorError::Crypto(
                            format!("PreKey message from {} missing X3DH init data", from_onion)
                        ))?;

                    let alice_identity_bytes = base64::engine::general_purpose::STANDARD
                        .decode(&x3dh_init.sender_identity_key)
                        .map_err(|e| error::ChattorError::Crypto(
                            format!("Failed to decode sender identity key: {}", e)
                        ))?;
                    let alice_identity_public: [u8; 33] = alice_identity_bytes.try_into()
                        .map_err(|_| error::ChattorError::Crypto(
                            "Sender identity key has wrong length (expected 33)".into()
                        ))?;

                    let alice_ephemeral_bytes = base64::engine::general_purpose::STANDARD
                        .decode(&x3dh_init.sender_ephemeral_key)
                        .map_err(|e| error::ChattorError::Crypto(
                            format!("Failed to decode sender ephemeral key: {}", e)
                        ))?;
                    let alice_ephemeral_public: [u8; 33] = alice_ephemeral_bytes.try_into()
                        .map_err(|_| error::ChattorError::Crypto(
                            "Sender ephemeral key has wrong length (expected 33)".into()
                        ))?;

                    // Load all stored PreKey private material
                    let conn = app.db.connection();
                    let identity_b64: String = conn.query_row(
                        "SELECT value FROM app_settings WHERE key = ?1",
                        [&format!("prekey_identity:{}", from_onion)],
                        |row| row.get(0),
                    ).map_err(|_| error::ChattorError::Crypto(
                        format!("No stored PreKey identity material for {}", from_onion)
                    ))?;
                    let spk_b64: String = conn.query_row(
                        "SELECT value FROM app_settings WHERE key = ?1",
                        [&format!("prekey_spk:{}", from_onion)],
                        |row| row.get(0),
                    ).map_err(|_| error::ChattorError::Crypto(
                        format!("No stored PreKey SPK material for {}", from_onion)
                    ))?;
                    let opk_b64: Option<String> = conn.query_row(
                        "SELECT value FROM app_settings WHERE key = ?1",
                        [&format!("prekey_opk:{}", from_onion)],
                        |row| row.get(0),
                    ).ok();

                    let identity_secret: [u8; 32] = base64::engine::general_purpose::STANDARD
                        .decode(&identity_b64)
                        .map_err(|e| error::ChattorError::Crypto(
                            format!("Failed to decode PreKey identity: {}", e)
                        ))?
                        .try_into()
                        .map_err(|_| error::ChattorError::Crypto(
                            "PreKey identity secret has wrong length".into()
                        ))?;
                    let signed_prekey_secret: [u8; 32] = base64::engine::general_purpose::STANDARD
                        .decode(&spk_b64)
                        .map_err(|e| error::ChattorError::Crypto(
                            format!("Failed to decode PreKey SPK: {}", e)
                        ))?
                        .try_into()
                        .map_err(|_| error::ChattorError::Crypto(
                            "PreKey SPK secret has wrong length".into()
                        ))?;
                    // OPK is optional. If a stored value is present but
                    // malformed, treat as an error rather than panicking —
                    // these bytes come from disk after a peer-initiated
                    // handshake and can be corrupted.
                    let prekey_secret: Option<[u8; 32]> = match opk_b64 {
                        Some(b64) => {
                            let bytes = base64::engine::general_purpose::STANDARD
                                .decode(&b64)
                                .map_err(|e| error::ChattorError::Crypto(
                                    format!("Failed to decode PreKey OPK: {}", e)
                                ))?;
                            let arr: [u8; 32] = bytes.try_into().map_err(|_| {
                                error::ChattorError::Crypto(
                                    "PreKey OPK has wrong length (expected 32)".into()
                                )
                            })?;
                            Some(arr)
                        }
                        None => None,
                    };

                    let private_material = crypto::PreKeyPrivateMaterial {
                        identity_secret,
                        signed_prekey_secret,
                        prekey_secret,
                    };

                    let (mut session, _ad) = crypto::SignalSession::from_prekey_message_real(
                        from_onion.clone(),
                        &private_material,
                        &alice_identity_public,
                        &alice_ephemeral_public,
                    )?;

                    let plaintext = session.decrypt(&header, &ciphertext)?;
                    store.store_session(&session)?;

                    // Clean up stored PreKey material (session is now established)
                    conn.execute(
                        "DELETE FROM app_settings WHERE key LIKE ?1",
                        [&format!("prekey_%:{}", from_onion)],
                    ).ok();
                    conn.execute(
                        "DELETE FROM app_settings WHERE key = ?1",
                        [&format!("signal_identity_secret:{}", from_onion)],
                    ).ok();

                    serde_json::from_slice::<protocol::message::PlaintextPayload>(&plaintext)
                        .map_err(|e| error::ChattorError::Crypto(
                            format!("Failed to parse payload: {}", e)
                        ))?
                }
                None => {
                    tracing::warn!(from_onion = %from_onion, "no session and not a PreKey message; cannot decrypt");
                    return Ok(());
                }
            };

            // Handshake messages are session-establishment only — don't display
            if payload.message_type == "handshake" {
                tracing::info!(from_onion = %from_onion, "signal session established via handshake");
                return Ok(());
            }

            // Find friend and conversation
            if let Some(friend_id) = db::queries::find_friend_by_onion(&app.db, from_onion)? {
                let conv_id = db::queries::get_or_create_conversation(&app.db, friend_id)?;
                db::queries::store_incoming_message_with_ttl(&app.db, conv_id, from_onion, &payload.content, &msg_id, payload.ephemeral_ttl)?;

                // Desktop notification (best-effort)
                if notifications::is_enabled(&app.db) {
                    let sender_name = db::queries::get_friend_display_name(&app.db, from_onion)
                        .unwrap_or_else(|_| from_onion.to_string());
                    notifications::notify_message(&sender_name);
                }

                // Queue delivery receipt back to sender
                let receipt = protocol::message::DeliveryReceiptMessage {
                    message_id: text_msg.message_id,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                };
                let receipt_msg = protocol::message::Message::DeliveryReceipt(receipt);
                app.message_queue.enqueue(&app.db, from_onion, &receipt_msg, "high")
                    .log_err("enqueue delivery receipt");
            }
        }
        protocol::message::Message::DeliveryReceipt(receipt) => {
            db::queries::update_message_status(&app.db, &receipt.message_id.to_string(), "delivered")
                .log_err("update_message_status('delivered')");
        }
        protocol::message::Message::ReadReceipt(receipt) => {
            db::queries::update_message_status(&app.db, &receipt.message_id.to_string(), "read")
                .log_err("update_message_status('read')");
        }
        protocol::message::Message::ChannelSubscribe(sub) => {
            // Check if subscriber is blocked
            let blocked: bool = app.db.connection().query_row(
                "SELECT COUNT(*) FROM blocked_onions WHERE onion_address = ?1",
                [&sub.subscriber_onion],
                |row| row.get::<_, i64>(0),
            ).map(|c| c > 0).unwrap_or(false);

            if !blocked {
                let channel_type = match sub.channel_type {
                    protocol::message::ChannelType::Public => "public",
                    protocol::message::ChannelType::FriendsOnly => "friends_only",
                };

                // For friends_only, verify they are a friend
                if channel_type == "friends_only"
                    && db::queries::find_friend_by_onion(&app.db, &sub.subscriber_onion)?.is_none()
                {
                    tracing::warn!(subscriber = %sub.subscriber_onion, "rejected friends_only subscription from non-friend");
                    return Ok(());
                }

                db::queries::add_channel_subscriber(&app.db, &sub.subscriber_onion, channel_type)?;
                tracing::info!(channel_type, subscriber = %sub.subscriber_onion, "new channel subscriber");
            }
        }
        protocol::message::Message::ChannelUnsubscribe(unsub) => {
            let channel_type = match unsub.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };
            db::queries::remove_channel_subscriber(&app.db, &unsub.subscriber_onion, channel_type)?;
            tracing::info!(subscriber = %unsub.subscriber_onion, channel_type, "unsubscribed from channel");
        }
        protocol::message::Message::ChannelPost(post) => {
            let channel_type_str = match post.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };

            // Friends-only enforcement: a non-friend publisher cannot push to
            // our friends-only feed view. (Note: this is symmetric with the
            // sync-request check below — both gates close the same hole.)
            if channel_type_str == "friends_only"
                && db::queries::find_friend_by_onion(&app.db, &post.publisher_onion)?.is_none()
            {
                tracing::warn!(
                    publisher = %post.publisher_onion,
                    "dropped friends-only channel post from non-friend"
                );
                return Ok(());
            }

            // Authenticity: Ed25519 signature over post_id||content||created_at
            // must verify against the publisher's onion-derived pubkey.
            if !post.verify_signature() {
                tracing::warn!(
                    publisher = %post.publisher_onion,
                    post_id = %post.post_id,
                    "rejected channel post: invalid signature"
                );
                return Ok(());
            }

            // Store per-publisher, per-channel-type — no more channel_id=0
            // bucket.
            db::queries::store_channel_post(
                &app.db, &post.publisher_onion, channel_type_str,
                &post.content, &post.post_id.to_string(),
                post.created_at, &post.signature,
            )?;

            // Bound foreign feeds the same way as our own (100-post cap).
            db::queries::enforce_channel_retention(
                &app.db, &post.publisher_onion, channel_type_str
            ).ok();

            // Send read receipt back to publisher.
            let receipt = protocol::message::ChannelPostReceiptMessage {
                post_id: post.post_id,
                reader_onion: app.onion_address.clone().unwrap_or_default(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            };
            let receipt_msg = protocol::message::Message::ChannelPostReceipt(receipt);
            app.message_queue.enqueue(&app.db, &post.publisher_onion, &receipt_msg, "low")
                .log_err("enqueue ChannelPostReceipt");
        }
        protocol::message::Message::ChannelSyncRequest(req) => {
            let channel_type_str = match req.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };

            // For friends_only, verify they are a friend
            if channel_type_str == "friends_only"
                && db::queries::find_friend_by_onion(&app.db, &req.subscriber_onion)?.is_none()
            {
                return Ok(());
            }

            let own_onion = app.onion_address.clone().unwrap_or_default();
            let posts = db::queries::get_channel_posts_since(
                &app.db, &own_onion, channel_type_str, req.since_timestamp
            )?;

            let post_messages: Vec<protocol::message::ChannelPostMessage> = posts.into_iter().map(|p| {
                protocol::message::ChannelPostMessage {
                    publisher_onion: own_onion.clone(),
                    channel_type: req.channel_type.clone(),
                    post_id: uuid::Uuid::parse_str(&p.post_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    content: p.content,
                    created_at: p.created_at,
                    signature: p.signature,
                }
            }).collect();

            if !post_messages.is_empty() {
                let response = protocol::message::Message::ChannelSyncResponse(
                    protocol::message::ChannelSyncResponseMessage {
                        publisher_onion: own_onion,
                        channel_type: req.channel_type.clone(),
                        posts: post_messages,
                    }
                );
                app.message_queue.enqueue(&app.db, &req.subscriber_onion, &response, "normal")
                    .log_err("enqueue ChannelSyncResponse");
            }
        }
        protocol::message::Message::ChannelSyncResponse(resp) => {
            let channel_type_str = match resp.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };

            // The envelope-level publisher_onion identifies whose feed this
            // batch belongs to. Friends-only feeds shouldn't reach us from
            // non-friends; drop the whole batch if so.
            if channel_type_str == "friends_only"
                && db::queries::find_friend_by_onion(&app.db, &resp.publisher_onion)?.is_none()
            {
                tracing::warn!(
                    publisher = %resp.publisher_onion,
                    "dropped friends-only sync response from non-friend"
                );
                return Ok(());
            }

            for post in &resp.posts {
                // The wrapped post must claim the same publisher (defense
                // against a friend forwarding posts attributed to another
                // user), AND its signature must verify under that publisher's
                // pubkey. Individual bad posts are skipped; valid ones still
                // land.
                if post.publisher_onion != resp.publisher_onion {
                    tracing::warn!(
                        envelope = %resp.publisher_onion,
                        claimed = %post.publisher_onion,
                        "dropped post: publisher mismatch in sync response"
                    );
                    continue;
                }
                if !post.verify_signature() {
                    tracing::warn!(
                        publisher = %post.publisher_onion,
                        post_id = %post.post_id,
                        "dropped post: invalid signature in sync response"
                    );
                    continue;
                }
                db::queries::store_channel_post(
                    &app.db, &post.publisher_onion, channel_type_str,
                    &post.content, &post.post_id.to_string(),
                    post.created_at, &post.signature,
                )?;
            }

            // Bound the publisher's feed and update the sync watermark.
            db::queries::enforce_channel_retention(
                &app.db, &resp.publisher_onion, channel_type_str
            ).ok();

            let max_time = resp.posts.iter().map(|p| p.created_at).max().unwrap_or(0);
            if max_time > 0 {
                db::queries::update_subscription_sync_time(
                    &app.db, &resp.publisher_onion, channel_type_str, max_time
                )?;
            }
        }
        protocol::message::Message::ChannelPostReceipt(receipt) => {
            db::queries::store_channel_post_receipt(
                &app.db, &receipt.post_id.to_string(), &receipt.reader_onion, receipt.timestamp
            )?;
        }
        protocol::message::Message::Presence(pres) => {
            match pres.presence_type {
                protocol::message::PresenceType::Heartbeat => {
                    presence::record_heartbeat(presence, &pres.from_onion).await;
                }
                protocol::message::PresenceType::TypingStarted => {
                    presence::record_typing_started(presence, &pres.from_onion).await;
                }
                protocol::message::PresenceType::TypingStopped => {
                    presence::record_typing_stopped(presence, &pres.from_onion).await;
                }
            }
        }
    }

    Ok(())
}

/// Process pending messages in the queue with per-peer concurrency
async fn process_message_queue(app: &App) -> Result<()> {
    use std::collections::HashMap;
    use tokio::task::JoinSet;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let pending = app.message_queue.get_pending_messages(&app.db, now)?;

    if pending.is_empty() {
        return Ok(());
    }

    // Group messages by peer
    let mut by_peer: HashMap<String, Vec<net::queue::QueuedMessage>> = HashMap::new();
    for msg in pending {
        by_peer.entry(msg.peer_onion.clone()).or_default().push(msg);
    }

    let pool = app.connection_pool.as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Connection pool not initialized".into()))?;
    let pool = Arc::clone(pool);

    // Semaphore limits concurrent peer tasks to 10
    let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
    let mut join_set = JoinSet::new();

    for (peer_onion, messages) in by_peer {
        let pool = Arc::clone(&pool);
        let sem = Arc::clone(&semaphore);

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let mut results: Vec<(i64, i64, i64, bool)> = Vec::new(); // (id, created_at, retry_count, success)

            for queued in messages {
                let success = pool.send(&peer_onion, &queued.message).await.is_ok();
                results.push((queued.id, queued.created_at, queued.retry_count, success));

                if !success {
                    break;
                }
            }

            results
        });
    }

    // Collect results and update DB
    while let Some(result) = join_set.join_next().await {
        if let Ok(outcomes) = result {
            for (id, created_at, retry_count, success) in outcomes {
                if success {
                    app.message_queue.mark_delivered(&app.db, id)?;
                } else {
                    match net::queue::compute_next_retry(retry_count, created_at, now) {
                        Some(next) => {
                            app.message_queue.schedule_retry(&app.db, id, next)?;
                        }
                        None => {
                            app.message_queue.mark_failed(&app.db, id)?;
                            tracing::warn!(message_id = id, "message expired after 24h");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Handle incoming friend request accept
fn handle_incoming_accept(
    app: &App,
    accept: &protocol::message::FriendRequestAcceptMessage,
) -> Result<()> {
    use crate::crypto::{PreKeyBundle, PreKeyPrivateMaterial, SignalSession, SessionStore};

    // Deserialize the remote peer's PreKey bundle from the accept message
    let bundle: PreKeyBundle = serde_json::from_str(&accept.signal_prekey_bundle)
        .map_err(|e| error::ChattorError::Crypto(
            format!("Failed to parse PreKey bundle: {}", e)
        ))?;

    // Load our Signal identity secret that was stored when we sent the friend request.
    // We are the original requester; the acceptor sent us their PreKey bundle.
    // We need our Signal identity to perform X3DH as initiator.
    //
    // When we sent the friend request, we didn't have the peer's .onion yet.
    // But when we ACCEPTED a friend request from them (handle_accept_friend_request),
    // we stored signal_identity_secret:<peer_onion>. However, in the case where
    // we are the REQUESTER receiving an accept, we need to generate a new Signal
    // identity now (the requester didn't pre-store one because the accept contains the bundle).
    let signal_identity_secret: [u8; 32] = {
        // Check if we stored a signal identity secret for this peer
        let key = format!("signal_identity_secret:{}", accept.from_onion);
        match app.db.connection().query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [&key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(b64) => {
                let bytes = base64::engine::general_purpose::STANDARD.decode(&b64)
                    .map_err(|e| error::ChattorError::Crypto(
                        format!("Failed to decode stored Signal identity secret: {}", e)
                    ))?;
                bytes.try_into().map_err(|_| error::ChattorError::Crypto(
                    "Stored Signal identity secret has wrong length".into()
                ))?
            }
            Err(_) => {
                // Generate a fresh Signal identity for this X3DH exchange
                let kp = libsignal_protocol::vxeddsa::gen_keypair();
                kp.secret
            }
        }
    };

    let _identity = app.identity.as_ref().expect("identity set during init");
    let dummy_private = PreKeyPrivateMaterial {
        identity_secret: [0u8; 32],
        signed_prekey_secret: [0u8; 32],
        prekey_secret: None,
    };
    let (session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
        accept.from_onion.clone(),
        &bundle,
        &dummy_private,
        &signal_identity_secret,
    )?;

    // Compute our identity public key for the X3DH init data
    let our_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&signal_identity_secret);

    // Store session
    let store = SessionStore::new(&app.db);
    store.store_session(&session)?;

    // Queue a handshake PreKey message to trigger the peer's session creation.
    // Without this, the acceptor can't send messages because they deferred
    // session creation until our first PreKey message arrives.
    let own_onion = app.onion_address.as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized".into()))?;
    {
        let mut session = store.load_session(&accept.from_onion)?
            .ok_or_else(|| error::ChattorError::Crypto("Session just stored but not found".into()))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let handshake = protocol::message::PlaintextPayload {
            content: String::new(),
            sent_at: now,
            message_type: "handshake".to_string(),
            ephemeral_ttl: None,
        };
        let plaintext = serde_json::to_vec(&handshake)
            .map_err(|e| error::ChattorError::Crypto(format!("Handshake serialize: {}", e)))?;

        let (header, ciphertext, is_prekey) = session.encrypt(&plaintext)?;
        store.store_session(&session)?; // persist updated ratchet state

        // Build X3DH init data for the PreKey message so Bob can run x3dh_responder
        let x3dh_init = if is_prekey {
            Some(protocol::message::X3DHInitData {
                sender_identity_key: base64::engine::general_purpose::STANDARD.encode(our_identity_encoded),
                sender_ephemeral_key: base64::engine::general_purpose::STANDARD.encode(ephemeral_public),
            })
        } else {
            None
        };

        let handshake_msg = protocol::message::Message::TextMessage(protocol::message::TextMessage {
            from_onion: own_onion.clone(),
            to_onion: accept.from_onion.clone(),
            signal_header: base64::engine::general_purpose::STANDARD.encode(&header),
            signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
            signal_type: if is_prekey {
                protocol::message::SignalMessageType::PrekeyMessage
            } else {
                protocol::message::SignalMessageType::Message
            },
            timestamp: now,
            message_id: uuid::Uuid::new_v4(),
            x3dh_init,
        });

        app.message_queue.enqueue(&app.db, &accept.from_onion, &handshake_msg, "high")?;
        tracing::info!(peer = %accept.from_onion, "queued handshake PreKey message");
    }

    // Add as friend
    let conn = app.db.connection();
    conn.execute(
        "INSERT OR IGNORE INTO friends (onion_address, display_name, added_at, status)
         VALUES (?1, ?2, ?3, 'active')",
        (
            &accept.from_onion,
            &if accept.from_onion.len() > 16 { format!("{}…", &accept.from_onion[..16]) } else { accept.from_onion.clone() },
            accept.timestamp,
        ),
    ).map_err(|e| error::ChattorError::Database(
        format!("Failed to add friend: {}", e)
    ))?;

    // Auto-subscribe to their channels
    db::queries::add_channel_subscription(&app.db, &accept.from_onion, "public")?;
    db::queries::add_channel_subscription(&app.db, &accept.from_onion, "friends_only")?;

    // Also subscribe them to our friends_only channel
    db::queries::add_channel_subscriber(&app.db, &accept.from_onion, "friends_only")?;

    tracing::info!(from_onion = %accept.from_onion, "friend request accepted by peer");

    Ok(())
}

/// Collect channel sync requests (synchronous, safe to call under lock)
fn collect_sync_requests(app: &App) -> Result<Vec<(String, protocol::message::Message)>> {
    let subscriptions = db::queries::get_channel_subscriptions(&app.db)?;
    let own_onion = app.onion_address.clone().unwrap_or_default();

    let mut requests = Vec::new();
    for sub in subscriptions {
        let since = sub.last_sync_at.unwrap_or(0);
        let channel_type = if sub.channel_type == "public" {
            protocol::message::ChannelType::Public
        } else {
            protocol::message::ChannelType::FriendsOnly
        };

        let sync_req = protocol::message::Message::ChannelSyncRequest(
            protocol::message::ChannelSyncRequestMessage {
                subscriber_onion: own_onion.clone(),
                channel_type,
                since_timestamp: since,
            }
        );

        requests.push((sub.publisher_onion, sync_req));
    }

    Ok(requests)
}

