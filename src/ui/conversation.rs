use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};
use crate::db::queries::{ChatMessage, FriendEntry};
use crate::ui::theme::Theme;

/// Render the conversation area
pub fn render_conversation(
    f: &mut Frame,
    area: Rect,
    friend: Option<&FriendEntry>,
    messages: &[ChatMessage],
    own_onion: Option<&str>,
    scroll_offset: usize,
    ephemeral_ttl: Option<i64>,
    theme: &Theme,
) {
    let title = if let (Some(friend_entry), Some(ttl)) = (friend, ephemeral_ttl) {
        format!(" {} [⏱ {}] ", friend_entry.display(), format_ttl(ttl))
    } else if let Some(friend_entry) = friend {
        format!(" {} ", friend_entry.display())
    } else {
        String::new()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    match friend {
        None => {
            // No conversation selected
            let text = Paragraph::new("Select a friend to start chatting")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.fg_dim));

            // Center vertically
            let v_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(45),
                    Constraint::Length(1),
                    Constraint::Percentage(45),
                ])
                .split(inner);
            f.render_widget(text, v_layout[1]);
        }
        Some(friend_entry) => {
            if messages.is_empty() {
                let text = Paragraph::new("No messages yet. Say hello!")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(theme.fg_dim));
                let v_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(45),
                        Constraint::Length(1),
                        Constraint::Percentage(45),
                    ])
                    .split(inner);
                f.render_widget(text, v_layout[1]);
            } else {
                render_messages(f, inner, messages, own_onion, &friend_entry.display(), scroll_offset, theme);
            }
        }
    }
}

/// Render message list
fn render_messages(
    f: &mut Frame,
    area: Rect,
    messages: &[ChatMessage],
    own_onion: Option<&str>,
    friend_name: &str,
    scroll_offset: usize,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in messages {
        let is_own = own_onion.map_or(false, |o| msg.sender_onion == o);
        let sender = if is_own { "You" } else { friend_name };
        let time = format_timestamp(msg.timestamp);

        let (status_str, status_style) = if is_own {
            match msg.status.as_str() {
                "sent" => (" ✓", Style::default().fg(theme.msg_status_sent)),
                "queued" => (" ⏳", Style::default().fg(theme.msg_status_sent)),
                "failed" => (" ✗", Style::default().fg(theme.msg_status_failed)),
                "delivered" => (" ✓✓", Style::default().fg(theme.msg_status_delivered)),
                "read" => (" ✓✓", Style::default().fg(theme.msg_status_read)),
                _ => ("", Style::default().fg(theme.msg_status_sent)),
            }
        } else {
            ("", Style::default().fg(theme.msg_status_sent))
        };

        // Sender line
        let sender_style = if is_own {
            Style::default().fg(theme.msg_own_sender).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.msg_peer_sender).add_modifier(Modifier::BOLD)
        };

        lines.push(Line::from(vec![
            Span::styled(sender.to_string(), sender_style),
            Span::styled(format!("  {}", time), Style::default().fg(theme.msg_timestamp)),
            Span::styled(status_str.to_string(), status_style),
        ]));

        // Content line
        let content_prefix = if msg.ephemeral_ttl.is_some() { "  ⏱ " } else { "  " };
        lines.push(Line::from(Span::raw(format!("{}{}", content_prefix, msg.content))));

        // Blank line between messages
        lines.push(Line::from(""));
    }

    // Apply scroll offset
    let skip = if scroll_offset > 0 && lines.len() > area.height as usize {
        lines.len().saturating_sub(area.height as usize + scroll_offset)
    } else {
        lines.len().saturating_sub(area.height as usize)
    };

    let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();

    let paragraph = Paragraph::new(visible_lines)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

/// Render the setup wizard (shown when no friends exist)
pub fn render_setup_wizard(
    f: &mut Frame,
    area: Rect,
    onion_address: Option<&str>,
    friend_code: Option<&str>,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Length(2),  // Welcome
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Step 1 label
            Constraint::Length(3),  // Identity box
            Constraint::Length(3),  // Friend code box
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Step 2
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Step 3
            Constraint::Min(0),    // Fill
        ])
        .split(inner);

    // Welcome
    let welcome = Paragraph::new("Welcome to chattor")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD));
    f.render_widget(welcome, chunks[1]);

    // Step 1
    let step1 = Paragraph::new("Step 1: Your identity")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg));
    f.render_widget(step1, chunks[3]);

    // Onion address
    let addr = onion_address.unwrap_or("(Waiting for Tor...)");
    let onion_widget = Paragraph::new(format!("  {}  [click to copy]", addr))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.success).add_modifier(Modifier::BOLD));
    f.render_widget(onion_widget, chunks[4]);

    // Friend code
    let code = friend_code.unwrap_or("(Waiting for Tor...)");
    let code_widget = Paragraph::new(format!("  {}  [click to copy]", code))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.warning));
    f.render_widget(code_widget, chunks[5]);

    // Step 2
    let step2 = Paragraph::new("Step 2: Share your .onion address with a friend")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg));
    f.render_widget(step2, chunks[7]);

    // Step 3
    let step3 = Paragraph::new("Step 3: Press [a] to add their .onion address")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg));
    f.render_widget(step3, chunks[9]);
}

/// Render the message input area
pub fn render_input(
    f: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    focused: bool,
    theme: &Theme,
) {
    let border_color = if focused { theme.border_focused } else { theme.border };
    let prompt = if focused { "> " } else { "  " };

    // Show cursor when focused
    let display_text = if focused {
        if cursor < input.len() {
            format!("{}{}\u{2588}{}", prompt, &input[..cursor], &input[cursor..])
        } else {
            format!("{}{}\u{2588}", prompt, input)
        }
    } else {
        if input.is_empty() {
            format!("{}Type a message...", prompt)
        } else {
            format!("{}{}", prompt, input)
        }
    };

    let widget = Paragraph::new(display_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        )
        .style(Style::default().fg(if focused { theme.input_fg } else { theme.input_placeholder }));

    f.render_widget(widget, area);
}

/// Format TTL for display (e.g., 300 -> "5m", 3600 -> "1h")
fn format_ttl(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

/// Format timestamp for display
fn format_timestamp(ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let diff = now - ts;

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}
