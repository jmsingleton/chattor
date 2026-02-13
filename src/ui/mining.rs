use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};
use crate::ui::theme::Theme;
use crate::crypto::vanity::{MiningProgress, estimate_seconds, format_eta};

/// Render the prefix input screen (first-run identity creation).
pub fn render_prefix_input(
    f: &mut Frame,
    prefix: &str,
    _cursor: usize,
    keys_per_sec_estimate: f64,
    theme: &Theme,
) {
    let area = f.size();
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(1),  // title
            Constraint::Length(2),  // spacer + description
            Constraint::Length(2),  // description cont.
            Constraint::Length(1),  // spacer
            Constraint::Length(3),  // input box
            Constraint::Length(1),  // ETA / validation
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // valid chars hint
            Constraint::Length(2),  // spacer
            Constraint::Length(1),  // controls
            Constraint::Min(0),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        "Create Your Identity",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // Description
    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Your .onion address is your identity on the Tor network.",
            Style::default().fg(theme.fg),
        )),
    ])
    .alignment(Alignment::Center);
    f.render_widget(desc, chunks[2]);

    let desc2 = Paragraph::new(vec![
        Line::from(Span::styled(
            "Mine a vanity prefix to make it memorable.",
            Style::default().fg(theme.fg),
        )),
        Line::from(""),
    ])
    .alignment(Alignment::Center);
    f.render_widget(desc2, chunks[3]);

    // Input box
    let display_text = if prefix.is_empty() {
        "type a prefix...".to_string()
    } else {
        format!("{}\u{2588}", prefix)
    };
    let input_style = if prefix.is_empty() {
        Style::default().fg(theme.input_placeholder)
    } else {
        Style::default().fg(theme.input_fg)
    };
    let input = Paragraph::new(display_text)
        .style(input_style)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_focused))
            .title(" Desired prefix "))
        .alignment(Alignment::Center);
    f.render_widget(input, centered_rect_width(40, chunks[5]));

    // ETA hint
    let eta_text = if prefix.is_empty() {
        String::new()
    } else {
        let eta = estimate_seconds(prefix.len(), keys_per_sec_estimate);
        format!("Estimated time: {} ({} chars)", format_eta(eta), prefix.len())
    };
    let eta = Paragraph::new(Span::styled(eta_text, Style::default().fg(theme.fg_dim)))
        .alignment(Alignment::Center);
    f.render_widget(eta, chunks[6]);

    // Valid chars
    let valid = Paragraph::new(Span::styled(
        "Valid characters: a-z, 2-7",
        Style::default().fg(theme.fg_dim),
    ))
    .alignment(Alignment::Center);
    f.render_widget(valid, chunks[8]);

    // Controls
    let controls = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(" Start mining  ", Style::default().fg(theme.fg)),
        Span::styled("[Esc]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(" Skip (random address)", Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(controls, chunks[10]);
}

/// ASCII art frames for the mining animation.
fn mining_frames() -> Vec<Vec<&'static str>> {
    vec![
        vec![
            r"       /\       ",
            r"      /  \      ",
            r"     / *  \     ",
            r"    /______\    ",
            r"   /########\   ",
            r"  /##########\  ",
        ],
        vec![
            r"       /\       ",
            r"      /  \      ",
            r"     /  * \     ",
            r"    /______\    ",
            r"   /########\   ",
            r"  /##########\  ",
        ],
        vec![
            r"       /\       ",
            r"      /  \      ",
            r"     / *  \     ",
            r"    /______\    ",
            r"   /# #####\   ",
            r"  /##########\  ",
        ],
        vec![
            r"       /\       ",
            r"      /  \      ",
            r"     /  * \     ",
            r"    /______\    ",
            r"   /#### ##\   ",
            r"  /##########\  ",
        ],
    ]
}

