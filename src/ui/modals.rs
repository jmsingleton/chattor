use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::ui::theme::Theme;
use crate::ui::text::truncate_with_ellipsis;

/// Render "Add Friend" modal.
///
/// `cursor` is the byte position within `input` where the OS cursor should
/// be placed (so users can see what they're editing).
pub fn render_add_friend_modal(
    f: &mut Frame,
    input: &str,
    cursor: usize,
    error: Option<&str>,
    theme: &Theme,
) {
    let area = centered_rect(60, 40, f.size());

    // Clear background
    f.render_widget(Clear, area);

    let block = Block::default()
        .title("Add New Friend")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.modal_border));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    // Prompt
    let prompt = Paragraph::new("Enter their .onion address or friend code:");
    f.render_widget(prompt, chunks[0]);

    // Input field
    let input_widget = Paragraph::new(input)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.input_fg));
    f.render_widget(input_widget, chunks[1]);
    set_input_cursor(f, chunks[1], input, cursor);

    // Help text or error
    let help = if let Some(err) = error {
        Paragraph::new(err).style(Style::default().fg(theme.error))
    } else {
        Paragraph::new("Paste .onion or friend code (from [i] Identity)")
            .style(Style::default().fg(theme.fg_dim))
    };
    f.render_widget(help, chunks[2]);

    // Controls
    let controls = Paragraph::new("[Enter] Send    [Esc] Cancel")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[3]);

    f.render_widget(block, area);
}

/// Render friend request notification modal
pub fn render_friend_request_modal(
    f: &mut Frame,
    from_onion: &str,
    friend_code: &str,
    theme: &Theme,
) {
    let area = centered_rect(60, 50, f.size());

    f.render_widget(Clear, area);

    let onion_short = truncate_with_ellipsis(from_onion, 17);

    let block = Block::default()
        .title(format!(" Friend Request from {} ", onion_short))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    // Friend code
    let code = Paragraph::new(format!("Friend code: {}", friend_code))
        .wrap(Wrap { trim: false });
    f.render_widget(code, chunks[0]);

    // Onion address (wrapped for long addresses)
    let onion = Paragraph::new(format!("Onion: {}", from_onion))
        .style(Style::default().fg(theme.fg_dim))
        .wrap(Wrap { trim: false });
    f.render_widget(onion, chunks[1]);

    // Message
    let msg = Paragraph::new("This person wants to connect with you.")
        .wrap(Wrap { trim: true });
    f.render_widget(msg, chunks[2]);

    // Controls
    let controls = Paragraph::new("[A]ccept    [R]eject    [Esc] Back")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[3]);

    f.render_widget(block, area);
}

