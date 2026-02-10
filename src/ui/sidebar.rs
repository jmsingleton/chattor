use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::db::queries::FriendEntry;

/// Render the friends sidebar
pub fn render_sidebar(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
) {
    let title = format!(" Friends ({}) ", friends.len());
    let border_color = if focused { Color::Cyan } else { Color::DarkGray };

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
