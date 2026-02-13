use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use crate::ui::theme::Theme;

/// Status updates sent from the Tor bootstrap process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapUpdate {
    /// Progress percentage (0-100).
    Progress(u8),
    /// Tor connection established successfully.
    Connected,
    /// Tor connection failed with the given error message.
    Failed(String),
}

/// State machine for the bootstrap/splash screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapPhase {
    /// Actively connecting to the Tor network.
    Connecting {
        progress: u8,
        frame: usize,
        tick: u64,
    },
    /// Connection attempt failed.
    Failed {
        error: String,
        frame: usize,
        tick: u64,
    },
    /// Bootstrap complete, ready to transition to main UI.
    Done,
}

/// Actions produced by the bootstrap key handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapAction {
    /// Retry the Tor connection.
    Retry,
    /// Skip Tor and continue in offline mode.
    ContinueOffline,
    /// Quit the application.
    Quit,
}

impl Default for BootstrapPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl BootstrapPhase {
    /// Create a new bootstrap phase in the initial Connecting state.
    pub fn new() -> Self {
        BootstrapPhase::Connecting {
            progress: 0,
            frame: 0,
            tick: 0,
        }
    }

    /// Advance the animation tick. The frame advances every 3 ticks.
    pub fn advance_tick(&mut self) {
        match self {
            BootstrapPhase::Connecting {
                tick, frame, ..
            } => {
                *tick += 1;
                if *tick % 3 == 0 {
                    *frame += 1;
                }
            }
            BootstrapPhase::Failed {
                tick, frame, ..
            } => {
                *tick += 1;
                if *tick % 3 == 0 {
                    *frame += 1;
                }
            }
            BootstrapPhase::Done => {}
        }
    }

    /// Update the progress percentage. Only effective in the Connecting state.
    pub fn set_progress(&mut self, value: u8) {
        if let BootstrapPhase::Connecting { progress, .. } = self {
            *progress = value;
        }
    }

    /// Transition to the Failed state with the given error message.
    /// Resets frame and tick to 0.
    pub fn fail(&mut self, error: String) {
        *self = BootstrapPhase::Failed {
            error,
            frame: 0,
            tick: 0,
        };
    }

    /// Transition to the Done state.
    pub fn done(&mut self) {
        *self = BootstrapPhase::Done;
    }
}

/// Returns 6 animation frames of Unicode block art showing three onion relay
/// nodes with a signal pulse traveling between them. Each frame is a Vec of
/// string lines, designed for 60-70 chars wide maximum.
pub fn connecting_frames() -> Vec<Vec<&'static str>> {
    vec![
        // Frame 0: "you" mushroom lit, pulse starting
        vec![
            "       ▄██▄                   ▄██▄                   ▄██▄       ",
            "     ▄██▓▓██▄               ▄██░░██▄               ▄██░░██▄     ",
            "    ███▓▓▓▓███             ███░░░░███             ███░░░░███    ",
            "     ▀██▓▓██▀               ▀██░░██▀               ▀██░░██▀     ",
            "       ████                   ████                   ████       ",
            "       █░░█  ══░▒▓═══════════ █░░█ ═════════════════ █░░█       ",
            "       ▀▀▀▀                   ▀▀▀▀                   ▀▀▀▀       ",
            "       you                   relay                   exit       ",
        ],
        // Frame 1: Pulse between "you" and "relay"
        vec![
            "       ▄██▄                   ▄██▄                   ▄██▄       ",
            "     ▄██░░██▄               ▄██░░██▄               ▄██░░██▄     ",
            "    ███░░░░███             ███░░░░███             ███░░░░███    ",
            "     ▀██░░██▀               ▀██░░██▀               ▀██░░██▀     ",
            "       ████                   ████                   ████       ",
            "       █░░█ ═══════░▒▓═══════ █░░█ ═════════════════ █░░█       ",
            "       ▀▀▀▀                   ▀▀▀▀                   ▀▀▀▀       ",
            "       you                   relay                   exit       ",
        ],
        // Frame 2: "relay" lights up
        vec![
            "       ▄██▄                   ▄██▄                   ▄██▄       ",
            "     ▄██░░██▄               ▄██▓▓██▄               ▄██░░██▄     ",
            "    ███░░░░███             ███▓▓▓▓███             ███░░░░███    ",
            "     ▀██░░██▀               ▀██▓▓██▀               ▀██░░██▀     ",
            "       ████                   ████                   ████       ",
            "       █░░█ ═════════════░▒▓═ █░░█ ═════════════════ █░░█       ",
            "       ▀▀▀▀                   ▀▀▀▀                   ▀▀▀▀       ",
            "       you                   relay                   exit       ",
        ],
        // Frame 3: Pulse between "relay" and "exit"
        vec![
            "       ▄██▄                   ▄██▄                   ▄██▄       ",
            "     ▄██░░██▄               ▄██▓▓██▄               ▄██░░██▄     ",
            "    ███░░░░███             ███▓▓▓▓███             ███░░░░███    ",
            "     ▀██░░██▀               ▀██▓▓██▀               ▀██░░██▀     ",
            "       ████                   ████                   ████       ",
            "       █░░█ ═════════════════ █░░█ ═══════░▒▓═══════ █░░█       ",
            "       ▀▀▀▀                   ▀▀▀▀                   ▀▀▀▀       ",
            "       you                   relay                   exit       ",
        ],
        // Frame 4: "exit" lights up
        vec![
            "       ▄██▄                   ▄██▄                   ▄██▄       ",
            "     ▄██░░██▄               ▄██▓▓██▄               ▄██▓▓██▄     ",
            "    ███░░░░███             ███▓▓▓▓███             ███▓▓▓▓███    ",
            "     ▀██░░██▀               ▀██▓▓██▀               ▀██▓▓██▀     ",
            "       ████                   ████                   ████       ",
            "       █░░█ ═════════════════ █░░█ ═════════════░▒▓═ █░░█       ",
            "       ▀▀▀▀                   ▀▀▀▀                   ▀▀▀▀       ",
            "       you                   relay                   exit       ",
        ],
        // Frame 5: All lit (success flash)
        vec![
            "       ▄██▄                   ▄██▄                   ▄██▄       ",
            "     ▄██▓▓██▄               ▄██▓▓██▄               ▄██▓▓██▄     ",
            "    ███▓▓▓▓███             ███▓▓▓▓███             ███▓▓▓▓███    ",
            "     ▀██▓▓██▀               ▀██▓▓██▀               ▀██▓▓██▀     ",
            "       ████                   ████                   ████       ",
            "       █░░█ ═════════════════ █░░█ ═════════════════ █░░█       ",
            "       ▀▀▀▀                   ▀▀▀▀                   ▀▀▀▀       ",
            "       you                   relay                   exit       ",
        ],
    ]
}

