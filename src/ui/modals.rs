use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::ui::theme::Theme;

/// Render "Add Friend" modal
pub fn render_add_friend_modal(
    f: &mut Frame,
    input: &str,
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
    let prompt = Paragraph::new("Enter their .onion address:");
    f.render_widget(prompt, chunks[0]);

    // Input field
    let input_widget = Paragraph::new(format!("{}_", input))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.input_fg));
    f.render_widget(input_widget, chunks[1]);

    // Help text or error
    let help = if let Some(err) = error {
        Paragraph::new(err).style(Style::default().fg(theme.error))
    } else {
        Paragraph::new("e.g., abc123...xyz.onion")
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
    let area = centered_rect(60, 40, f.size());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(format!("Friend Request from {}", &from_onion[..10]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    // Friend code
    let code = Paragraph::new(format!("Friend code: {}", friend_code));
    f.render_widget(code, chunks[0]);

    // Timestamp (simplified)
    let time = Paragraph::new("Received: just now")
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(time, chunks[1]);

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

            let onion_display = if req.from_onion.len() > 16 {
                format!("{}...", &req.from_onion[..16])
            } else {
                req.from_onion.clone()
            };

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

/// Render "My Identity" modal showing friend code and onion address
pub fn render_identity_modal(f: &mut Frame, friend_code: &str, onion_address: &str, theme: &Theme) {
    use ratatui::style::Modifier;

    let area = centered_rect(60, 50, f.size());

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
            Constraint::Length(1),  // Label
            Constraint::Length(3),  // Friend code box
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Label
            Constraint::Length(3),  // Onion address box
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Help text
        ])
        .split(inner);

    // Onion address label
    let label1 = Paragraph::new("Share this address with friends:")
        .style(Style::default().fg(theme.fg));
    f.render_widget(label1, chunks[0]);

    // Onion address value (primary - this is what friends need to add you)
    let onion_widget_top = Paragraph::new(onion_address)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
        .wrap(Wrap { trim: false });
    f.render_widget(onion_widget_top, chunks[1]);

    // Friend code label
    let label2 = Paragraph::new("Friend Code:")
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(label2, chunks[3]);

    // Friend code value
    let code_widget = Paragraph::new(friend_code)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.warning))
        .wrap(Wrap { trim: false });
    f.render_widget(code_widget, chunks[4]);

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

/// Render "Subscribe to Channel" modal
pub fn render_subscribe_channel_modal(
    f: &mut Frame,
    input: &str,
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

    let input_widget = Paragraph::new(format!("{}_", input))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.input_fg));
    f.render_widget(input_widget, chunks[1]);

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
