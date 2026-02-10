mod error;
mod app;
mod cli;
mod config;
mod crypto;
mod db;
mod tor;
mod protocol;
mod net;
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
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    // Initialize application wrapped in Arc<Mutex> for sharing between threads
    let app = Arc::new(Mutex::new(App::new()?));

    // Initialize Tor in background
    let app_tor = Arc::clone(&app);
    tokio::spawn(async move {
        let mut app_lock = app_tor.lock().await;
        if let Err(e) = app_lock.init_tor().await {
            eprintln!("Failed to initialize Tor: {}", e);
        }
    });

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize state machine
    let mut app_state = AppState::default();

    // Main event loop
    let result = loop {
        // Lock app to build render context
        let app_lock = app.lock().await;

        let friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
        let own_onion = app_lock.onion_address.clone();
        let friend_code = own_onion.as_deref().and_then(|o| {
            crate::tor::address::onion_to_friend_code(o).ok()
        });
        let tor_connected = app_lock.tor_client.is_some();

        // Load messages for selected conversation
        let messages = if let AppState::Normal { conversation_id: Some(conv_id), .. } = &app_state {
            db::queries::get_messages(&app_lock.db, *conv_id, 100, 0).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Release lock before rendering
        drop(app_lock);

        let ctx = RenderContext {
            friends,
            messages,
            own_onion,
            friend_code,
            tor_connected,
        };

        // Render current state
        if let Err(e) = terminal.draw(|f| {
            ui::render_app(f, &app_state, &ctx);
        }) {
            break Err(e.into());
        }

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app_state.handle_key(key)? {
                    Some(AppAction::SendFriendRequest(code)) => {
                        let app_lock = app.lock().await;

                        match handle_send_friend_request(&*app_lock, &code).await {
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
                        // Lock app to accept friend request
                        let app_lock = app.lock().await;

                        match handle_accept_friend_request(&*app_lock, id).await {
                            Ok(_) => app_state = AppState::default(),
                            Err(e) => {
                                eprintln!("Failed to accept friend request: {}", e);
                                app_state = AppState::default();
                            }
                        }
                        drop(app_lock);
                    }
                    Some(AppAction::RejectFriendRequest(id)) => {
                        // Lock app to reject friend request
                        let app_lock = app.lock().await;

                        match handle_reject_friend_request(&*app_lock, id) {
                            Ok(_) => app_state = AppState::default(),
                            Err(e) => {
                                eprintln!("Failed to reject friend request: {}", e);
                                app_state = AppState::default();
                            }
                        }
                        drop(app_lock);
                    }
                    Some(AppAction::ViewMyIdentity) => {
                        let app_lock = app.lock().await;
                        if let Some(onion) = &app_lock.onion_address {
                            let friend_code = crate::tor::address::onion_to_friend_code(onion)
                                .unwrap_or_else(|_| "unknown".to_string());
                            app_state = AppState::ViewingMyIdentity {
                                friend_code,
                                onion_address: onion.clone(),
                            };
                        } else {
                            // Tor not ready yet - can't show identity
                            app_state = AppState::ViewingMyIdentity {
                                friend_code: "(Waiting for Tor...)".to_string(),
                                onion_address: "(Waiting for Tor...)".to_string(),
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
                                db::queries::mark_conversation_read(&app_lock.db, conv_id).ok();
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

                                // Store locally first
                                db::queries::store_outgoing_message(
                                    &app_lock.db, conv_id, &own_onion, &content, &msg_id
                                ).ok();

                                // Try to send directly, queue on failure
                                match try_send_direct(&*app_lock, &peer_onion, &protocol::message::Message::TextMessage(
                                    protocol::message::TextMessage {
                                        from_onion: own_onion.clone(),
                                        to_onion: peer_onion.clone(),
                                        signal_ciphertext: content.clone(),
                                        signal_type: protocol::message::SignalMessageType::Message,
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs() as i64,
                                        message_id: uuid::Uuid::parse_str(&msg_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                                    }
                                )).await {
                                    Ok(_) => {
                                        db::queries::update_message_status(&app_lock.db, &msg_id, "sent").ok();
                                    }
                                    Err(_) => {
                                        // Queue for later delivery
                                        let text_msg = protocol::message::Message::TextMessage(
                                            protocol::message::TextMessage {
                                                from_onion: own_onion.clone(),
                                                to_onion: peer_onion.clone(),
                                                signal_ciphertext: content,
                                                signal_type: protocol::message::SignalMessageType::Message,
                                                timestamp: std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .unwrap_or_default()
                                                    .as_secs() as i64,
                                                message_id: uuid::Uuid::parse_str(&msg_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                                            }
                                        );
                                        app_lock.message_queue.enqueue(&app_lock.db, &peer_onion, &text_msg, "normal").ok();
                                        db::queries::update_message_status(&app_lock.db, &msg_id, "queued").ok();
                                    }
                                }
                            }
                        }

                        drop(app_lock);
                    }
                    Some(AppAction::Quit) => break Ok(()),
                    None => {} // Just state change
                }
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

            // Process collected messages
            for incoming in incoming_messages {
                if let Err(e) = handle_incoming_message(&*app_lock, incoming) {
                    eprintln!("Failed to handle incoming message: {}", e);
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
            if let Err(e) = process_message_queue(&*app_lock).await {
                eprintln!("Queue processing error: {}", e);
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

/// Handle sending a friend request
async fn handle_send_friend_request(app: &App, peer_input: &str) -> Result<SendResult> {
    use crate::protocol::friend_request::FriendRequestHandler;

    // For MVP: peer_input is a .onion address directly
    // Validate it looks like a .onion address
    let peer_onion = peer_input.trim();
    if !peer_onion.ends_with(".onion") {
        return Err(error::TorrentChatError::Tor(
            "Please enter a .onion address (e.g., abc123.onion)".into()
        ));
    }

    // Get our .onion address
    let own_onion = app.onion_address.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Tor not initialized yet".into()))?;

    // Generate our own friend code to include in the request
    let own_friend_code = crate::tor::address::onion_to_friend_code(own_onion)
        .unwrap_or_else(|_| "unknown".to_string());

    // Create friend request message
    let request_msg = FriendRequestHandler::create_request(
        &app.identity,
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
async fn handle_accept_friend_request(app: &App, request_id: i64) -> Result<()> {
    use crate::protocol::friend_request::FriendRequestHandler;
    use crate::crypto::{PreKeyBundle, SignalSession, SessionStore};

    // Get our .onion address
    let own_onion = app.onion_address.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Tor not initialized yet".into()))?;

    // Get the friend request from database
    let conn = app.db.connection();
    let (from_onion, _friend_code): (String, String) = conn.query_row(
        "SELECT from_onion, friend_code FROM friend_requests WHERE id = ?1",
        [request_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(|e| error::TorrentChatError::Database(format!("Failed to load request: {}", e)))?;

    // Generate PreKey bundle for the accept message
    let (bundle, _private_keys) = PreKeyBundle::generate_real(&app.identity)?;

    // Create accept message (inline to avoid Database clone issue)
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Sign message
    let data = format!("{}{}{}", own_onion, from_onion, timestamp);
    let signature = app.identity.sign(data.as_bytes());

    // Serialize bundle to JSON
    let bundle_json = serde_json::to_string(&bundle)
        .map_err(|e| error::TorrentChatError::Crypto(format!("Failed to serialize bundle: {}", e)))?;

    let accept_msg = protocol::message::FriendRequestAcceptMessage {
        from_onion: own_onion.to_string(),
        to_onion: from_onion.clone(),
        signal_prekey_bundle: bundle_json,
        timestamp,
        signature: format!("{}", base64::encode(&signature.to_bytes())),
    };

    // Initialize Signal session (simplified - in production would use real keys exchange)
    let session = SignalSession::from_prekey_bundle(from_onion.clone(), &bundle)?;

    // Store the session
    let store = SessionStore::new(&app.db);
    store.store_session(&session)?;

    // Add friend to database
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO friends (onion_address, display_name, added_at, status)
         VALUES (?1, ?2, ?3, 'active')",
        (
            &from_onion,
            &from_onion[..std::cmp::min(10, from_onion.len())],
            timestamp,
        ),
    ).map_err(|e| error::TorrentChatError::Database(format!("Failed to add friend: {}", e)))?;

    // Mark request as accepted
    conn.execute(
        "UPDATE friend_requests SET status = 'accepted' WHERE id = ?1",
        [request_id],
    ).map_err(|e| error::TorrentChatError::Database(format!("Failed to update request: {}", e)))?;

    // Send the accept message over Tor (try direct, queue on failure)
    let message = protocol::message::Message::FriendRequestAccept(accept_msg);
    match try_send_direct(app, &from_onion, &message).await {
        Ok(_) => {
            eprintln!("Friend request #{} accepted (sent directly)", request_id);
        }
        Err(_) => {
            app.message_queue.enqueue(&app.db, &from_onion, &message, "high")?;
            eprintln!("Friend request #{} accepted (queued for delivery)", request_id);
        }
    }

    eprintln!("Session established with {}", from_onion);

    Ok(())
}

/// Handle rejecting a friend request
fn handle_reject_friend_request(app: &App, request_id: i64) -> Result<()> {
    // Simply delete the request from the database
    let conn = app.db.connection();

    let rows_affected = conn.execute(
        "DELETE FROM friend_requests WHERE id = ?1",
        [request_id],
    ).map_err(|e| error::TorrentChatError::Database(format!("Failed to delete request: {}", e)))?;

    if rows_affected == 0 {
        eprintln!("Friend request #{} not found", request_id);
    } else {
        eprintln!("Friend request #{} rejected", request_id);
    }

    Ok(())
}

/// Result of attempting to send a message
pub enum SendResult {
    SentImmediately,
    Queued,
}

/// Try to send a message directly to peer via Tor with timeout
async fn try_send_direct(
    app: &App,
    peer_onion: &str,
    message: &protocol::message::Message,
) -> Result<()> {
    let tor_client = app.tor_client.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Tor not initialized".into()))?;

    // Connect with 5-second timeout
    let mut conn = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        crate::tor::connection::TorConnection::connect(tor_client.as_ref(), peer_onion)
    )
    .await
    .map_err(|_| error::TorrentChatError::Network("Connection timed out (5s)".into()))??;

    // Send with 5-second timeout
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        conn.send(message)
    )
    .await
    .map_err(|_| error::TorrentChatError::Network("Send timed out (5s)".into()))??;

    Ok(())
}

/// Handle an incoming message from the listener
fn handle_incoming_message(app: &App, incoming: net::listener::IncomingMessage) -> Result<()> {
    match &incoming.message {
        protocol::message::Message::FriendRequest(req) => {
            // Store incoming friend request in database
            let conn = app.db.connection();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO friend_requests (from_onion, friend_code, received_at, status)
                 VALUES (?1, ?2, ?3, 'pending')",
                (&req.from_onion, &req.from_friendcode, now),
            ).map_err(|e| error::TorrentChatError::Database(
                format!("Failed to save friend request: {}", e)
            ))?;

            eprintln!("Received friend request from {}", req.from_onion);
        }
        protocol::message::Message::FriendRequestAccept(accept) => {
            handle_incoming_accept(app, accept)?;
        }
        protocol::message::Message::FriendRequestReject(reject) => {
            eprintln!("Friend request rejected by {}", reject.from_onion);
        }
        protocol::message::Message::TextMessage(text_msg) => {
            let from_onion = &text_msg.from_onion;
            let content = &text_msg.signal_ciphertext; // MVP: plaintext
            let msg_id = text_msg.message_id.to_string();

            // Find friend and conversation
            if let Some(friend_id) = db::queries::find_friend_by_onion(&app.db, from_onion)? {
                let conv_id = db::queries::get_or_create_conversation(&app.db, friend_id)?;
                db::queries::store_incoming_message(&app.db, conv_id, from_onion, content, &msg_id)?;
            }
        }
        protocol::message::Message::DeliveryReceipt(_) => {
            // Not implemented yet
        }
    }

    Ok(())
}

/// Process pending messages in the queue
async fn process_message_queue(app: &App) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let pending = app.message_queue.get_pending_messages(&app.db, now)?;

    if pending.is_empty() {
        return Ok(());
    }

    for queued in pending {
        match try_send_direct(app, &queued.peer_onion, &queued.message).await {
            Ok(_) => {
                app.message_queue.mark_delivered(&app.db, queued.id)?;
                eprintln!("Queued message #{} delivered to {}", queued.id, queued.peer_onion);
            }
            Err(_) => {
                if queued.retry_count >= 10 {
                    app.message_queue.mark_failed(&app.db, queued.id)?;
                    eprintln!("Message #{} failed after 10 retries", queued.id);
                } else {
                    let next_retry = now + 30;
                    app.message_queue.schedule_retry(&app.db, queued.id, next_retry)?;
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
    use crate::crypto::{PreKeyBundle, SignalSession, SessionStore};

    // Deserialize PreKey bundle
    let bundle: PreKeyBundle = serde_json::from_str(&accept.signal_prekey_bundle)
        .map_err(|e| error::TorrentChatError::Crypto(
            format!("Failed to parse PreKey bundle: {}", e)
        ))?;

    // Initialize Signal session
    let session = SignalSession::from_prekey_bundle(
        accept.from_onion.clone(),
        &bundle,
    )?;

    // Store session
    let store = SessionStore::new(&app.db);
    store.store_session(&session)?;

    // Add as friend
    let conn = app.db.connection();
    conn.execute(
        "INSERT OR IGNORE INTO friends (onion_address, display_name, added_at, status)
         VALUES (?1, ?2, ?3, 'active')",
        (
            &accept.from_onion,
            &accept.from_onion[..std::cmp::min(10, accept.from_onion.len())],
            accept.timestamp,
        ),
    ).map_err(|e| error::TorrentChatError::Database(
        format!("Failed to add friend: {}", e)
    ))?;

    eprintln!("Friend request accepted by {}", accept.from_onion);

    Ok(())
}
