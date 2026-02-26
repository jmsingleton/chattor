use crate::db::queries::{ChatMessage, FriendEntry};
use crate::ui::theme::Theme;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};

/// Render the conversation area
#[allow(clippy::too_many_arguments)]
pub fn render_conversation(
    f: &mut Frame,
    area: Rect,
    friend: Option<&FriendEntry>,
    messages: &[ChatMessage],
    own_onion: Option<&str>,
    scroll_offset: usize,
    ephemeral_ttl: Option<i64>,
    friend_is_typing: bool,
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

    // Add 1-char horizontal padding
    let padded = ratatui::layout::Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    match friend {
        None => {
            let hint = vec![
                Line::from(Span::styled(
                    "Welcome to chattor",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press [a] to add a friend",
                    Style::default().fg(theme.fg),
                )),
                Line::from(Span::styled(
                    "Press [i] to view your identity",
                    Style::default().fg(theme.fg),
                )),
                Line::from(Span::styled(
                    "Press [p] to open your public channel",
                    Style::default().fg(theme.fg),
                )),
                Line::from(Span::styled(
                    "Press [s] to subscribe to a channel",
                    Style::default().fg(theme.fg),
                )),
            ];

            let text = Paragraph::new(hint).alignment(Alignment::Center);

            let v_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(35),
                    Constraint::Length(6),
                    Constraint::Percentage(35),
                ])
                .split(padded);
            f.render_widget(text, v_layout[1]);
        }
        Some(friend_entry) => {
            // Reserve a dedicated line for the typing indicator so it
            // doesn't overlap message content.
            let (msg_area, typing_area) = if friend_is_typing && padded.height > 1 {
                let msg = Rect {
                    x: padded.x,
                    y: padded.y,
                    width: padded.width,
                    height: padded.height.saturating_sub(1),
                };
                let typing = Rect {
                    x: padded.x,
                    y: padded.y + msg.height,
                    width: padded.width,
                    height: 1,
                };
                (msg, Some(typing))
            } else {
                (padded, None)
            };

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
                    .split(msg_area);
                f.render_widget(text, v_layout[1]);
            } else {
                render_messages(
                    f,
                    msg_area,
                    messages,
                    own_onion,
                    &friend_entry.display(),
                    scroll_offset,
                    theme,
                );
            }

            if let Some(typing_rect) = typing_area {
                let typing_text = format!("{} is typing\u{2026}", friend_entry.display());
                let typing_line = Paragraph::new(typing_text).style(
                    Style::default()
                        .fg(theme.fg_dim)
                        .add_modifier(Modifier::ITALIC),
                );
                f.render_widget(typing_line, typing_rect);
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
    let mut last_day: Option<i64> = None;

    for msg in messages {
        let msg_day = day_of_timestamp(msg.timestamp);

        if last_day != Some(msg_day) {
            if last_day.is_some() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("\u{2500}\u{2500}\u{2500} {} \u{2500}\u{2500}\u{2500}", format_date_separator(msg.timestamp)),
                Style::default().fg(theme.fg_dim),
            )));
            lines.push(Line::from(""));
            last_day = Some(msg_day);
        }

        let is_own = own_onion.is_some_and(|o| msg.sender_onion == o);
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
            Style::default()
                .fg(theme.msg_own_sender)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.msg_peer_sender)
                .add_modifier(Modifier::BOLD)
        };

        lines.push(Line::from(vec![
            Span::styled(sender.to_string(), sender_style),
            Span::styled(
                format!("  {}", time),
                Style::default().fg(theme.msg_timestamp),
            ),
            Span::styled(status_str.to_string(), status_style),
        ]));

        // Content line
        let content_prefix = if msg.ephemeral_ttl.is_some() {
            "  ⏱ "
        } else {
            "  "
        };
        lines.push(Line::from(Span::raw(format!(
            "{}{}",
            content_prefix, msg.content
        ))));

        // Blank line between messages
        lines.push(Line::from(""));
    }

    // Apply scroll offset
    let skip = if scroll_offset > 0 && lines.len() > area.height as usize {
        lines
            .len()
            .saturating_sub(area.height as usize + scroll_offset)
    } else {
        lines.len().saturating_sub(area.height as usize)
    };

    let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();

    let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
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
    let border_color = if focused {
        theme.border_focused
    } else {
        theme.border
    };
    let prompt = if focused { "> " } else { "  " };

    // Show cursor when focused
    let display_text = if focused {
        let (before, after) = crate::ui::input::split_at_char(input, cursor);
        if after.is_empty() {
            format!("{}{}\u{2588}", prompt, before)
        } else {
            format!("{}{}\u{2588}{}", prompt, before, after)
        }
    } else if input.is_empty() {
        format!("{}Type a message...", prompt)
    } else {
        format!("{}{}", prompt, input)
    };

    let widget = Paragraph::new(display_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        )
        .style(Style::default().fg(if focused {
            theme.input_fg
        } else {
            theme.input_placeholder
        }));

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

/// Format a Unix timestamp as a human-readable date for separators.
fn format_date_separator(ts: i64) -> String {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let today_start = (now_secs / 86400) * 86400;
    let msg_day_start = (ts / 86400) * 86400;

    if msg_day_start == today_start {
        "Today".to_string()
    } else if msg_day_start == today_start - 86400 {
        "Yesterday".to_string()
    } else {
        let days_ago = (today_start - msg_day_start) / 86400;
        if days_ago < 7 {
            format!("{} days ago", days_ago)
        } else if days_ago < 30 {
            format!("{} weeks ago", days_ago / 7)
        } else {
            format!("{} months ago", days_ago / 30)
        }
    }
}

/// Get the day number (days since epoch) for a Unix timestamp.
fn day_of_timestamp(ts: i64) -> i64 {
    ts / 86400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_date_separator_today() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_date_separator(now), "Today");
    }

    #[test]
    fn test_format_date_separator_yesterday() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_date_separator(now - 86400), "Yesterday");
    }

    #[test]
    fn test_format_date_separator_days_ago() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let result = format_date_separator(now - 86400 * 3);
        assert_eq!(result, "3 days ago");
    }

    #[test]
    fn test_day_of_timestamp() {
        assert_eq!(day_of_timestamp(86400), 1);
        assert_eq!(day_of_timestamp(86400 * 2 + 100), 2);
        assert_eq!(day_of_timestamp(0), 0);
    }
}
