use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::io;
use crate::error::Result;
use crate::app::App;
use super::{AppState, render_add_friend_modal, render_friend_request_modal, render_identity_modal};

pub struct AppUI {
    should_quit: bool,
}

impl AppUI {
    pub fn new() -> Self {
        AppUI {
            should_quit: false,
        }
    }

    fn tor_status(&self) -> &str {
        // TODO: Get actual status from app state
        // For now, return stub status
        "Not Connected"
    }

    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Run main loop
        let result = self.main_loop(&mut terminal);

        // Cleanup terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    fn main_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            terminal.draw(|f| self.render(f))?;

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => self.should_quit = true,
                        KeyCode::Esc => self.should_quit = true,
                        _ => {}
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn render(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.size());

        // Header
        let header_text = format!(
            "torrent-chat v0.1.0  [Tor: {}]",
            self.tor_status()
        );
        let header = Paragraph::new(header_text)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(header, chunks[0]);

        // Main area
        let main = Paragraph::new("Welcome to torrent-chat! (Phase 2: Core Foundation)\n\nPress '?' for help")
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Welcome"));
        f.render_widget(main, chunks[1]);

        // Footer
        let footer = Paragraph::new("Press 'q' to quit | Phase 2: Core Foundation + Tor Integration")
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(footer, chunks[2]);
    }
}

/// Render the application UI based on current state
pub fn render_app(f: &mut Frame, app_state: &AppState, app: &App) {
    // Base layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Header - show Tor status
    let tor_status = if app.tor_client.is_some() {
        "Connected"
    } else {
        "Not Connected"
    };
    let header_text = format!("torrent-chat v0.1.0  [Tor: {}]", tor_status);
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Main area - base UI
    let main_text = match app_state {
        AppState::Normal => {
            "Welcome to torrent-chat!\n\n\
             Press 'i' to view your identity\n\
             Press 'a' to add a friend\n\
             Press 'q' to quit"
        }
        _ => "Welcome to torrent-chat!",
    };
    let main = Paragraph::new(main_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Welcome"));
    f.render_widget(main, chunks[1]);

    // Footer - show keyboard shortcuts based on state
    let footer_text = match app_state {
        AppState::Normal => "[i] My Identity | [a] Add Friend | [q] Quit",
        AppState::AddingFriend { .. } => "Enter friend code | [Enter] Send | [Esc] Cancel",
        AppState::ViewingFriendRequest { .. } => "[A]ccept | [R]eject | [Esc] Back",
        AppState::ViewingMyIdentity { .. } => "[i/Esc] Close identity",
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);

    // Render modals on top if in modal state
    match app_state {
        AppState::AddingFriend { input, error, .. } => {
            render_add_friend_modal(f, input, error.as_deref());
        }
        AppState::ViewingFriendRequest { from_onion, friend_code, .. } => {
            render_friend_request_modal(f, from_onion, friend_code);
        }
        AppState::ViewingMyIdentity { friend_code, onion_address } => {
            render_identity_modal(f, friend_code, onion_address);
        }
        AppState::Normal => {}
    }
}
