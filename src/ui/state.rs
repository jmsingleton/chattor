use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::error::Result;

// === Multi-byte-safe input editing helpers =================================
//
// `String::insert` and `String::remove` take *byte* indices and panic if the
// index isn't on a UTF-8 char boundary. A naive `*cursor += 1` after
// `input.insert(*cursor, c)` is wrong for any non-ASCII `c` (2-4 bytes) and
// will eventually crash the renderer the next time it tries to touch a
// non-boundary index. These helpers keep the byte cursor on a boundary at
// all times.

/// Insert `c` at the byte-position cursor and step past it.
fn input_insert_char(input: &mut String, cursor: &mut usize, c: char) {
    input.insert(*cursor, c);
    *cursor += c.len_utf8();
}

/// Backspace: remove the character whose final byte is immediately before
/// the cursor; cursor lands on the start of what was just removed.
fn input_backspace(input: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let prev_start = input[..*cursor]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0);
    *cursor = prev_start;
    input.remove(*cursor);
}

/// Move cursor one character to the left along char boundaries.
fn input_cursor_left(input: &str, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    *cursor = input[..*cursor]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0);
}

/// Move cursor one character to the right along char boundaries.
fn input_cursor_right(input: &str, cursor: &mut usize) {
    if *cursor >= input.len() {
        return;
    }
    if let Some(c) = input[*cursor..].chars().next() {
        *cursor += c.len_utf8();
    }
}

