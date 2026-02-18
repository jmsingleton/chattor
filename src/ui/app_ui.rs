use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use crate::db::queries::{FriendEntry, ChatMessage, ChannelPost, ChannelSubscription};
use super::{AppState, Theme};

/// Data needed for rendering (populated by main loop before render)
pub struct RenderContext {
    pub friends: Vec<FriendEntry>,
    pub messages: Vec<ChatMessage>,
    pub own_onion: Option<String>,
    pub friend_code: Option<String>,
    pub tor_connected: bool,
    pub pending_request_count: i64,
    pub conversation_ephemeral_ttl: Option<i64>,
    pub channel_subscriptions: Vec<ChannelSubscription>,
    pub channel_posts: Vec<ChannelPost>,
    pub channel_post_read_counts: std::collections::HashMap<String, i64>,
    pub theme: Theme,
    /// Per-onion presence: (is_online, is_typing)
    pub presence: std::collections::HashMap<String, (bool, bool)>,
}

/// Render the application UI based on current state
pub fn render_app(f: &mut Frame, app_state: &AppState, ctx: &RenderContext) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Main area
            Constraint::Length(1),  // Footer
        ])
        .split(f.size());

    // Header
    let addr_display = ctx.own_onion.as_deref()
        .map(|a| {
            let trunc = if a.len() > 16 { &a[..16] } else { a };
            format!("  [@{}...]", trunc)
        })
        .unwrap_or_default();

    let (tor_icon, tor_label, tor_color) = if ctx.tor_connected {
        ("\u{25c9}", "Connected", ctx.theme.success)
    } else {
        ("\u{25cc}", "Connecting...", ctx.theme.warning)
    };

    let header_line = Line::from(vec![
        Span::styled("  chattor", Style::default().fg(ctx.theme.header_accent).add_modifier(Modifier::BOLD)),
        Span::styled(addr_display, Style::default().fg(ctx.theme.fg_dim)),
        Span::raw("  "),
        Span::styled(format!("{} {}", tor_icon, tor_label), Style::default().fg(tor_color)),
    ]);
    let header = Paragraph::new(header_line)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(ctx.theme.border)));
    f.render_widget(header, chunks[0]);

    // Main area -- depends on state
    if let AppState::ViewingChannel { ref publisher_onion, ref channel_type, is_own, ref input, cursor, scroll_offset } = app_state {
        crate::ui::channel_feed::render_channel_feed(
            f, chunks[1], publisher_onion, channel_type, *is_own,
            input, *cursor, *scroll_offset,
            &ctx.channel_posts, &ctx.channel_post_read_counts,
            &ctx.theme,
        );
    } else {
        // Split into sidebar + conversation
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(24),  // Sidebar
                Constraint::Min(0),      // Conversation
            ])
            .split(chunks[1]);

        // Extract state from Normal variant
        let (selected_idx, _conv_id, input, cursor, input_focused, scroll_offset) =
            if let AppState::Normal {
                selected_friend_idx, conversation_id, input, cursor, input_focused, scroll_offset
            } = app_state {
                (*selected_friend_idx, *conversation_id, input.as_str(), *cursor, *input_focused, *scroll_offset)
            } else {
                (None, None, "", 0, false, 0)
            };

        // Sidebar (with channels)
        crate::ui::sidebar::render_sidebar_with_channels(
            f, main_chunks[0], &ctx.friends, selected_idx, !input_focused,
            ctx.pending_request_count, &ctx.channel_subscriptions,
            &ctx.presence,
            &ctx.theme,
        );

        // Right panel: conversation + input
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),     // Messages
                Constraint::Length(3),  // Input
            ])
            .split(main_chunks[1]);

        // Find the selected friend
        let selected_friend = selected_idx
            .and_then(|i| ctx.friends.get(i));

        // Conversation
        crate::ui::conversation::render_conversation(
            f,
            right_chunks[0],
            selected_friend,
            &ctx.messages,
            ctx.own_onion.as_deref(),
            scroll_offset,
            ctx.conversation_ephemeral_ttl,
            &ctx.theme,
        );

        // Input
        crate::ui::conversation::render_input(
            f, right_chunks[1], input, cursor, input_focused,
            &ctx.theme,
        );
    }

    // Footer
    let footer_spans = format_footer_spans(app_state, &ctx.theme);
    let footer = Paragraph::new(Line::from(footer_spans));
    f.render_widget(footer, chunks[2]);

    // Modal overlays
    match app_state {
        AppState::AddingFriend { input, error, .. } => {
            crate::ui::modals::render_add_friend_modal(f, input, error.as_deref(), &ctx.theme);
        }
        AppState::ViewingFriendRequests { requests, selected_idx } => {
            crate::ui::modals::render_friend_request_list(f, requests, *selected_idx, &ctx.theme);
        }
        AppState::ViewingFriendRequest { from_onion, friend_code, .. } => {
            crate::ui::modals::render_friend_request_modal(f, from_onion, friend_code, &ctx.theme);
        }
        AppState::ViewingMyIdentity { friend_code, onion_address, copied_field } => {
            crate::ui::modals::render_identity_modal(f, friend_code, onion_address, copied_field.as_deref(), &ctx.theme);
        }
        AppState::SettingEphemeral { selected_idx, .. } => {
            crate::ui::modals::render_ephemeral_modal(f, *selected_idx, &ctx.theme);
        }
        AppState::SubscribingToChannel { input, error, .. } => {
            crate::ui::modals::render_subscribe_channel_modal(f, input, error.as_deref(), &ctx.theme);
        }
        _ => {}
    }
}

fn format_footer_spans<'a>(state: &AppState, theme: &'a Theme) -> Vec<Span<'a>> {
    let pairs: Vec<(&str, &str)> = match state {
        AppState::Normal { input_focused: true, .. } => vec![("Enter", "Send"), ("Esc", "Nav")],
        AppState::Normal { .. } => vec![("Tab/\u{2191}\u{2193}", "Select"), ("Enter", "Open"), ("a", "Add"), ("s", "Subscribe"), ("p", "Channel"), ("i", "Identity"), ("f", "Requests"), ("q", "Quit")],
        AppState::AddingFriend { .. } => vec![("Enter", "Send"), ("Esc", "Cancel")],
        AppState::ViewingFriendRequests { .. } => vec![("\u{2191}\u{2193}", "Navigate"), ("Enter", "View"), ("Esc", "Back")],
        AppState::ViewingFriendRequest { .. } => vec![("A", "Accept"), ("R", "Reject"), ("Esc", "Back")],
        AppState::ViewingMyIdentity { .. } => vec![("o", "Copy onion"), ("c", "Copy code"), ("i/Esc", "Close")],
        AppState::SettingEphemeral { .. } => vec![("\u{2191}\u{2193}", "Select"), ("Enter", "Confirm"), ("Esc", "Cancel")],
        AppState::ViewingChannel { is_own: true, .. } => vec![("Enter", "Post"), ("Esc", "Back")],
        AppState::ViewingChannel { .. } => vec![("Esc", "Back")],
        AppState::SubscribingToChannel { .. } => vec![("Enter", "Subscribe"), ("Esc", "Cancel")],
    };

    let mut spans = vec![Span::raw("  ")];
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(theme.fg_dim)));
        }
        spans.push(Span::styled(
            format!("[{}]", key),
            Style::default().fg(theme.accent),
        ));
        spans.push(Span::styled(
            format!(" {}", desc),
            Style::default().fg(theme.fg_dim),
        ));
    }
    spans
}
