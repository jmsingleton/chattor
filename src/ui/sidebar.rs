use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::db::queries::{FriendEntry, ChannelSubscription};

/// Render the friends sidebar with channels section
pub fn render_sidebar(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
    pending_request_count: i64,
) {
    render_sidebar_with_channels(f, area, friends, selected_idx, focused, pending_request_count, &[]);
}

/// Render the friends sidebar with channels section
pub fn render_sidebar_with_channels(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
    pending_request_count: i64,
    channel_subscriptions: &[ChannelSubscription],
) {
    // Split sidebar into friends + channels
    let channel_height = if channel_subscriptions.is_empty() { 5 } else { 3 + channel_subscriptions.len() as u16 + 2 };
    let sidebar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),                    // Friends list
            Constraint::Length(channel_height),     // Channels section
        ])
        .split(area);

    render_friends_list(f, sidebar_chunks[0], friends, selected_idx, focused, pending_request_count);
    render_channels_section(f, sidebar_chunks[1], channel_subscriptions);
}

fn render_friends_list(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
    pending_request_count: i64,
) {
    let title = if pending_request_count > 0 {
        format!(" Friends ({} new) ", pending_request_count)
    } else {
        format!(" Friends ({}) ", friends.len())
    };
    let border_color = if pending_request_count > 0 {
        Color::Yellow
    } else if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let items: Vec<ListItem> = friends
        .iter()
        .enumerate()
        .map(|(i, friend)| {
            let is_selected = selected_idx == Some(i);
            let arrow = if is_selected { "▸ " } else { "  " };
            let name = friend.display();

            // Truncate name to fit sidebar (leave room for arrow + status + unread)
            let max_name_len = 10;
            let truncated = if name.len() > max_name_len {
                format!("{}…", &name[..max_name_len])
            } else {
                name
            };

            let status_icon = "○"; // MVP: always gray for now

            let mut spans = vec![
                Span::raw(arrow),
                Span::raw(truncated),
                Span::raw(" "),
                Span::styled(status_icon, Style::default().fg(Color::DarkGray)),
            ];

            if friend.unread_count > 0 {
                spans.push(Span::styled(
                    format!(" ({})", friend.unread_count),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
            }

            let style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(Line::from(spans)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_widget(list, area);
}

/// Render the channels section in the sidebar
fn render_channels_section(
    f: &mut Frame,
    area: Rect,
    subscriptions: &[ChannelSubscription],
) {
    let mut items: Vec<ListItem> = Vec::new();

    // My channels header
    items.push(ListItem::new(Line::from(vec![
        Span::styled("  My Channels", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ])));
    items.push(ListItem::new(Line::from(vec![
        Span::styled("    Public", Style::default().fg(Color::Gray)),
    ])));
    items.push(ListItem::new(Line::from(vec![
        Span::styled("    Friends", Style::default().fg(Color::Gray)),
    ])));

    // Subscriptions
    if !subscriptions.is_empty() {
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  Subscriptions", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ])));

        for sub in subscriptions {
            let name = if sub.publisher_onion.len() > 8 {
                format!("{}...", &sub.publisher_onion[..8])
            } else {
                sub.publisher_onion.clone()
            };
            let ch_label = if sub.channel_type == "public" { "pub" } else { "fri" };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("    {} [{}]", name, ch_label), Style::default().fg(Color::Gray)),
            ])));
        }
    }

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Channels ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    f.render_widget(list, area);
}