#[derive(Debug, Clone)]
pub enum AppState {
    Normal {
        selected_friend_idx: Option<usize>,
        conversation_id: Option<i64>,
        input: String,
        cursor: usize,
        input_focused: bool,
        scroll_offset: usize,
    },
    AddingFriend {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
    ViewingFriendRequests {
        requests: Vec<crate::db::queries::PendingFriendRequest>,
        selected_idx: usize,
    },
    ViewingFriendRequest {
        request_id: i64,
        from_onion: String,
        friend_code: String,
        #[allow(dead_code)]
        timestamp: i64,
        return_to_list: bool,
    },
    ViewingMyIdentity {
        friend_code: String,
        onion_address: String,
        copied_field: Option<String>,
    },
    SettingEphemeral {
        conversation_id: i64,
        selected_idx: usize,
    },
    ViewingChannel {
        publisher_onion: String,
        channel_type: String,     // "public" or "friends_only"
        is_own: bool,
        input: String,            // for composing (own channels only)
        cursor: usize,
        scroll_offset: usize,
    },
    SubscribingToChannel {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
    /// Picker over the user's existing subscriptions. Opened with `S`
    /// (shift+s) from Normal mode; mirrors the ViewingFriendRequests
    /// flow — Up/Down to move, Enter to open the selected feed, Esc to
    /// return to Normal.
    ChoosingSubscription {
        subscriptions: Vec<crate::db::queries::ChannelSubscription>,
        selected_idx: usize,
    },
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Normal {
            selected_friend_idx: None,
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    SendFriendRequest(String),
    AcceptFriendRequest(i64),
    RejectFriendRequest(i64),
    SelectFriend(usize),
    SendMessage(String),
    SetEphemeralTtl(i64, Option<i64>), // (conversation_id, ttl_seconds or None for off)
    ViewMyIdentity,
    ViewFriendRequests,
    PublishChannelPost(String, String),     // (content, channel_type)
    SubscribeToChannel(String),             // publisher .onion address
    /// Unsubscribe from a foreign channel — (publisher_onion, channel_type).
    /// Fired from the ViewingChannel state when the user presses `u` on a
    /// subscribed (non-own) channel feed.
    UnsubscribeFromChannel(String, String),
    /// Open a specific channel feed — fired by the subscriptions picker
    /// (`S` keybinding) and the Enter action inside ChoosingSubscription.
    SelectChannel(String, String, bool),    // (publisher_onion, channel_type, is_own)
    /// Open the subscriptions picker. main.rs loads the current
    /// subscription list and transitions to ChoosingSubscription.
    BrowseSubscriptions,
    ViewOwnChannel,
    /// Delete the currently-selected friend. main.rs resolves the index
    /// against its own friends slice and removes the row, the
    /// conversation, queued messages, and any local subscription.
    DeleteSelectedFriend,
    /// Block the currently-selected friend's onion: insert into
    /// blocked_onions and remove the friend record. Subsequent inbound
    /// messages from that peer are dropped at the dispatcher.
    BlockSelectedFriend,
    ToggleNotifications,
    #[allow(dead_code)]
    SendPresence(crate::protocol::message::PresenceType),
    Quit,
}

/// Counts that `handle_key` needs in order to bound navigation actions to
/// the actual list lengths. Populated by the event loop from the
/// most-recent render context. Defaults to zero, which renders all
/// directional moves into no-ops — this is the safe behaviour when the
/// caller hasn't supplied counts (e.g. tests).
#[derive(Debug, Default, Clone, Copy)]
pub struct NavContext {
    pub friends_count: usize,
    /// Currently unused; will gate sidebar-channel selection (task #29).
    #[allow(dead_code)]
    pub subscriptions_count: usize,
}

impl AppState {
    /// Test-friendly entry point that defers to `handle_key_with_context`
    /// using a zero-count `NavContext`. Production code should call
    /// `handle_key_with_context` with the actual counts so that down-arrow
    /// navigation is bounded.
    #[cfg(test)]
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        self.handle_key_with_context(key, NavContext::default())
    }

    pub fn handle_key_with_context(
        &mut self,
        key: KeyEvent,
        ctx: NavContext,
    ) -> Result<Option<AppAction>> {
        // Check global keys first
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(Some(AppAction::Quit));
        }

        match self {
            AppState::Normal {
                selected_friend_idx,
                conversation_id,
                input,
                cursor,
                input_focused,
                ..
            } => {
                if *input_focused {
                    // Input mode: keystrokes go to message input
                    match key.code {
                        KeyCode::Esc => {
                            *input_focused = false;
                            Ok(None)
                        }
                        KeyCode::Enter => {
                            if input.is_empty() {
                                Ok(None)
                            } else {
                                let msg = input.clone();
                                input.clear();
                                *cursor = 0;
                                Ok(Some(AppAction::SendMessage(msg)))
                            }
                        }
                        KeyCode::Char(c) => {
                            input_insert_char(input, cursor, c);
                            Ok(None)
                        }
                        KeyCode::Backspace => {
                            input_backspace(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Left => {
                            input_cursor_left(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Right => {
                            input_cursor_right(input, cursor);
                            Ok(None)
                        }
                        _ => Ok(None),
                    }
                } else {
                    // Navigation mode: shortcuts active
                    match key.code {
                        KeyCode::Char('q') => Ok(Some(AppAction::Quit)),
                        KeyCode::Char('a') => {
                            *self = AppState::AddingFriend {
                                input: String::new(),
                                cursor: 0,
                                error: None,
                            };
                            Ok(None)
                        }
                        KeyCode::Char('i') => Ok(Some(AppAction::ViewMyIdentity)),
                        KeyCode::Char('f') => Ok(Some(AppAction::ViewFriendRequests)),
                        KeyCode::Char('e') => {
                            if let Some(conv_id) = *conversation_id {
                                *self = AppState::SettingEphemeral {
                                    conversation_id: conv_id,
                                    selected_idx: 0,
                                };
                            }
                            Ok(None)
                        }
                        KeyCode::Char('s') => {
                            *self = AppState::SubscribingToChannel {
                                input: String::new(),
                                cursor: 0,
                                error: None,
                            };
                            Ok(None)
                        }
                        // Shift+S: browse existing subscriptions instead
                        // of subscribing to a new one. main.rs loads the
                        // list and switches us to ChoosingSubscription.
                        KeyCode::Char('S') => Ok(Some(AppAction::BrowseSubscriptions)),
                        KeyCode::Char('p') => Ok(Some(AppAction::ViewOwnChannel)),
                        KeyCode::Char('n') => Ok(Some(AppAction::ToggleNotifications)),
                        // Delete / block only fire when a friend is
                        // selected. The shortcut intentionally needs a
                        // selection to avoid acting on something the user
                        // can't see highlighted.
                        KeyCode::Char('d') => {
                            if selected_friend_idx.is_some() {
                                Ok(Some(AppAction::DeleteSelectedFriend))
                            } else {
                                Ok(None)
                            }
                        }
                        KeyCode::Char('b') => {
                            if selected_friend_idx.is_some() {
                                Ok(Some(AppAction::BlockSelectedFriend))
                            } else {
                                Ok(None)
                            }
                        }
                        KeyCode::Tab => {
                            if selected_friend_idx.is_none() && ctx.friends_count > 0 {
                                *selected_friend_idx = Some(0);
                            }
                            Ok(None)
                        }
                        KeyCode::Up => {
                            if let Some(idx) = selected_friend_idx {
                                if *idx > 0 {
                                    *idx -= 1;
                                }
                            }
                            Ok(None)
                        }
                        KeyCode::Down => {
                            // Stay bounded to friends_count - 1. With
                            // friends_count == 0 the index can't move at
                            // all (matches Up semantics).
                            if let Some(idx) = selected_friend_idx {
                                if *idx + 1 < ctx.friends_count {
                                    *idx += 1;
                                }
                            }
                            Ok(None)
                        }
                        KeyCode::Enter => {
                            if let Some(idx) = *selected_friend_idx {
                                *input_focused = true;
                                Ok(Some(AppAction::SelectFriend(idx)))
                            } else {
                                Ok(None)
                            }
                        }
                        _ => Ok(None),
                    }
                }
            }

            AppState::AddingFriend { input, cursor, error } => {
                match key.code {
                    KeyCode::Char(c) => {
                        input_insert_char(input, cursor, c);
                        Ok(None)
                    }
                    KeyCode::Backspace => {
                        input_backspace(input, cursor);
                        Ok(None)
                    }
                    KeyCode::Left => {
                        input_cursor_left(input, cursor);
                        Ok(None)
                    }
                    KeyCode::Right => {
                        input_cursor_right(input, cursor);
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        if input.is_empty() {
                            *error = Some("Please enter a .onion address or friend code".to_string());
                            Ok(None)
                        } else {
                            Ok(Some(AppAction::SendFriendRequest(input.clone())))
                        }
                    }
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingFriendRequests { requests, selected_idx } => {
                match key.code {
                    KeyCode::Up => {
                        if *selected_idx > 0 {
                            *selected_idx -= 1;
                        }
                        Ok(None)
                    }
                    KeyCode::Down => {
                        if *selected_idx + 1 < requests.len() {
                            *selected_idx += 1;
                        }
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        if let Some(req) = requests.get(*selected_idx) {
                            *self = AppState::ViewingFriendRequest {
                                request_id: req.id,
                                from_onion: req.from_onion.clone(),
                                friend_code: req.friend_code.clone(),
                                timestamp: req.received_at,
                                return_to_list: true,
                            };
                        }
                        Ok(None)
                    }
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingFriendRequest { request_id, return_to_list, .. } => {
                match key.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        let id = *request_id;
                        let back_to_list = *return_to_list;
                        if back_to_list {
                            // Will be replaced with refreshed list in main.rs
                            *self = AppState::ViewingFriendRequests {
                                requests: Vec::new(),
                                selected_idx: 0,
                            };
                        } else {
                            *self = AppState::default();
                        }
                        Ok(Some(AppAction::AcceptFriendRequest(id)))
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        let id = *request_id;
                        let back_to_list = *return_to_list;
                        if back_to_list {
                            *self = AppState::ViewingFriendRequests {
                                requests: Vec::new(),
                                selected_idx: 0,
                            };
                        } else {
                            *self = AppState::default();
                        }
                        Ok(Some(AppAction::RejectFriendRequest(id)))
                    }
                    KeyCode::Esc => {
                        if *return_to_list {
                            *self = AppState::ViewingFriendRequests {
                                requests: Vec::new(),
                                selected_idx: 0,
                            };
                        } else {
                            *self = AppState::default();
                        }
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingMyIdentity { ref onion_address, ref friend_code, ref mut copied_field } => {
                match key.code {
                    KeyCode::Char('i') | KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    KeyCode::Char('o') | KeyCode::Char('1') => {
                        if crate::ui::copy_to_clipboard(onion_address) {
                            *copied_field = Some("onion".into());
                        }
                        Ok(None)
                    }
                    KeyCode::Char('c') | KeyCode::Char('2') => {
                        if crate::ui::copy_to_clipboard(friend_code) {
                            *copied_field = Some("code".into());
                        }
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::SettingEphemeral { conversation_id, selected_idx } => {
                match key.code {
                    KeyCode::Up => {
                        if *selected_idx > 0 {
                            *selected_idx -= 1;
                        }
                        Ok(None)
                    }
                    KeyCode::Down => {
                        if *selected_idx < 4 {
                            *selected_idx += 1;
                        }
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        let conv_id = *conversation_id;
                        let ttl = match *selected_idx {
                            0 => None,
                            1 => Some(300),
                            2 => Some(3600),
                            3 => Some(86400),
                            4 => Some(604800),
                            _ => None,
                        };
                        *self = AppState::default();
                        Ok(Some(AppAction::SetEphemeralTtl(conv_id, ttl)))
                    }
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingChannel { input, cursor, is_own, channel_type, publisher_onion, .. } => {
                if *is_own {
                    match key.code {
                        KeyCode::Char(c) => {
                            input_insert_char(input, cursor, c);
                            Ok(None)
                        }
                        KeyCode::Backspace => {
                            input_backspace(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Left => {
                            input_cursor_left(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Right => {
                            input_cursor_right(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Enter => {
                            if input.is_empty() {
                                Ok(None)
                            } else {
                                let content = input.clone();
                                let ch_type = channel_type.clone();
                                input.clear();
                                *cursor = 0;
                                Ok(Some(AppAction::PublishChannelPost(content, ch_type)))
                            }
                        }
                        KeyCode::Esc => {
                            *self = AppState::default();
                            Ok(None)
                        }
                        _ => Ok(None),
                    }
                } else {
                    match key.code {
                        KeyCode::Esc => {
                            *self = AppState::default();
                            Ok(None)
                        }
                        // `u` unsubscribes from the publisher we're
                        // currently viewing and returns to the friends
                        // view. The protocol-level ChannelUnsubscribe is
                        // sent by the action handler.
                        KeyCode::Char('u') => {
                            let pub_o = publisher_onion.clone();
                            let ch_t = channel_type.clone();
                            *self = AppState::default();
                            Ok(Some(AppAction::UnsubscribeFromChannel(pub_o, ch_t)))
                        }
                        _ => Ok(None),
                    }
                }
            }

            AppState::ChoosingSubscription { subscriptions, selected_idx } => {
                match key.code {
                    KeyCode::Up => {
                        if *selected_idx > 0 {
                            *selected_idx -= 1;
                        }
                        Ok(None)
                    }
                    KeyCode::Down => {
                        if *selected_idx + 1 < subscriptions.len() {
                            *selected_idx += 1;
                        }
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        if let Some(sub) = subscriptions.get(*selected_idx) {
                            let pub_o = sub.publisher_onion.clone();
                            let ch_t = sub.channel_type.clone();
                            *self = AppState::default();
                            Ok(Some(AppAction::SelectChannel(pub_o, ch_t, false)))
                        } else {
                            Ok(None)
                        }
                    }
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::SubscribingToChannel { input, cursor, error } => {
                match key.code {
                    KeyCode::Char(c) => {
                        input_insert_char(input, cursor, c);
                        Ok(None)
                    }
                    KeyCode::Backspace => {
                        input_backspace(input, cursor);
                        Ok(None)
                    }
                    KeyCode::Left => {
                        input_cursor_left(input, cursor);
                        Ok(None)
                    }
                    KeyCode::Right => {
                        input_cursor_right(input, cursor);
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        if input.is_empty() {
                            *error = Some("Please enter a channel address".to_string());
                            Ok(None)
                        } else {
                            Ok(Some(AppAction::SubscribeToChannel(input.clone())))
                        }
                    }
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    // === Multi-byte input helper tests =====================================

    #[test]
    fn insert_advances_cursor_by_byte_len_of_char() {
        let mut input = String::new();
        let mut cursor = 0usize;
        input_insert_char(&mut input, &mut cursor, '日'); // 3 bytes
        assert_eq!(input, "日");
        assert_eq!(cursor, 3);
        input_insert_char(&mut input, &mut cursor, 'a'); // 1 byte
        assert_eq!(input, "日a");
        assert_eq!(cursor, 4);
        input_insert_char(&mut input, &mut cursor, '👋'); // 4 bytes
        assert_eq!(input, "日a👋");
        assert_eq!(cursor, 8);
    }

    #[test]
    fn backspace_walks_char_boundaries() {
        let mut input = "a日b".to_string();
        let mut cursor = input.len(); // at end
        input_backspace(&mut input, &mut cursor);
        assert_eq!(input, "a日");
        assert_eq!(cursor, 4);
        input_backspace(&mut input, &mut cursor);
        assert_eq!(input, "a"); // removes the 3-byte 日 in one step
        assert_eq!(cursor, 1);
        input_backspace(&mut input, &mut cursor);
        assert_eq!(input, "");
        assert_eq!(cursor, 0);
        // Empty input no-ops.
        input_backspace(&mut input, &mut cursor);
        assert_eq!(input, "");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn arrows_walk_char_boundaries() {
        let input = "a日b".to_string(); // bytes: 0 a, 1-3 日, 4 b, len 5
        let mut cursor = 0usize;
        input_cursor_right(&input, &mut cursor);
        assert_eq!(cursor, 1); // past 'a'
        input_cursor_right(&input, &mut cursor);
        assert_eq!(cursor, 4); // past 日 (3 bytes)
        input_cursor_right(&input, &mut cursor);
        assert_eq!(cursor, 5); // past 'b' (end)
        input_cursor_right(&input, &mut cursor);
        assert_eq!(cursor, 5); // clamped at end
        input_cursor_left(&input, &mut cursor);
        assert_eq!(cursor, 4);
        input_cursor_left(&input, &mut cursor);
        assert_eq!(cursor, 1); // back through 日
        input_cursor_left(&input, &mut cursor);
        assert_eq!(cursor, 0);
        input_cursor_left(&input, &mut cursor);
        assert_eq!(cursor, 0); // clamped at start
    }

    /// Replays the sequence from the code-review finding: typing a 3-byte
    /// CJK char and then any further keystroke used to panic on
    /// is_char_boundary because cursor was bumped by 1 not 3. This test
    /// exercises the whole AppState::Normal input loop and would have
    /// crashed before the fix.
    #[test]
    fn normal_input_does_not_panic_on_multibyte_then_more() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        let ctx = NavContext::default();
        for c in "日本語abc".chars() {
            state.handle_key_with_context(
                KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
                ctx,
            ).unwrap();
        }
        // Backspace three times — across the ASCII tail and into the CJK.
        for _ in 0..5 {
            state.handle_key_with_context(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                ctx,
            ).unwrap();
        }
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "日");
                assert_eq!(*cursor, 3, "cursor stays on the boundary after 日");
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn default_state_is_normal() {
        let state = AppState::default();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                conversation_id,
                input,
                cursor,
                input_focused,
                scroll_offset,
            } => {
                assert_eq!(*selected_friend_idx, None);
                assert_eq!(*conversation_id, None);
                assert_eq!(input, "");
                assert_eq!(*cursor, 0);
                assert!(!input_focused);
                assert_eq!(*scroll_offset, 0);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn normal_nav_mode_quit() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::Quit));
    }

    #[test]
    fn normal_nav_mode_add_friend() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::AddingFriend { .. }));
    }

    #[test]
    fn normal_down_arrow_is_bounded_by_friend_count() {
        // With 3 friends and current selection at index 0, three Downs should
        // land on index 2 and stay there — never roll off the end.
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let ctx = NavContext { friends_count: 3, subscriptions_count: 0 };
        for _ in 0..10 {
            state.handle_key_with_context(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                ctx,
            ).unwrap();
        }
        match &state {
            AppState::Normal { selected_friend_idx, .. } => {
                assert_eq!(*selected_friend_idx, Some(2));
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn normal_tab_does_not_select_with_zero_friends() {
        // Pressing Tab when there are no friends must not produce a Some(0)
        // selection — there is no friend at index 0 to select.
        let mut state = AppState::default();
        let ctx = NavContext { friends_count: 0, subscriptions_count: 0 };
        state.handle_key_with_context(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            ctx,
        ).unwrap();
        match &state {
            AppState::Normal { selected_friend_idx, .. } => {
                assert_eq!(*selected_friend_idx, None);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn normal_nav_mode_view_identity() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::ViewMyIdentity));
    }

    #[test]
    fn normal_nav_mode_arrow_selects_friend() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(1),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        state.handle_key(key).unwrap();
        match &state {
            AppState::Normal { selected_friend_idx, .. } => {
                assert_eq!(*selected_friend_idx, Some(0));
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn normal_enter_selects_friend_and_focuses_input() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::SelectFriend(0)));
        match &state {
            AppState::Normal { input_focused, .. } => {
                assert!(input_focused);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn input_focused_typing() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "hi");
                assert_eq!(*cursor, 2);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn input_focused_enter_sends_message() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello".to_string(),
            cursor: 5,
            input_focused: true,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::SendMessage("hello".to_string())));
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "");
                assert_eq!(*cursor, 0);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn input_focused_escape_unfocuses() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "draft".to_string(),
            cursor: 5,
            input_focused: true,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        state.handle_key(key).unwrap();
        match &state {
            AppState::Normal { input_focused, .. } => {
                assert!(!input_focused);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn input_focused_backspace() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hi".to_string(),
            cursor: 2,
            input_focused: true,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        state.handle_key(key).unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "h");
                assert_eq!(*cursor, 1);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn ctrl_c_quits_from_any_state() {
        // From Normal
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::Quit));

        // From AddingFriend
        let mut state = AppState::AddingFriend {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::Quit));

        // From ViewingMyIdentity
        let mut state = AppState::ViewingMyIdentity {
            friend_code: "test".to_string(),
            onion_address: "test.onion".to_string(),
            copied_field: None,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::Quit));
    }

    #[test]
    fn adding_friend_enter_sends() {
        let mut state = AppState::AddingFriend {
            input: "friend.onion".to_string(),
            cursor: 12,
            error: None,
        };
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::SendFriendRequest("friend.onion".to_string())));
    }

    #[test]
    fn adding_friend_escape_returns_to_normal() {
        let mut state = AppState::AddingFriend {
            input: "test".to_string(),
            cursor: 4,
            error: None,
        };
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        state.handle_key(key).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn friend_request_accept() {
        let mut state = AppState::ViewingFriendRequest {
            request_id: 42,
            from_onion: "test.onion".to_string(),
            friend_code: "friend-1234-code-5678".to_string(),
            timestamp: 1234567890,
            return_to_list: false,
        };
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::AcceptFriendRequest(42)));
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn identity_escape() {
        let mut state = AppState::ViewingMyIdentity {
            friend_code: "test-code".to_string(),
            onion_address: "test.onion".to_string(),
            copied_field: None,
        };
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        state.handle_key(key).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn tab_initializes_friend_selection() {
        // Tab needs a non-zero friends_count to be willing to set a
        // selection — see `normal_tab_does_not_select_with_zero_friends`.
        let mut state = AppState::default();
        let ctx = NavContext { friends_count: 1, subscriptions_count: 0 };
        state.handle_key_with_context(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            ctx,
        ).unwrap();
        match &state {
            AppState::Normal { selected_friend_idx, .. } => {
                assert_eq!(*selected_friend_idx, Some(0));
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn normal_nav_mode_view_friend_requests() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::ViewFriendRequests));
    }

    #[test]
    fn friend_requests_list_navigation() {
        use crate::db::queries::PendingFriendRequest;
        let mut state = AppState::ViewingFriendRequests {
            requests: vec![
                PendingFriendRequest { id: 1, from_onion: "a.onion".into(), friend_code: "code-1".into(), received_at: 1000 },
                PendingFriendRequest { id: 2, from_onion: "b.onion".into(), friend_code: "code-2".into(), received_at: 2000 },
            ],
            selected_idx: 0,
        };

        // Down
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 1);
        } else { panic!("Wrong state"); }

        // Down at bottom stays at bottom
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 1);
        } else { panic!("Wrong state"); }

        // Up
        state.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)).unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 0);
        } else { panic!("Wrong state"); }
    }

    #[test]
    fn friend_requests_list_enter_opens_modal() {
        use crate::db::queries::PendingFriendRequest;
        let mut state = AppState::ViewingFriendRequests {
            requests: vec![
                PendingFriendRequest { id: 42, from_onion: "a.onion".into(), friend_code: "code-1".into(), received_at: 1000 },
            ],
            selected_idx: 0,
        };

        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::ViewingFriendRequest { request_id, return_to_list, .. } => {
                assert_eq!(*request_id, 42);
                assert!(*return_to_list);
            }
            _ => panic!("Expected ViewingFriendRequest state"),
        }
    }

    #[test]
    fn friend_requests_list_esc_returns_to_normal() {
        let mut state = AppState::ViewingFriendRequests {
            requests: vec![],
            selected_idx: 0,
        };

        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn friend_request_accept_returns_to_list() {
        let mut state = AppState::ViewingFriendRequest {
            request_id: 42,
            from_onion: "test.onion".to_string(),
            friend_code: "code".to_string(),
            timestamp: 1000,
            return_to_list: true,
        };

        let action = state.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::AcceptFriendRequest(42)));
        assert!(matches!(state, AppState::ViewingFriendRequests { .. }));
    }

    #[test]
    fn empty_enter_in_input_does_nothing() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::Normal { input, .. } => {
                assert_eq!(input, "");
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn test_ephemeral_hotkey_with_conversation() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::SettingEphemeral { conversation_id, selected_idx } => {
                assert_eq!(*conversation_id, 1);
                assert_eq!(*selected_idx, 0);
            }
            _ => panic!("Expected SettingEphemeral state"),
        }
    }

    #[test]
    fn test_ephemeral_hotkey_without_conversation() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_ephemeral_selection() {
        let mut state = AppState::SettingEphemeral {
            conversation_id: 42,
            selected_idx: 0,
        };
        // Down twice to select "1 hour" (index 2)
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::SettingEphemeral { selected_idx, .. } => {
                assert_eq!(*selected_idx, 2);
            }
            _ => panic!("Expected SettingEphemeral state"),
        }
        // Enter to confirm
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::SetEphemeralTtl(42, Some(3600))));
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_ephemeral_escape() {
        let mut state = AppState::SettingEphemeral {
            conversation_id: 42,
            selected_idx: 2,
        };
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_view_own_channel_hotkey() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert_eq!(action, Some(AppAction::ViewOwnChannel));
    }

    #[test]
    fn test_subscribe_channel_hotkey() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::SubscribingToChannel { .. }));
    }

    #[test]
    fn test_subscribing_to_channel_typing() {
        let mut state = AppState::SubscribingToChannel {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        state.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::SubscribingToChannel { input, cursor, .. } => {
                assert_eq!(input, "ab");
                assert_eq!(*cursor, 2);
            }
            _ => panic!("Expected SubscribingToChannel state"),
        }
    }

    #[test]
    fn test_subscribing_to_channel_enter_submits() {
        let mut state = AppState::SubscribingToChannel {
            input: "peer.onion".to_string(),
            cursor: 10,
            error: None,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::SubscribeToChannel("peer.onion".to_string())));
    }

    #[test]
    fn test_subscribing_to_channel_enter_empty_shows_error() {
        let mut state = AppState::SubscribingToChannel {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::SubscribingToChannel { error, .. } => {
                assert!(error.is_some());
            }
            _ => panic!("Expected SubscribingToChannel state"),
        }
    }

    #[test]
    fn test_subscribing_to_channel_escape() {
        let mut state = AppState::SubscribingToChannel {
            input: "draft".to_string(),
            cursor: 5,
            error: None,
        };
        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_viewing_own_channel_publish() {
        let mut state = AppState::ViewingChannel {
            publisher_onion: "self.onion".to_string(),
            channel_type: "public".to_string(),
            is_own: true,
            input: "hello world".to_string(),
            cursor: 11,
            scroll_offset: 0,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::PublishChannelPost("hello world".to_string(), "public".to_string())));
        match &state {
            AppState::ViewingChannel { input, cursor, .. } => {
                assert_eq!(input, "");
                assert_eq!(*cursor, 0);
            }
            _ => panic!("Expected ViewingChannel state"),
        }
    }

    #[test]
    fn test_viewing_own_channel_empty_enter() {
        let mut state = AppState::ViewingChannel {
            publisher_onion: "self.onion".to_string(),
            channel_type: "public".to_string(),
            is_own: true,
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert!(action.is_none());
    }

    #[test]
    fn test_viewing_own_channel_escape() {
        let mut state = AppState::ViewingChannel {
            publisher_onion: "self.onion".to_string(),
            channel_type: "public".to_string(),
            is_own: true,
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
        };
        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_viewing_remote_channel_escape() {
        let mut state = AppState::ViewingChannel {
            publisher_onion: "peer.onion".to_string(),
            channel_type: "public".to_string(),
            is_own: false,
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
        };
        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_viewing_remote_channel_typing_ignored() {
        let mut state = AppState::ViewingChannel {
            publisher_onion: "peer.onion".to_string(),
            channel_type: "public".to_string(),
            is_own: false,
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::ViewingChannel { input, .. } => {
                assert_eq!(input, "");
            }
            _ => panic!("Expected ViewingChannel state"),
        }
    }

}
