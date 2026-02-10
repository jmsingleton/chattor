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
                    Some(AppAction::SendFriendRequest(_code)) => {
                        // TODO: Implement send_friend_request
                        // For now, just return to normal
                        app_state = AppState::Normal;
                    }
                    Some(AppAction::AcceptFriendRequest(_id)) => {
                        // TODO: Implement accept_friend_request
                        app_state = AppState::Normal;
                    }
                    Some(AppAction::RejectFriendRequest(_id)) => {
                        // TODO: Implement reject_friend_request
                        app_state = AppState::Normal;
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
