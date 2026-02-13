use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use crate::db::queries::ChannelPost;

/// Render the channel feed view
pub fn render_channel_feed(
    f: &mut Frame,
    area: Rect,
    publisher_onion: &str,
    channel_type: &str,
    is_own: bool,
    input: &str,
    cursor: usize,
    scroll_offset: usize,
    posts: &[ChannelPost],
    read_counts: &std::collections::HashMap<String, i64>,
) {
    let ch_label = if channel_type == "public" { "Public" } else { "Friends Only" };
    let owner_label = if is_own { "My" } else {
        if publisher_onion.len() > 12 { &publisher_onion[..12] } else { publisher_onion }
    };
    let title = format!(" {} {} Channel ", owner_label, ch_label);

    if is_own {
        // Split into feed + input
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),     // Posts
                Constraint::Length(3),  // Input
            ])
            .split(area);

        render_posts(f, chunks[0], &title, posts, read_counts, scroll_offset);
        render_channel_input(f, chunks[1], input, cursor);
    } else {
        render_posts(f, area, &title, posts, read_counts, scroll_offset);
    }
}

fn render_posts(
    f: &mut Frame,
    area: Rect,
    title: &str,
    posts: &[ChannelPost],
    read_counts: &std::collections::HashMap<String, i64>,
    scroll_offset: usize,
) {
    let block = Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if posts.is_empty() {
        let text = Paragraph::new("No posts yet")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        let v_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(45),
                Constraint::Length(1),
                Constraint::Percentage(45),
            ])
            .split(inner);
        f.render_widget(text, v_layout[1]);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for post in posts {
        let time = format_timestamp(post.created_at);

        // Header line with timestamp
        let mut header_spans = vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
        ];

        // Show read count for own channel posts
        if let Some(&count) = read_counts.get(&post.post_id) {
            header_spans.push(Span::styled(
                format!("  seen by {}", count),
                Style::default().fg(Color::DarkGray),
            ));
        }

        lines.push(Line::from(header_spans));

        // Content
        lines.push(Line::from(Span::styled(
            format!("  {}", post.content),
            Style::default().fg(Color::White),
        )));

        // Separator
        lines.push(Line::from(""));
    }

    // Apply scroll offset
    let skip = if scroll_offset > 0 && lines.len() > inner.height as usize {
        lines.len().saturating_sub(inner.height as usize + scroll_offset)
    } else {
        lines.len().saturating_sub(inner.height as usize)
    };

    let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();

    let paragraph = Paragraph::new(visible_lines)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

fn render_channel_input(
    f: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
) {
    let display_text = if cursor < input.len() {
        format!("> {}\u{2588}{}", &input[..cursor], &input[cursor..])
    } else {
        format!("> {}\u{2588}", input)
    };

    let widget = Paragraph::new(display_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(widget, area);
}

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
