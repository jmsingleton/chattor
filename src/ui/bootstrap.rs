use crate::ui::rain::RainField;
use crate::ui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

/// How far the displayed progress may lead the real Tor value during stalls.
pub const PROGRESS_LEAD: f32 = 0.08;

/// Ease `shown` toward `target` (both in `0.0..=1.0`).
///
/// Exponential approach plus a tiny minimum creep so the bar keeps drifting
/// even when Tor stalls at a fixed percentage. Hard-capped at `target +
/// PROGRESS_LEAD` and at `0.99`, so easing alone never claims completion —
/// only the real `Connected` event drives it to 1.0 (see `force_shown_full`).
pub fn ease_progress(shown: f32, target: f32) -> f32 {
    let step = ((target - shown) * 0.08).max(0.0008);
    let next = shown + step;
    let cap = (target + PROGRESS_LEAD).min(0.99);
    next.min(cap).max(shown)
}

/// Small onion sprite that resolves above the wordmark.
pub const ONION_ART: [&str; 5] = ["  ▄██▄  ", "▄██████▄", "████████", " ▀████▀ ", "  ▀██▀  "];

/// Wordmark that resolves below the onion.
pub const WORDMARK: &str = "chattor";

/// The `progress_shown` value at which a logo cell locks to its true glyph.
/// Onion cells occupy a lower band (0.00..0.45) than wordmark cells
/// (0.55..0.90), so the onion always resolves before the wordmark.
pub fn logo_threshold(x: u32, y: u32, is_wordmark: bool) -> f32 {
    let salt: u32 = if is_wordmark {
        0x0042_0000
    } else {
        0x0000_0411
    };
    let r = crate::ui::rain::rng_unit(x, y, salt);
    if is_wordmark {
        0.55 + r * 0.35 // 0.55..0.90
    } else {
        r * 0.45 // 0.00..0.45
    }
}

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
#[derive(Debug, Clone, PartialEq)]
pub enum BootstrapPhase {
    /// Actively connecting to the Tor network.
    Connecting {
        progress: u8,
        progress_shown: f32,
        tick: u64,
        rain: RainField,
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
            progress_shown: 0.0,
            tick: 0,
            rain: RainField::new(0, 0),
        }
    }

    /// Advance the animation tick, stepping rain and easing displayed progress.
    pub fn advance_tick(&mut self) {
        match self {
            BootstrapPhase::Connecting {
                tick,
                progress,
                progress_shown,
                rain,
            } => {
                *tick += 1;
                rain.step();
                let target = *progress as f32 / 100.0;
                *progress_shown = ease_progress(*progress_shown, target);
            }
            BootstrapPhase::Failed { tick, frame, .. } => {
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

    /// Resize the rain field to the current terminal (rebuilds only on change).
    pub fn resize_rain(&mut self, cols: u16, rows: u16) {
        if let BootstrapPhase::Connecting { rain, .. } = self {
            rain.resize(cols, rows);
        }
    }

    /// Force full resolution for the connect-success flash beat.
    pub fn force_shown_full(&mut self) {
        if let BootstrapPhase::Connecting {
            progress,
            progress_shown,
            ..
        } = self
        {
            *progress = 100;
            *progress_shown = 1.0;
        }
    }

    /// Borrow the rain field, if currently connecting.
    pub fn rain(&self) -> Option<&RainField> {
        match self {
            BootstrapPhase::Connecting { rain, .. } => Some(rain),
            _ => None,
        }
    }
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

/// Map a rain level to a style for the given theme.
fn level_style(level: crate::ui::rain::Level, theme: &Theme) -> Style {
    use crate::ui::rain::Level;
    match level {
        Level::Head => Style::default().fg(theme.accent),
        Level::Body => Style::default().fg(theme.fg),
        Level::Trail => Style::default().fg(theme.fg_dim),
        Level::Empty => Style::default(),
    }
}

/// Build the logo overlay: for each terminal cell, `Some((char, locked))` if a
/// logo glyph belongs there. Onion sits above the wordmark, both centered.
/// `locked == true` → show the true glyph; `false` → tease with a rain glyph.
fn logo_overlay(
    cols: u16,
    rows: u16,
    progress_shown: f32,
    show_onion: bool,
) -> Vec<Vec<Option<(char, bool)>>> {
    let mut overlay = vec![vec![None; cols as usize]; rows as usize];
    let onion_h = if show_onion {
        ONION_ART.len() as u16
    } else {
        0
    };
    let block_h = onion_h + if show_onion { 1 } else { 0 } + 1; // onion + gap + wordmark
    if rows < block_h + 2 || cols < WORDMARK.len() as u16 + 2 {
        return overlay; // too small; caller falls back / skips
    }
    let top = (rows - block_h) / 2;

    // Onion
    if show_onion {
        for (dy, line) in ONION_ART.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let w = chars.len() as u16;
            let left = (cols.saturating_sub(w)) / 2;
            for (dx, ch) in chars.iter().enumerate() {
                if *ch == ' ' {
                    continue;
                }
                let y = top + dy as u16;
                let x = left + dx as u16;
                let locked = progress_shown > logo_threshold(x as u32, y as u32, false);
                overlay[y as usize][x as usize] = Some((*ch, locked));
            }
        }
    }

    // Wordmark
    let word: Vec<char> = WORDMARK.chars().collect();
    let w = word.len() as u16;
    let left = (cols.saturating_sub(w)) / 2;
    let y = top + onion_h + if show_onion { 1 } else { 0 };
    for (dx, ch) in word.iter().enumerate() {
        let x = left + dx as u16;
        let locked = progress_shown > logo_threshold(x as u32, y as u32, true);
        overlay[y as usize][x as usize] = Some((*ch, locked));
    }
    overlay
}

/// Render the data-rain connecting screen.
pub fn render_connecting(
    f: &mut Frame,
    rain: &RainField,
    tick: u64,
    progress_shown: f32,
    theme: &Theme,
) {
    use crate::ui::rain::Level;
    let area = f.size();
    f.render_widget(Clear, area);

    let cols = area.width;
    let rows = area.height;
    let grid = rain.render_grid(tick);

    // Drop the onion on short terminals; resolve wordmark only.
    let show_onion = rows >= ONION_ART.len() as u16 + 4;
    let overlay = logo_overlay(cols, rows, progress_shown, show_onion);

    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for y in 0..rows as usize {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for x in 0..cols as usize {
            if let Some(Some((ch, locked))) = overlay.get(y).map(|r| r[x]) {
                if locked {
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    // Tease: show a bright rain glyph in the logo's footprint.
                    let g = crate::ui::rain::glyph_at(x as u32, y as u32, tick);
                    spans.push(Span::styled(
                        g.to_string(),
                        Style::default().fg(theme.accent),
                    ));
                }
            } else {
                let (gch, level) = grid
                    .get(y)
                    .and_then(|r| r.get(x))
                    .copied()
                    .unwrap_or((' ', Level::Empty));
                spans.push(Span::styled(gch.to_string(), level_style(level, theme)));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);

    // Cheeky rotating status, bottom-centered (slowed to ~2s at 20fps).
    let msgs = status_messages();
    let msg_idx = (tick / 40) as usize % msgs.len();
    let status_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + rows.saturating_sub(2),
        width: cols,
        height: 1,
    };
    let status = Paragraph::new(Line::from(Span::styled(
        msgs[msg_idx],
        Style::default().fg(theme.fg_dim),
    )))
    .alignment(Alignment::Center);
    f.render_widget(status, status_area);
}

/// Render frozen, desaturated rain for the ~0.3s "signal lost" beat before the
/// failure screen.
pub fn render_failure_glitch(f: &mut Frame, rain: &RainField, theme: &Theme) {
    let area = f.size();
    f.render_widget(Clear, area);
    let grid = rain.render_grid(0);
    let mut lines: Vec<Line> = Vec::with_capacity(area.height as usize);
    for y in 0..area.height as usize {
        let mut spans: Vec<Span> = Vec::with_capacity(area.width as usize);
        for x in 0..area.width as usize {
            let (ch, _lvl) = grid
                .get(y)
                .and_then(|r| r.get(x))
                .copied()
                .unwrap_or((' ', crate::ui::rain::Level::Empty));
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(theme.fg_dim),
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

/// Render the connect-success flash: the fully-resolved logo pops in bold,
/// reverse-video accent over dimmed, frozen rain. Shown for a brief beat the
/// moment Tor connects, before entering the app.
pub fn render_connect_flash(f: &mut Frame, rain: &RainField, theme: &Theme) {
    use crate::ui::rain::Level;
    let area = f.size();
    f.render_widget(Clear, area);

    let cols = area.width;
    let rows = area.height;
    // Frozen rain backdrop (tick 0): glyphs stop mutating for the beat.
    let grid = rain.render_grid(0);

    let show_onion = rows >= ONION_ART.len() as u16 + 4;
    // progress_shown = 1.0 → every logo cell is locked to its true glyph.
    let overlay = logo_overlay(cols, rows, 1.0, show_onion);

    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for y in 0..rows as usize {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for x in 0..cols as usize {
            if let Some(Some((ch, _locked))) = overlay.get(y).map(|r| r[x]) {
                // Logo pops: bold accent, reverse-video block.
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                ));
            } else {
                // Rain dimmed to the faintest tone so the logo dominates.
                let (gch, _level) = grid
                    .get(y)
                    .and_then(|r| r.get(x))
                    .copied()
                    .unwrap_or((' ', Level::Empty));
                spans.push(Span::styled(
                    gch.to_string(),
                    Style::default().fg(theme.fg_dim),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
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
    use crate::ui::theme::Theme;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn new_starts_connecting_at_zero() {
        let phase = BootstrapPhase::new();
        match phase {
            BootstrapPhase::Connecting {
                progress,
                progress_shown,
                tick,
                ..
            } => {
                assert_eq!(progress, 0);
                assert_eq!(tick, 0);
                assert_eq!(progress_shown, 0.0);
            }
            _ => panic!("expected Connecting"),
        }
    }

    #[test]
    fn advance_tick_increments_and_eases() {
        let mut phase = BootstrapPhase::new();
        phase.resize_rain(20, 10);
        phase.set_progress(50); // target 0.5
        phase.advance_tick();
        match phase {
            BootstrapPhase::Connecting {
                tick,
                progress_shown,
                ..
            } => {
                assert_eq!(tick, 1);
                assert!(progress_shown > 0.0, "did not ease toward target");
            }
            _ => panic!("expected Connecting"),
        }
    }

    #[test]
    fn force_shown_full_completes() {
        let mut phase = BootstrapPhase::new();
        phase.force_shown_full();
        match phase {
            BootstrapPhase::Connecting {
                progress,
                progress_shown,
                ..
            } => {
                assert_eq!(progress, 100);
                assert_eq!(progress_shown, 1.0);
            }
            _ => panic!("expected Connecting"),
        }
    }

    #[test]
    fn set_progress_ignored_in_failed_still_holds() {
        let mut phase = BootstrapPhase::new();
        phase.fail("error".to_string());
        phase.set_progress(50);
        assert!(matches!(phase, BootstrapPhase::Failed { .. }));
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
        let phase = BootstrapPhase::Failed {
            error: "test".into(),
            frame: 0,
            tick: 0,
        };
        let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        assert_eq!(
            handle_bootstrap_key(&phase, key),
            Some(BootstrapAction::Retry)
        );
    }

    #[test]
    fn failure_screen_c_continues() {
        let phase = BootstrapPhase::Failed {
            error: "test".into(),
            frame: 0,
            tick: 0,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        assert_eq!(
            handle_bootstrap_key(&phase, key),
            Some(BootstrapAction::ContinueOffline)
        );
    }

    #[test]
    fn failure_screen_q_quits() {
        let phase = BootstrapPhase::Failed {
            error: "test".into(),
            frame: 0,
            tick: 0,
        };
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(
            handle_bootstrap_key(&phase, key),
            Some(BootstrapAction::Quit)
        );
    }

    #[test]
    fn connecting_screen_ctrl_c_quits() {
        let phase = BootstrapPhase::new();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            handle_bootstrap_key(&phase, key),
            Some(BootstrapAction::Quit)
        );
    }

    #[test]
    fn connecting_screen_ignores_other_keys() {
        let phase = BootstrapPhase::new();
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(handle_bootstrap_key(&phase, key), None);
    }

    #[test]
    fn ease_never_exceeds_cap_below_one() {
        // Even when target is 1.0, easing alone never reaches 1.0.
        let mut shown = 0.0;
        for _ in 0..10_000 {
            shown = ease_progress(shown, 1.0);
        }
        assert!(
            shown <= 0.99,
            "easing reached completion on its own: {shown}"
        );
        assert!(shown > 0.95, "easing stalled short: {shown}");
    }

    #[test]
    fn ease_creeps_forward_when_target_is_static() {
        // Tor stalled at 38%. Shown should still drift up (then plateau near lead cap).
        let target = 0.38;
        let a = ease_progress(0.38, target);
        let b = ease_progress(a, target);
        assert!(a > 0.38, "did not creep past a stalled target");
        assert!(b >= a, "not monotonic");
        // ...but capped so it never overruns the real value by much.
        let mut shown = 0.38;
        for _ in 0..1000 {
            shown = ease_progress(shown, target);
        }
        assert!(
            shown <= 0.38 + PROGRESS_LEAD + 1e-3,
            "led target too far: {shown}"
        );
    }

    #[test]
    fn ease_is_monotonic_nondecreasing() {
        let mut shown = 0.0;
        for step in 0..100 {
            let target = (step as f32) / 100.0;
            let next = ease_progress(shown, target);
            assert!(next >= shown, "decreased: {shown} -> {next}");
            shown = next;
        }
    }

    #[test]
    fn onion_locks_before_wordmark() {
        // Every onion threshold is strictly below every wordmark threshold,
        // so the onion always resolves first.
        let mut max_onion = 0.0f32;
        let mut min_word = 1.0f32;
        for y in 0..8 {
            for x in 0..16 {
                max_onion = max_onion.max(logo_threshold(x, y, false));
                min_word = min_word.min(logo_threshold(x, y, true));
            }
        }
        assert!(
            max_onion < min_word,
            "onion {max_onion} not below wordmark {min_word}"
        );
    }

    #[test]
    fn at_half_progress_onion_locked_wordmark_not() {
        let shown = 0.5f32;
        // All onion cells locked (max onion threshold < 0.45 < 0.5).
        for y in 0..8 {
            for x in 0..16 {
                assert!(
                    shown > logo_threshold(x, y, false),
                    "onion cell not locked at 0.5"
                );
            }
        }
        // No wordmark cell locked (min wordmark threshold >= 0.55 > 0.5).
        for x in 0..16 {
            assert!(
                shown <= logo_threshold(x, 0, true),
                "wordmark cell locked too early"
            );
        }
    }

    #[test]
    fn render_connecting_smoke() {
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let theme = Theme::preset("dark");
        let mut rain = RainField::new(80, 24);
        for _ in 0..10 {
            rain.step();
        }
        // Must not panic and must fill the buffer.
        term.draw(|f| render_connecting(f, &rain, 30, 0.5, &theme))
            .unwrap();
    }

    #[test]
    fn render_failure_glitch_smoke() {
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let theme = Theme::preset("dark");
        let rain = RainField::new(80, 24);
        term.draw(|f| render_failure_glitch(f, &rain, &theme))
            .unwrap();
    }

    #[test]
    fn render_connect_flash_smoke() {
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let theme = Theme::preset("dark");
        let mut rain = RainField::new(80, 24);
        for _ in 0..10 {
            rain.step();
        }
        term.draw(|f| render_connect_flash(f, &rain, &theme))
            .unwrap();
    }
}
