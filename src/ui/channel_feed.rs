use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use crate::db::queries::ChannelPost;
use crate::ui::theme::Theme;
use crate::ui::text::truncate_with_ellipsis;

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
    let owner_label: String = if is_own {
        "My".to_string()
    } else {
        truncate_with_ellipsis(publisher_onion, 13)
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

        render_posts(f, chunks[0], &title, is_own, posts, read_counts, scroll_offset, theme);
        render_channel_input(f, chunks[1], input, cursor, theme);
    } else {
        render_posts(f, area, &title, is_own, posts, read_counts, scroll_offset, theme);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_posts(
    f: &mut Frame,
    area: Rect,
    title: &str,
    is_own: bool,
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

        // Header line: publisher (only when not our own feed — own posts
        // always come from us), then timestamp and seen-count.
        let mut header_spans: Vec<Span<'_>> = Vec::new();
        if !is_own {
            header_spans.push(Span::styled(
                truncate_with_ellipsis(&post.publisher_onion, 13),
                Style::default().fg(theme.msg_peer_sender).add_modifier(ratatui::style::Modifier::BOLD),
            ));
            header_spans.push(Span::styled("  ", Style::default()));
        }
        header_spans.push(Span::styled(time, Style::default().fg(theme.msg_timestamp)));

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

    // Scroll + scrollbar in visual-row space. Mirrors the conversation
    // view: `Paragraph::line_count` accounts for soft-wrap so long posts
    // don't count as a single line just because they're a single Span.
    let paragraph_all = Paragraph::new(lines).wrap(Wrap { trim: false });
    let viewport_h = inner.height as usize;
    let text_area_width = inner.width.saturating_sub(1);
    let visual_total = paragraph_all.line_count(text_area_width);
    let has_overflow = visual_total > viewport_h;
    let text_area = if has_overflow {
        Rect { width: text_area_width, ..inner }
    } else {
        inner
    };

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
            .thumb_style(Style::default().fg(theme.channel_border));
        f.render_stateful_widget(scrollbar, inner, &mut sb_state);
    }
}

fn render_channel_input(
    f: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    theme: &Theme,
) {
    let widget = Paragraph::new(format!("> {}", input))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.channel_border)),
        )
        .style(Style::default().fg(theme.input_fg));

    f.render_widget(widget, area);

    // Native cursor placement, clamped via the shared helper so the
    // cursor can't park past the right border on long input.
    let prompt_cols: u16 = 2; // "> "
    let effective_width = area.width.saturating_sub(prompt_cols);
    let inner_col = crate::ui::text::input_cursor_column(input, cursor, effective_width);
    f.set_cursor(area.x + prompt_cols + inner_col, area.y + 1);
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
