use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use crate::db::queries::{ChatMessage, FriendEntry};
use crate::ui::theme::Theme;

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
                Line::from(Span::styled("Welcome to chattor", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(Span::styled("Press [a] to add a friend", Style::default().fg(theme.fg))),
                Line::from(Span::styled("Press [i] to view your identity", Style::default().fg(theme.fg))),
                Line::from(Span::styled("Press [p] to open your public channel", Style::default().fg(theme.fg))),
                Line::from(Span::styled("Press [s] to subscribe to a channel", Style::default().fg(theme.fg))),
            ];

            let text = Paragraph::new(hint)
                .alignment(Alignment::Center);

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
                    .split(padded);
                f.render_widget(text, v_layout[1]);

                if friend_is_typing {
                    let typing_text = format!("{} is typing\u{2026}", friend_entry.display());
                    let typing_line = Paragraph::new(typing_text)
                        .style(Style::default().fg(theme.fg_dim));
                    let typing_area = Rect {
                        x: padded.x,
                        y: padded.y + padded.height.saturating_sub(1),
                        width: padded.width,
                        height: 1,
                    };
                    f.render_widget(typing_line, typing_area);
                }
            } else {
                render_messages(f, padded, messages, own_onion, &friend_entry.display(), scroll_offset, theme);

                if friend_is_typing {
                    let typing_text = format!("{} is typing\u{2026}", friend_entry.display());
                    let typing_line = Paragraph::new(typing_text)
                        .style(Style::default().fg(theme.fg_dim));
                    if padded.height > 1 {
                        let typing_area = Rect {
                            x: padded.x,
                            y: padded.y + padded.height - 1,
                            width: padded.width,
                            height: 1,
                        };
                        f.render_widget(typing_line, typing_area);
                    }
                }
            }
        }
    }
}

/// Render message list. Inserts a centered date separator
/// ("───── Tue, Mar 14 ─────") between messages whenever the local day
/// changes, so old messages stay anchored in absolute time without losing
/// the per-message "5m ago" relative stamps.
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
    let mut last_day: Option<chrono::NaiveDate> = None;

    for msg in messages {
        // Date header if the day rolled over from the previous message.
        let msg_day = local_date(msg.timestamp);
        if last_day != Some(msg_day) {
            lines.push(date_separator_line(msg_day, area.width, theme));
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

    // Build the paragraph once over every logical line. `line_count` then
    // tells us how many visual rows that paragraph would occupy at the
    // current text width — accounting for soft-wrap on long messages.
    // Without this, a single long message would count as 1 logical line
    // even though it wraps to many on-screen rows, and the scrollbar would
    // disappear while content was clearly cut off.
    //
    // The width we count at must match the width we render at: if we count
    // assuming a reserved scrollbar column but then render full-width on
    // no-overflow, the count is one column narrower than the rendered
    // wrap point and we'd briefly show a scrollbar at exactly-fit widths.
    // Compute the would-be visual count first, decide overflow, *then*
    // fix the text-area width.
    let paragraph_all = Paragraph::new(lines).wrap(Wrap { trim: false });
    let viewport_h = area.height as usize;
    let full_width_visual = paragraph_all.line_count(area.width);
    let has_overflow = full_width_visual > viewport_h;
    let text_area = if has_overflow {
        Rect { width: area.width.saturating_sub(1), ..area }
    } else {
        area
    };
    // When overflow is true the rendered width is narrower, which may push
    // a borderline message into one more wrapped row — recompute against
    // the actual render width so the scrollbar position is accurate.
    let visual_total = if has_overflow {
        paragraph_all.line_count(text_area.width)
    } else {
        full_width_visual
    };

    // Scrolling lives in visual-row space. The viewport sticks to the
    // bottom by default; user-driven scroll_offset (currently always 0,
    // but ready for PgUp/PgDn) walks rows back up from the bottom.
    let max_scroll = visual_total.saturating_sub(viewport_h);
    let scroll_rows = max_scroll.saturating_sub(scroll_offset);
    let paragraph = paragraph_all.scroll((scroll_rows as u16, 0));
    f.render_widget(paragraph, text_area);

    if has_overflow {
        let mut sb_state = ScrollbarState::new(max_scroll).position(scroll_rows);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_style(Style::default().fg(theme.fg_dim))
            .thumb_style(Style::default().fg(theme.accent));
        f.render_stateful_widget(scrollbar, area, &mut sb_state);
    }
}

/// Render the message input area. When focused, the native terminal cursor
/// is positioned at the right column — no more fake block character.
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

    let display_text = if focused {
        format!("{}{}", prompt, input)
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
        .style(Style::default().fg(if focused { theme.input_fg } else { theme.input_placeholder }));

    f.render_widget(widget, area);

    if focused {
        // Place the OS cursor at the right column. `input_cursor_column`
        // returns the offset *inside* the box (skipping the left border)
        // and clamps so the cursor can't escape the right border on long
        // input.
        let prompt_cols = prompt.chars().count() as u16;
        // The prompt sits at the start of the inner area; treat it as a
        // hidden prefix so the visible cursor budget is `width - prompt`.
        let effective_width = area.width.saturating_sub(prompt_cols);
        let inner_col = crate::ui::text::input_cursor_column(input, cursor, effective_width);
        f.set_cursor(area.x + prompt_cols + inner_col, area.y + 1);
    }
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

/// Local-time date for a unix-seconds timestamp. Falls back to the unix
/// epoch if the timestamp can't be represented (out-of-range, etc.).
fn local_date(ts: i64) -> chrono::NaiveDate {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.date_naive())
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
}

/// Build a centered date-separator line — `─── Today ───`, `─── Yesterday ───`,
/// or `─── Wed, Mar 14 ───` for older messages. The horizontal rules fill the
/// available width on either side.
fn date_separator_line<'a>(date: chrono::NaiveDate, total_width: u16, theme: &Theme) -> Line<'a> {
    use chrono::Local;
    let today = Local::now().date_naive();
    let label = if date == today {
        "Today".to_string()
    } else if today.signed_duration_since(date).num_days() == 1 {
        "Yesterday".to_string()
    } else if today.signed_duration_since(date).num_days() < 365 {
        date.format("%a, %b %-d").to_string()
    } else {
        date.format("%b %-d, %Y").to_string()
    };

    let label_with_padding = format!(" {} ", label);
    let label_width = label_with_padding.chars().count() as u16;
    let rule_total = total_width.saturating_sub(label_width);
    let left_rule = (rule_total / 2) as usize;
    let right_rule = (rule_total - rule_total / 2) as usize;

    let rule_style = Style::default().fg(theme.fg_dim);
    let label_style = Style::default().fg(theme.fg_dim).add_modifier(Modifier::BOLD);

    Line::from(vec![
        Span::styled("─".repeat(left_rule), rule_style),
        Span::styled(label_with_padding, label_style),
        Span::styled("─".repeat(right_rule), rule_style),
    ])
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