/// Returns a single dim onion sprite for the failure screen, using lightest
/// shading to appear "powered down".
pub fn failure_art() -> Vec<&'static str> {
    vec![
        "                  ▄██▄    ",
        "                ▄██░░██▄  ",
        "               ███░░░░███ ",
        "                ▀██░░██▀  ",
        "                  ████    ",
        "                  █░░█    ",
        "                  ▀▀▀▀    ",
    ]
}

/// Returns rotating cheeky status messages shown during Tor bootstrap.
pub fn status_messages() -> Vec<&'static str> {
    vec![
        "Peeling onion layers...",
        "Negotiating with relays...",
        "Building circuits in the dark...",
        "Routing through the underground...",
        "Almost there, patience is a virtue...",
        "Wrapping in layers of encryption...",
    ]
}

/// Render the connecting animation screen.
///
/// Shows the chattor title, ASCII relay animation, and a rotating status
/// message. The `frame` selects which animation frame to display, and `tick`
/// determines which status message to show (cycles every 10 ticks).
pub fn render_connecting(f: &mut Frame, frame: usize, tick: u64, _progress: u8, theme: &Theme) {
    let area = f.size();
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25), // top padding
            Constraint::Length(1),      // title
            Constraint::Length(1),      // spacer
            Constraint::Length(8),      // ASCII art
            Constraint::Length(1),      // spacer
            Constraint::Length(1),      // status message
            Constraint::Min(0),         // bottom fill
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        "chattor",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // ASCII art frame
    let frames = connecting_frames();
    let total_frames = frames.len();
    let current_frame = &frames[frame % total_frames];
    let art_lines: Vec<Line> = current_frame
        .iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(theme.accent))))
        .collect();
    let art = Paragraph::new(art_lines).alignment(Alignment::Center);
    f.render_widget(art, chunks[3]);

    // Rotating status message
    let msgs = status_messages();
    let msg_idx = (tick / 10) as usize % msgs.len();
    let status = Paragraph::new(Line::from(Span::styled(
        msgs[msg_idx],
        Style::default().fg(theme.fg_dim),
    )))
    .alignment(Alignment::Center);
    f.render_widget(status, chunks[5]);
}

