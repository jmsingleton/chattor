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

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    // Initialize application
    let app = App::new()?;

    // TODO: Initialize Tor in background
    // Currently commented out because we need `app` in the main thread for UI
    // In the future, wrap App in Arc<Mutex<App>> or Arc<RwLock<App>> for sharing
    // tokio::spawn(async move {
    //     println!("Bootstrapping Tor connection...");
    //     match app.init_tor().await {
    //         Ok(_) => println!("Tor connected!"),
    //         Err(e) => eprintln!("Tor initialization failed: {}", e),
    //     }
    // });

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
        // Render current state
        if let Err(e) = terminal.draw(|f| {
            ui::render_app(f, &app_state, &app);
        }) {
            break Err(e.into());
        }

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