/// Render the full-screen mining view.
pub fn render_mining_fullscreen(
    f: &mut Frame,
    prefix: &str,
    progress: &MiningProgress,
    elapsed_secs: f64,
    theme: &Theme,
) {
    let area = f.size();
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Length(1),  // title
            Constraint::Length(1),  // spacer
            Constraint::Length(6),  // ASCII art
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // target
            Constraint::Length(1),  // best match
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // hash rate
            Constraint::Length(1),  // attempts
            Constraint::Length(1),  // elapsed
            Constraint::Length(1),  // ETA
            Constraint::Length(2),  // spacer
            Constraint::Length(1),  // controls
            Constraint::Min(0),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        "MINING VANITY ADDRESS",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // ASCII art
    let frames = mining_frames();
    let frame_idx = (elapsed_secs * 3.0) as usize % frames.len();
    let art_lines: Vec<Line> = frames[frame_idx]
        .iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(theme.accent))))
        .collect();
    let art = Paragraph::new(art_lines).alignment(Alignment::Center);
    f.render_widget(art, chunks[3]);

    // Target prefix
    let target = Paragraph::new(Line::from(vec![
        Span::styled("Target:  ", Style::default().fg(theme.fg_dim)),
        Span::styled(prefix, Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(target, chunks[5]);

    // Best match so far
    let best = progress.best_onion.as_deref().unwrap_or("searching...");
    let best_display = if best.len() > 30 {
        format!("{}...", &best[..30])
    } else {
        best.to_string()
    };
    let best_line = Paragraph::new(Line::from(vec![
        Span::styled("Best:    ", Style::default().fg(theme.fg_dim)),
        Span::styled(best_display, Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(best_line, chunks[6]);

    // Stats
    let kps = format!("{:.0}", progress.keys_per_sec);
    let rate = Paragraph::new(Line::from(vec![
        Span::styled("Hash rate:  ", Style::default().fg(theme.fg_dim)),
        Span::styled(format!("{} keys/sec", kps), Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(rate, chunks[8]);

    let attempts = Paragraph::new(Line::from(vec![
        Span::styled("Attempts:   ", Style::default().fg(theme.fg_dim)),
        Span::styled(format_number(progress.attempts), Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(attempts, chunks[9]);

    let elapsed_str = format_duration(elapsed_secs);
    let elapsed = Paragraph::new(Line::from(vec![
        Span::styled("Elapsed:    ", Style::default().fg(theme.fg_dim)),
        Span::styled(elapsed_str, Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(elapsed, chunks[10]);

    let remaining = if progress.keys_per_sec > 0.0 {
        let expected = crate::crypto::vanity::estimate_attempts(prefix.len());
        let remaining_attempts = expected.saturating_sub(progress.attempts);
        let remaining_secs = remaining_attempts as f64 / progress.keys_per_sec;
        format_eta(remaining_secs)
    } else {
        "calculating...".into()
    };
    let eta = Paragraph::new(Line::from(vec![
        Span::styled("Est. left:  ", Style::default().fg(theme.fg_dim)),
        Span::styled(remaining, Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(eta, chunks[11]);

    // Controls
    let controls = if progress.found {
        Paragraph::new(Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::styled(" Accept match!", Style::default().fg(theme.success)),
        ]))
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled("[Esc]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" Browse UI  ", Style::default().fg(theme.fg)),
            Span::styled("[Enter]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" Accept best  ", Style::default().fg(theme.fg)),
            Span::styled("[q]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" Cancel", Style::default().fg(theme.fg)),
        ]))
    };
    f.render_widget(controls.alignment(Alignment::Center), chunks[13]);
}

/// Render the mining indicator in the header bar.
/// Returns the formatted spans to be inserted into the header.
pub fn mining_header_spans(prefix: &str, keys_per_sec: f64, theme: &Theme) -> Vec<Span<'static>> {
    let kps_text = format!("\" {:.0}k/s", keys_per_sec / 1000.0);
    vec![
        Span::styled("  Mining \"", Style::default().fg(theme.warning)),
        Span::styled(prefix.to_string(), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
        Span::styled(kps_text, Style::default().fg(theme.warning)),
    ]
}

/// Format a number with commas (e.g. 1,234,567).
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(b as char);
    }
    result
}

/// Format a duration in seconds to MM:SS or HH:MM:SS.
fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Helper to create a centered horizontal rect of a given width.
fn centered_rect_width(width: u16, area: Rect) -> Rect {
    let side = area.width.saturating_sub(width) / 2;
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(side),
            Constraint::Length(width.min(area.width)),
            Constraint::Length(side),
        ])
        .split(area)[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_number_basic() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn format_duration_basic() {
        assert_eq!(format_duration(0.0), "00:00");
        assert_eq!(format_duration(65.0), "01:05");
        assert_eq!(format_duration(3661.0), "01:01:01");
    }
}
