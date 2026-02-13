use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
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
    let tor_status = if ctx.tor_connected { "Connected" } else { "Connecting..." };
    let addr_display = ctx.own_onion.as_deref()
        .map(|a| {
            let trunc = if a.len() > 16 { &a[..16] } else { a };
            format!("  [@{}...]", trunc)
        })
        .unwrap_or_default();

    let header = Paragraph::new(format!("  chattor v0.1.0{}  [Tor: {}]", addr_display, tor_status))
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Main area -- depends on state
    if let AppState::ViewingChannel { ref publisher_onion, ref channel_type, is_own, ref input, cursor, scroll_offset } = app_state {
        crate::ui::channel_feed::render_channel_feed(
            f, chunks[1], publisher_onion, channel_type, *is_own,
            input, *cursor, *scroll_offset,
            &ctx.channel_posts, &ctx.channel_post_read_counts,
        );
    } else if ctx.friends.is_empty() && ctx.channel_subscriptions.is_empty() {
        // Setup wizard
        let (onion_ref, code_ref) = (ctx.own_onion.as_deref(), ctx.friend_code.as_deref());
        crate::ui::conversation::render_setup_wizard(f, chunks[1], onion_ref, code_ref);
    } else {
        // Split into sidebar + conversation
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),  // Sidebar
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
        );

        // Input
        crate::ui::conversation::render_input(
            f, right_chunks[1], input, cursor, input_focused,
        );
    }

    // Footer
    let footer_text = match app_state {
        AppState::Normal { input_focused: true, .. } => "[Enter] Send  [Esc] Navigation mode",
        AppState::Normal { .. } => "[Tab/\u{2191}\u{2193}] Select  [Enter] Open  [a] Add  [e] Ephemeral  [i] Identity  [f] Requests  [q] Quit",
        AppState::AddingFriend { .. } => "[Enter] Send request  [Esc] Cancel",
        AppState::ViewingFriendRequests { .. } => "[\u{2191}\u{2193}] Navigate  [Enter] View  [Esc] Back",
        AppState::ViewingFriendRequest { .. } => "[A]ccept  [R]eject  [Esc] Back",
        AppState::ViewingMyIdentity { .. } => "[i/Esc] Close",
        AppState::SettingEphemeral { .. } => "[\u{2191}\u{2193}] Select  [Enter] Confirm  [Esc] Cancel",
        AppState::ViewingChannel { is_own: true, .. } => "[Enter] Post  [Esc] Back",
        AppState::ViewingChannel { .. } => "[Esc] Back",
        AppState::SubscribingToChannel { .. } => "[Enter] Subscribe  [Esc] Cancel",
    };
    let footer = Paragraph::new(format!("  {}", footer_text))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, chunks[2]);

    // Modal overlays
    match app_state {
        AppState::AddingFriend { input, error, .. } => {
            crate::ui::modals::render_add_friend_modal(f, input, error.as_deref());
        }
        AppState::ViewingFriendRequests { requests, selected_idx } => {
            crate::ui::modals::render_friend_request_list(f, requests, *selected_idx);
        }
        AppState::ViewingFriendRequest { from_onion, friend_code, .. } => {
            crate::ui::modals::render_friend_request_modal(f, from_onion, friend_code);
        }
        AppState::ViewingMyIdentity { friend_code, onion_address } => {
            crate::ui::modals::render_identity_modal(f, friend_code, onion_address);
        }
        AppState::SettingEphemeral { selected_idx, .. } => {
            crate::ui::modals::render_ephemeral_modal(f, *selected_idx);
        }
        AppState::SubscribingToChannel { input, error, .. } => {
            crate::ui::modals::render_subscribe_channel_modal(f, input, error.as_deref());
        }
        _ => {}
    }
}