/// Render friend request list modal
pub fn render_friend_request_list(
    f: &mut Frame,
    requests: &[crate::db::queries::PendingFriendRequest],
    selected_idx: usize,
    theme: &Theme,
) {
    use ratatui::style::Modifier;
    use ratatui::widgets::{List, ListItem};
    use ratatui::text::{Line, Span};

    let area = centered_rect(60, 50, f.size());
    f.render_widget(Clear, area);

    let title = format!(" Friend Requests ({} pending) ", requests.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if requests.is_empty() {
        let msg = Paragraph::new("No pending friend requests.")
            .style(Style::default().fg(theme.fg_dim))
            .alignment(Alignment::Center);
        f.render_widget(msg, inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(0),    // List
            Constraint::Length(1), // Controls
        ])
        .split(inner);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let items: Vec<ListItem> = requests
        .iter()
        .enumerate()
        .map(|(i, req)| {
            let is_selected = i == selected_idx;
            let arrow = if is_selected { "\u{25b8} " } else { "  " };

            let onion_display = truncate_with_ellipsis(&req.from_onion, 17);

            let elapsed = now - req.received_at;
            let time_ago = if elapsed < 60 {
                "just now".to_string()
            } else if elapsed < 3600 {
                format!("{}m ago", elapsed / 60)
            } else if elapsed < 86400 {
                format!("{}h ago", elapsed / 3600)
            } else {
                format!("{}d ago", elapsed / 86400)
            };

            let style = if is_selected {
                Style::default().fg(theme.sidebar_selected_fg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };

            let line = Line::from(vec![
                Span::raw(arrow.to_string()),
                Span::styled(onion_display, style),
                Span::styled(format!("  {}", time_ago), Style::default().fg(theme.fg_dim)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, chunks[0]);

    let controls = Paragraph::new("[Enter] View    [Esc] Back")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[1]);
}

/// Render "My Identity" modal showing friend code and onion address.
/// The friend code is primary (human-friendly, shareable) and the .onion
/// address is secondary (raw identifier).
pub fn render_identity_modal(f: &mut Frame, friend_code: &str, onion_address: &str, copied_field: Option<&str>, theme: &Theme) {
    use ratatui::style::Modifier;

    let area = centered_rect(70, 70, f.size());

    // Clear area first
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" My Identity ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.modal_border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),  // Friend code label
            Constraint::Length(1),  // Hint
            Constraint::Min(4),    // Friend code box (needs room for 32 words)
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Onion address label
            Constraint::Length(3),  // Onion address box
            Constraint::Length(1),  // Help text
        ])
        .split(inner);

    // Friend code label with copy feedback (primary — this is what users share)
    let code_label = if copied_field == Some("code") {
        "Friend Code  [Copied!]"
    } else {
        "Friend Code  [c] copy"
    };
    let code_label_color = if copied_field == Some("code") { theme.success } else { theme.fg };
    let label1 = Paragraph::new(code_label)
        .style(Style::default().fg(code_label_color).add_modifier(Modifier::BOLD));
    f.render_widget(label1, chunks[0]);

    // Hint text
    let hint = Paragraph::new("Share this with friends so they can add you")
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(hint, chunks[1]);

    // Friend code value (primary, prominent)
    let code_widget = Paragraph::new(friend_code)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
        .wrap(Wrap { trim: false });
    f.render_widget(code_widget, chunks[2]);

    // Onion address label with copy feedback (secondary)
    let onion_label = if copied_field == Some("onion") {
        "Onion Address  [Copied!]"
    } else {
        "Onion Address  [o] copy"
    };
    let onion_label_color = if copied_field == Some("onion") { theme.success } else { theme.fg_dim };
    let label2 = Paragraph::new(onion_label)
        .style(Style::default().fg(onion_label_color));
    f.render_widget(label2, chunks[4]);

    // Onion address value (secondary)
    let onion_widget = Paragraph::new(onion_address)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.fg_dim))
        .wrap(Wrap { trim: false });
    f.render_widget(onion_widget, chunks[5]);

    // Help text
    let help = Paragraph::new("[Esc/i] Close")
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(help, chunks[6]);
}

/// Render ephemeral messages duration picker modal
pub fn render_ephemeral_modal(f: &mut Frame, selected_idx: usize, theme: &Theme) {
    use ratatui::style::Modifier;
    use ratatui::widgets::{List, ListItem};
    use ratatui::text::{Line, Span};

    let area = centered_rect(50, 40, f.size());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Ephemeral Messages ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.modal_border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(0),    // List
            Constraint::Length(1), // Controls
        ])
        .split(inner);

    let options = ["Off", "5 minutes", "1 hour", "24 hours", "7 days"];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let is_selected = i == selected_idx;
            let arrow = if is_selected { "\u{25b8} " } else { "  " };

            let style = if is_selected {
                Style::default().fg(theme.sidebar_selected_fg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };

            let line = Line::from(vec![
                Span::raw(arrow.to_string()),
                Span::styled(label.to_string(), style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, chunks[0]);

    let controls = Paragraph::new("[Enter] Select    [Esc] Cancel")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[1]);
}

/// Render "Subscribe to Channel" modal.
///
/// `cursor` is the byte position within `input` for OS cursor placement.
pub fn render_subscribe_channel_modal(
    f: &mut Frame,
    input: &str,
    cursor: usize,
    error: Option<&str>,
    theme: &Theme,
) {
    let area = centered_rect(60, 40, f.size());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Subscribe to Channel ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.channel_border));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    let prompt = Paragraph::new("Enter publisher's .onion address:");
    f.render_widget(prompt, chunks[0]);

    let input_widget = Paragraph::new(input)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.input_fg));
    f.render_widget(input_widget, chunks[1]);
    set_input_cursor(f, chunks[1], input, cursor);

    let help = if let Some(err) = error {
        Paragraph::new(err).style(Style::default().fg(theme.error))
    } else {
        Paragraph::new("Subscribes to their public channel")
            .style(Style::default().fg(theme.fg_dim))
    };
    f.render_widget(help, chunks[2]);

    let controls = Paragraph::new("[Enter] Subscribe    [Esc] Cancel")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[3]);

    f.render_widget(block, area);
}

/// Render the subscriptions picker — a centered list of subscribed
/// channels with selection-aware highlighting. Mirrors
/// `render_friend_request_list`.
pub fn render_subscription_picker(
    f: &mut Frame,
    subscriptions: &[crate::db::queries::ChannelSubscription],
    selected_idx: usize,
    theme: &Theme,
) {
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{List, ListItem};

    let area = centered_rect(60, 50, f.size());
    f.render_widget(Clear, area);

    let title = format!(" Channel Subscriptions ({}) ", subscriptions.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.channel_border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if subscriptions.is_empty() {
        let msg = Paragraph::new("No subscriptions yet. Press [s] to subscribe to a channel.")
            .style(Style::default().fg(theme.fg_dim))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        f.render_widget(msg, inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let items: Vec<ListItem> = subscriptions
        .iter()
        .enumerate()
        .map(|(i, sub)| {
            let is_selected = i == selected_idx;
            let arrow = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default().fg(theme.sidebar_selected_fg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            let ch_label = if sub.channel_type == "public" { "public" } else { "friends" };
            let line = Line::from(vec![
                Span::raw(arrow.to_string()),
                Span::styled(truncate_with_ellipsis(&sub.publisher_onion, 24), style),
                Span::styled(
                    format!("  [{}]", ch_label),
                    Style::default().fg(theme.fg_dim),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, chunks[0]);

    let controls = Paragraph::new("[Enter] Open    [Esc] Back")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(controls, chunks[1]);
}

/// Position the OS cursor inside a bordered input box. `area` is the box's
/// outer rect (including the border); the cursor lands one column right of
/// the left border, on the inner row, offset by the character (not byte)
/// count of the prefix up to `cursor`.
fn set_input_cursor(f: &mut Frame, area: Rect, input: &str, cursor: usize) {
    let chars_before = input
        .get(..cursor)
        .map(|s| s.chars().count())
        .unwrap_or(0) as u16;
    // Clamp to inner width so we don't park the cursor past the right
    // border when the input is longer than the visible area.
    let max_col = area.width.saturating_sub(2);
    let col = chars_before.min(max_col);
    f.set_cursor(area.x + 1 + col, area.y + 1);
}

/// Helper to center a rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