/// Render the failure screen.
///
/// Shows the chattor title (dimmed), a sad onion sprite, the error message,
/// troubleshooting tips, and action keys for retry/continue/quit.
pub fn render_failure(f: &mut Frame, error: &str, theme: &Theme) {
    let area = f.size();
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15), // top padding
            Constraint::Length(1),      // title
            Constraint::Length(1),      // spacer
            Constraint::Length(6),      // failure art
            Constraint::Length(1),      // spacer
            Constraint::Length(1),      // "connection failed :("
            Constraint::Length(1),      // spacer
            Constraint::Length(1),      // error detail
            Constraint::Length(1),      // spacer
            Constraint::Length(4),      // troubleshooting tips
            Constraint::Length(1),      // spacer
            Constraint::Length(1),      // docs link
            Constraint::Length(1),      // spacer
            Constraint::Length(1),      // action keys
            Constraint::Min(0),         // bottom fill
        ])
        .split(area);

    // Title (dimmed)
    let title = Paragraph::new(Line::from(Span::styled(
        "chattor",
        Style::default().fg(theme.fg_dim),
    )))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // Failure art
    let art_data = failure_art();
    let art_lines: Vec<Line> = art_data
        .iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(theme.fg_dim))))
        .collect();
    let art = Paragraph::new(art_lines).alignment(Alignment::Center);
    f.render_widget(art, chunks[3]);

    // "connection failed :("
    let fail_msg = Paragraph::new(Line::from(Span::styled(
        "connection failed :(",
        Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    f.render_widget(fail_msg, chunks[5]);

    // Error detail
    let err_detail = Paragraph::new(Line::from(Span::styled(
        error,
        Style::default().fg(theme.fg_dim),
    )))
    .alignment(Alignment::Center);
    f.render_widget(err_detail, chunks[7]);

    // Troubleshooting tips
    let tips = vec![
        Line::from(Span::styled(
            "check your internet connection",
            Style::default().fg(theme.fg),
        )),
        Line::from(Span::styled(
            "your firewall may be blocking outbound traffic",
            Style::default().fg(theme.fg),
        )),
        Line::from(Span::styled(
            "tor network may be temporarily unreachable",
            Style::default().fg(theme.fg),
        )),
        Line::from(Span::styled(
            "try a different network — some block tor",
            Style::default().fg(theme.fg),
        )),
    ];
    let tips_widget = Paragraph::new(tips).alignment(Alignment::Center);
    f.render_widget(tips_widget, chunks[9]);

    // Docs link
    let docs = Paragraph::new(Line::from(Span::styled(
        "docs: https://github.com/chattor/chattor/wiki/tor",
        Style::default().fg(theme.fg_dim),
    )))
    .alignment(Alignment::Center);
    f.render_widget(docs, chunks[11]);

    // Action keys
    let action_line = Line::from(vec![
        Span::styled(
            "[R]",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Retry  ", Style::default().fg(theme.fg)),
        Span::styled(
            "[C]",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Continue  ", Style::default().fg(theme.fg)),
        Span::styled(
            "[Q]",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Quit", Style::default().fg(theme.fg)),
    ]);
    let actions = Paragraph::new(action_line).alignment(Alignment::Center);
    f.render_widget(actions, chunks[13]);
}

/// Handle keyboard input during the bootstrap phase.
///
/// - Ctrl+C always returns `Quit` regardless of phase.
/// - In `Failed` state: `r/R` retries, `c/C` continues offline, `q/Q` quits.
/// - In `Connecting` or `Done` states: all other keys are ignored.
pub fn handle_bootstrap_key(phase: &BootstrapPhase, key: KeyEvent) -> Option<BootstrapAction> {
    // Ctrl+C always quits
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(BootstrapAction::Quit);
    }

    match phase {
        BootstrapPhase::Failed { .. } => match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => Some(BootstrapAction::Retry),
            KeyCode::Char('c') | KeyCode::Char('C') => Some(BootstrapAction::ContinueOffline),
            KeyCode::Char('q') | KeyCode::Char('Q') => Some(BootstrapAction::Quit),
            _ => None,
        },
        BootstrapPhase::Connecting { .. } => None,
        BootstrapPhase::Done => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_phase_starts_connecting() {
        let phase = BootstrapPhase::new();
        assert_eq!(
            phase,
            BootstrapPhase::Connecting {
                progress: 0,
                frame: 0,
                tick: 0,
            }
        );
    }

    #[test]
    fn bootstrap_phase_advance_tick() {
        let mut phase = BootstrapPhase::new();
        phase.advance_tick();
        assert_eq!(
            phase,
            BootstrapPhase::Connecting {
                progress: 0,
                frame: 0,
                tick: 1,
            }
        );
        phase.advance_tick();
        assert_eq!(
            phase,
            BootstrapPhase::Connecting {
                progress: 0,
                frame: 0,
                tick: 2,
            }
        );
    }

    #[test]
    fn bootstrap_phase_frame_advances_every_3_ticks() {
        let mut phase = BootstrapPhase::new();
        // Tick 1, 2: frame stays at 0
        phase.advance_tick();
        phase.advance_tick();
        if let BootstrapPhase::Connecting { frame, .. } = &phase {
            assert_eq!(*frame, 0);
        } else {
            panic!("expected Connecting state");
        }
        // Tick 3: frame advances to 1
        phase.advance_tick();
        if let BootstrapPhase::Connecting { frame, tick, .. } = &phase {
            assert_eq!(*tick, 3);
            assert_eq!(*frame, 1);
        } else {
            panic!("expected Connecting state");
        }
    }

    #[test]
    fn bootstrap_update_variants() {
        let progress = BootstrapUpdate::Progress(42);
        assert_eq!(progress, BootstrapUpdate::Progress(42));

        let connected = BootstrapUpdate::Connected;
        assert_eq!(connected, BootstrapUpdate::Connected);

        let failed = BootstrapUpdate::Failed("timeout".to_string());
        assert_eq!(failed, BootstrapUpdate::Failed("timeout".to_string()));
    }

    #[test]
    fn set_progress_updates_connecting() {
        let mut phase = BootstrapPhase::new();
        phase.set_progress(50);
        if let BootstrapPhase::Connecting { progress, .. } = &phase {
            assert_eq!(*progress, 50);
        } else {
            panic!("expected Connecting state");
        }
    }

    #[test]
    fn set_progress_ignored_in_failed() {
        let mut phase = BootstrapPhase::new();
        phase.fail("error".to_string());
        phase.set_progress(50);
        if let BootstrapPhase::Failed { error, .. } = &phase {
            assert_eq!(error, "error");
        } else {
            panic!("expected Failed state");
        }
    }

    #[test]
    fn fail_transitions_and_resets() {
        let mut phase = BootstrapPhase::new();
        phase.advance_tick();
        phase.advance_tick();
        phase.advance_tick();
        // Now tick=3, frame=1
        phase.fail("connection refused".to_string());
        assert_eq!(
            phase,
            BootstrapPhase::Failed {
                error: "connection refused".to_string(),
                frame: 0,
                tick: 0,
            }
        );
    }

    #[test]
    fn done_transitions() {
        let mut phase = BootstrapPhase::new();
        phase.done();
        assert_eq!(phase, BootstrapPhase::Done);
    }

    #[test]
    fn advance_tick_on_failed_state() {
        let mut phase = BootstrapPhase::new();
        phase.fail("error".to_string());
        phase.advance_tick();
        phase.advance_tick();
        phase.advance_tick();
        if let BootstrapPhase::Failed { tick, frame, .. } = &phase {
            assert_eq!(*tick, 3);
            assert_eq!(*frame, 1);
        } else {
            panic!("expected Failed state");
        }
    }

    #[test]
    fn advance_tick_on_done_is_noop() {
        let mut phase = BootstrapPhase::Done;
        phase.advance_tick();
        assert_eq!(phase, BootstrapPhase::Done);
    }

    #[test]
    fn connecting_frames_exist_and_are_nonempty() {
        let frames = connecting_frames();
        assert!(frames.len() >= 4);
        for frame in &frames {
            assert!(!frame.is_empty());
        }
    }

    #[test]
    fn failure_art_exists() {
        let art = failure_art();
        assert!(!art.is_empty());
    }

    #[test]
    fn status_messages_exist() {
        let msgs = status_messages();
        assert!(msgs.len() >= 3);
    }

    #[test]
    fn failure_screen_r_retries() {
        let phase = BootstrapPhase::Failed { error: "test".into(), frame: 0, tick: 0 };
        let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        assert_eq!(handle_bootstrap_key(&phase, key), Some(BootstrapAction::Retry));
    }

    #[test]
    fn failure_screen_c_continues() {
        let phase = BootstrapPhase::Failed { error: "test".into(), frame: 0, tick: 0 };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        assert_eq!(handle_bootstrap_key(&phase, key), Some(BootstrapAction::ContinueOffline));
    }

    #[test]
    fn failure_screen_q_quits() {
        let phase = BootstrapPhase::Failed { error: "test".into(), frame: 0, tick: 0 };
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(handle_bootstrap_key(&phase, key), Some(BootstrapAction::Quit));
    }

    #[test]
    fn connecting_screen_ctrl_c_quits() {
        let phase = BootstrapPhase::new();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(handle_bootstrap_key(&phase, key), Some(BootstrapAction::Quit));
    }

    #[test]
    fn connecting_screen_ignores_other_keys() {
        let phase = BootstrapPhase::new();
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(handle_bootstrap_key(&phase, key), None);
    }
}
