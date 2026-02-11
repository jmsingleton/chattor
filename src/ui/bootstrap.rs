use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

/// Render Tor bootstrap progress screen
pub fn render_bootstrap(f: &mut Frame, progress: u8, status: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Percentage(30),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            "🔄 chattor",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::raw(status)),
    ])
    .alignment(Alignment::Center);

    f.render_widget(title, chunks[0]);

    // Progress bar
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Connecting to Tor"))
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .percent(progress as u16)
        .label(format!("{}%", progress));

    f.render_widget(gauge, chunks[1]);

    // Fun messages based on progress
    let message = match progress {
        0..=24 => "Building circuits...",
        25..=44 => "Finding relays...",
        45..=74 => "Establishing connection...",
        75..=94 => "Almost there...",
        _ => "Connected!",
    };

    let msg_widget = Paragraph::new(message)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    f.render_widget(msg_widget, chunks[2]);
}
