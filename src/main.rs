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

        // Release lock before rendering
        drop(app_lock);

        let ctx = RenderContext {
            friends,
            messages,
            own_onion,
            friend_code,
            tor_connected,
            pending_request_count,
            conversation_ephemeral_ttl,
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
                            let return_to_list = matches!(app_state, AppState::ViewingFriendRequests { .. });
                            let app_lock = app.lock().await;

                            match handle_accept_friend_request(&*app_lock, id).await {
                                Ok(_) => {}
                                Err(e) => eprintln!("Failed to accept friend request: {}", e),
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

                            match handle_reject_friend_request(&*app_lock, id) {
                                Ok(_) => {}
                                Err(e) => eprintln!("Failed to reject friend request: {}", e),
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
                                    db::queries::activate_ephemeral_timers(&app_lock.db, conv_id).ok();

                                    // Send read receipts for unread messages from peer
                                    let own_onion = app_lock.onion_address.clone().unwrap_or_default();
                                    if let Ok(unreceipted) = db::queries::get_unreceipted_message_ids(&app_lock.db, conv_id, &own_onion) {
                                        for (msg_id, sender_onion) in &unreceipted {
                                            if let Ok(uuid) = uuid::Uuid::parse_str(msg_id) {
                                                let receipt = protocol::message::DeliveryReceiptMessage {
                                                    message_id: uuid,
                                                    timestamp: std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap()
                                                        .as_secs() as i64,
                                                };
                                                let receipt_msg = protocol::message::Message::ReadReceipt(receipt);
                                                app_lock.message_queue.enqueue(&app_lock.db, sender_onion, &receipt_msg, "low").ok();
                                            }
                                            // Mark the message as read locally
                                            db::queries::update_message_status(&app_lock.db, msg_id, "read").ok();
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
                                                            Ok((ciphertext, is_prekey)) => {
                                                                store.store_session(&session).ok();
                                                                Some(protocol::message::TextMessage {
                                                                    from_onion: own_onion.clone(),
                                                                    to_onion: peer_onion.clone(),
                                                                    signal_ciphertext: base64::encode(&ciphertext),
                                                                    signal_type: if is_prekey {
                                                                        protocol::message::SignalMessageType::PrekeyMessage
                                                                    } else {
                                                                        protocol::message::SignalMessageType::Message
                                                                    },
                                                                    timestamp: payload.sent_at,
                                                                    message_id: uuid::Uuid::parse_str(&msg_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
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
                                        match try_send_direct(&*app_lock, &peer_onion, &msg).await {
                                            Ok(_) => {
                                                db::queries::update_message_status(&app_lock.db, &msg_id, "sent").ok();
                                            }
                                            Err(_) => {
                                                app_lock.message_queue.enqueue(&app_lock.db, &peer_onion, &msg, "normal").ok();
                                                db::queries::update_message_status(&app_lock.db, &msg_id, "queued").ok();
                                            }
                                        }
                                    } else {
                                        eprintln!("Failed to encrypt message: no session for {}", peer_onion);
                                        db::queries::update_message_status(&app_lock.db, &msg_id, "failed").ok();
                                    }
                                }
                            }

                            drop(app_lock);
                        }
                        Some(AppAction::SetEphemeralTtl(conv_id, ttl)) => {
                            let app_lock = app.lock().await;
                            db::queries::set_conversation_ephemeral_ttl(&app_lock.db, conv_id, ttl).ok();
                            drop(app_lock);
                        }
                        Some(AppAction::Quit) => break Ok(()),
                        None => {} // Just state change
                    }
                }
                Event::Mouse(mouse_event) => {
                    use crossterm::event::{MouseEventKind, MouseButton};
                    if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
                        let app_lock = app.lock().await;
                        // Check if click is in setup wizard area (when no friends)
                        let friends = db::queries::get_friends_with_unread(&app_lock.db).unwrap_or_default();
                        if friends.is_empty() {
                            if let Some(ref onion) = app_lock.onion_address {
                                // Rough check: click in the identity box area
                                let row = mouse_event.row;
                                let term_height = terminal.size().map(|s| s.height).unwrap_or(24);
                                let wizard_start = term_height / 4;

                                if row >= wizard_start + 4 && row <= wizard_start + 6 {
                                    // Onion address area
                                    ui::copy_to_clipboard(onion);
                                } else if row >= wizard_start + 7 && row <= wizard_start + 9 {
                                    // Friend code area
                                    let code = crate::tor::address::onion_to_friend_code(onion)
                                        .unwrap_or_default();
                                    if !code.is_empty() {
                                        ui::copy_to_clipboard(&code);
                                    }
                                }
                            }
                        }
                        drop(app_lock);
                    }
                }
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
    let (bundle, private_keys) = PreKeyBundle::generate_real(&app.identity)?;

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

    // Initialize Signal session with real X3DH key exchange
    let session = SignalSession::from_prekey_bundle_real(
        from_onion.clone(),
        &bundle,
        &private_keys,
        &app.identity,
    )?;

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
            let msg_id = text_msg.message_id.to_string();

            // Decrypt the message using Signal session
            let store = crypto::SessionStore::new(&app.db);
            let (content, ephemeral_ttl) = match store.load_session(from_onion)? {
                Some(mut session) => {
                    let ciphertext = base64::decode(&text_msg.signal_ciphertext)
                        .map_err(|e| error::TorrentChatError::Crypto(
                            format!("Failed to decode base64: {}", e)
                        ))?;
                    let plaintext = session.decrypt(&ciphertext)?;
                    store.store_session(&session)?;
                    let payload: protocol::message::PlaintextPayload = serde_json::from_slice(&plaintext)
                        .map_err(|e| error::TorrentChatError::Crypto(
                            format!("Failed to parse payload: {}", e)
                        ))?;
                    (payload.content, payload.ephemeral_ttl)
                }
                None => {
                    eprintln!("No session for {}, cannot decrypt", from_onion);
                    return Ok(());
                }
            };

            // Find friend and conversation
            if let Some(friend_id) = db::queries::find_friend_by_onion(&app.db, from_onion)? {
                let conv_id = db::queries::get_or_create_conversation(&app.db, friend_id)?;
                db::queries::store_incoming_message_with_ttl(&app.db, conv_id, from_onion, &content, &msg_id, ephemeral_ttl)?;

                // Queue delivery receipt back to sender
                let receipt = protocol::message::DeliveryReceiptMessage {
                    message_id: text_msg.message_id,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                };
                let receipt_msg = protocol::message::Message::DeliveryReceipt(receipt);
                app.message_queue.enqueue(&app.db, from_onion, &receipt_msg, "high").ok();
            }
        }
        protocol::message::Message::DeliveryReceipt(receipt) => {
            db::queries::update_message_status(&app.db, &receipt.message_id.to_string(), "delivered").ok();
        }
        protocol::message::Message::ReadReceipt(receipt) => {
            db::queries::update_message_status(&app.db, &receipt.message_id.to_string(), "read").ok();
        }
        // Channel messages will be handled in a future task
        protocol::message::Message::ChannelSubscribe(_)
        | protocol::message::Message::ChannelUnsubscribe(_)
        | protocol::message::Message::ChannelPost(_)
        | protocol::message::Message::ChannelSyncRequest(_)
        | protocol::message::Message::ChannelSyncResponse(_)
        | protocol::message::Message::ChannelPostReceipt(_) => {
            eprintln!("Received channel message (not yet handled)");
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

    // Generate our own key material for X3DH
    let (_, local_private) = PreKeyBundle::generate_real(&app.identity)?;

    // Initialize Signal session with real X3DH key exchange
    let session = SignalSession::from_prekey_bundle_real(
        accept.from_onion.clone(),
        &bundle,
        &local_private,
        &app.identity,
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
