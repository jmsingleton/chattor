use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};
use crate::db::queries::ChannelPost;
use crate::ui::theme::Theme;

/// Render the channel feed view
#[allow(clippy::too_many_arguments)]
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
    theme: &Theme,
) {
    let ch_label = if channel_type == "public" { "Public" } else { "Friends Only" };
    let owner_label_owned;
    let owner_label = if is_own {
        "My"
    } else {
        owner_label_owned = crate::ui::input::truncate_display_dots(publisher_onion, 12);
        &owner_label_owned
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

        render_posts(f, chunks[0], &title, posts, read_counts, scroll_offset, theme);
        render_channel_input(f, chunks[1], input, cursor, theme);
    } else {
        render_posts(f, area, &title, posts, read_counts, scroll_offset, theme);
    }
}

fn render_posts(
    f: &mut Frame,
    area: Rect,
    title: &str,
    posts: &[ChannelPost],
    read_counts: &std::collections::HashMap<String, i64>,
    scroll_offset: usize,
    theme: &Theme,
) {
    let block = Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.channel_border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if posts.is_empty() {
        let text = Paragraph::new("No posts yet")
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
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for post in posts {
        let time = format_timestamp(post.created_at);

        // Header line with timestamp
        let mut header_spans = vec![
            Span::styled(time, Style::default().fg(theme.msg_timestamp)),
        ];

        // Show read count for own channel posts
        if let Some(&count) = read_counts.get(&post.post_id) {
            header_spans.push(Span::styled(
                format!("  seen by {}", count),
                Style::default().fg(theme.channel_read_count),
            ));
        }

        lines.push(Line::from(header_spans));

        // Content
        lines.push(Line::from(Span::styled(
            format!("  {}", post.content),
            Style::default().fg(theme.fg),
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
    theme: &Theme,
) {
    let display_text = {
        let (before, after) = crate::ui::input::split_at_char(input, cursor);
        if after.is_empty() {
            format!("> {}\u{2588}", before)
        } else {
            format!("> {}\u{2588}{}", before, after)
        }
    };

    let widget = Paragraph::new(display_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.channel_border)),
        )
        .style(Style::default().fg(theme.input_fg));

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
