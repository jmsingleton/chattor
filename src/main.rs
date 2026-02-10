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
use ui::{AppState, AppAction};
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
        // Lock app for rendering (need to do this outside the draw closure since it's async)
        let app_lock = app.lock().await;

        // Render current state
        if let Err(e) = terminal.draw(|f| {
            ui::render_app(f, &app_state, &*app_lock);
        }) {
            drop(app_lock); // Release lock before breaking
            break Err(e.into());
        }

        // Release lock before polling events
        drop(app_lock);

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app_state.handle_key(key)? {
                    Some(AppAction::SendFriendRequest(code)) => {
                        // Lock app to send friend request
                        let app_lock = app.lock().await;

                        match handle_send_friend_request(&*app_lock, &code).await {
                            Ok(_) => {
                                // Success - return to normal
                                app_state = AppState::Normal;
                            }
                            Err(e) => {
                                // Show error in the modal
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
                            Ok(_) => app_state = AppState::Normal,
                            Err(e) => {
                                eprintln!("Failed to accept friend request: {}", e);
                                app_state = AppState::Normal;
                            }
                        }
                        drop(app_lock);
                    }
                    Some(AppAction::RejectFriendRequest(id)) => {
                        // Lock app to reject friend request
                        let app_lock = app.lock().await;

                        match handle_reject_friend_request(&*app_lock, id) {
                            Ok(_) => app_state = AppState::Normal,
                            Err(e) => {
                                eprintln!("Failed to reject friend request: {}", e);
                                app_state = AppState::Normal;
                            }
                        }
                        drop(app_lock);
                    }
                    Some(AppAction::Quit) => break Ok(()),
                    None => {} // Just state change
                }
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
async fn handle_send_friend_request(app: &App, friend_code: &str) -> Result<()> {
    use crate::protocol::friend_code::validate_friend_code;
    use crate::protocol::friend_request::FriendRequestHandler;

    // Validate friend code format
    validate_friend_code(friend_code)?;

    // Get our .onion address (need Tor to be initialized)
    let own_onion = app.onion_address.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Tor not initialized yet".into()))?;

    // Create the friend request message (static method, doesn't need handler instance)
    let _request_msg = FriendRequestHandler::create_request(
        &app.identity,
        own_onion,
        friend_code,
    )?;

    // TODO: Send the request over Tor connection to the peer
    // For MVP, we'll just pretend it was sent successfully
    // In production:
    // 1. Convert friend_code to .onion address
    // 2. Connect to peer via Tor
    // 3. Send the friend request message
    // 4. Wait for response or queue for later delivery

    // For now, just show success
    eprintln!("Friend request created for {}", friend_code);
    eprintln!("(TODO: Actually send over Tor connection)");

    Ok(())
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

    let _accept_msg = protocol::message::FriendRequestAcceptMessage {
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
            &from_onion[..10], // Use first 10 chars as name
            timestamp,
        ),
    ).map_err(|e| error::TorrentChatError::Database(format!("Failed to add friend: {}", e)))?;

    // Mark request as accepted
    conn.execute(
        "UPDATE friend_requests SET status = 'accepted' WHERE id = ?1",
        [request_id],
    ).map_err(|e| error::TorrentChatError::Database(format!("Failed to update request: {}", e)))?;

    // TODO: Send the accept message over Tor connection to the peer
    // For MVP, we'll just pretend it was sent successfully

    eprintln!("Friend request #{} accepted", request_id);
    eprintln!("Session established with {}", from_onion);
    eprintln!("(TODO: Actually send accept message over Tor)");

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
