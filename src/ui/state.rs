use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::error::Result;

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
    MiningPrefixInput {
        prefix: String,
        cursor: usize,
    },
    MiningActive {
        prefix: String,
        show_fullscreen: bool,
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
    SelectChannel(String, String, bool),    // (publisher_onion, channel_type, is_own)
    ViewOwnChannel,
    StartMining(String),           // prefix to mine
    AcceptMiningResult,
    CancelMining,
    ToggleMiningView,
    Quit,
}

impl AppState {
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
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
                            input.insert(*cursor, c);
                            *cursor += 1;
                            Ok(None)
                        }
                        KeyCode::Backspace => {
                            if *cursor > 0 {
                                *cursor -= 1;
                                input.remove(*cursor);
                            }
                            Ok(None)
                        }
                        KeyCode::Left => {
                            if *cursor > 0 {
                                *cursor -= 1;
                            }
                            Ok(None)
                        }
                        KeyCode::Right => {
                            if *cursor < input.len() {
                                *cursor += 1;
                            }
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
                        KeyCode::Char('p') => Ok(Some(AppAction::ViewOwnChannel)),
                        KeyCode::Tab => {
                            if selected_friend_idx.is_none() {
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
                            if let Some(idx) = selected_friend_idx {
                                *idx += 1;
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
                        input.insert(*cursor, c);
                        *cursor += 1;
                        Ok(None)
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            input.remove(*cursor);
                        }
                        Ok(None)
                    }
                    KeyCode::Left => {
                        if *cursor > 0 {
                            *cursor -= 1;
                        }
                        Ok(None)
                    }
                    KeyCode::Right => {
                        if *cursor < input.len() {
                            *cursor += 1;
                        }
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

            AppState::ViewingChannel { input, cursor, is_own, publisher_onion, channel_type, .. } => {
                if *is_own {
                    match key.code {
                        KeyCode::Char(c) => {
                            input.insert(*cursor, c);
                            *cursor += 1;
                            Ok(None)
                        }
                        KeyCode::Backspace => {
                            if *cursor > 0 {
                                *cursor -= 1;
                                input.remove(*cursor);
                            }
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
                        _ => Ok(None),
                    }
                }
            }

            AppState::SubscribingToChannel { input, cursor, error } => {
                match key.code {
                    KeyCode::Char(c) => {
                        input.insert(*cursor, c);
                        *cursor += 1;
                        Ok(None)
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            input.remove(*cursor);
                        }
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

            AppState::MiningPrefixInput { prefix, cursor } => {
                match key.code {
                    KeyCode::Char(c) => {
                        // Only allow valid base32 characters (a-z, 2-7)
                        let c_lower = c.to_ascii_lowercase();
                        if (c_lower.is_ascii_lowercase() || (c_lower >= '2' && c_lower <= '7'))
                            && c_lower != '0' && c_lower != '1' && c_lower != '8' && c_lower != '9'
                        {
                            if prefix.len() < 7 {
                                prefix.push(c_lower);
                                *cursor += 1;
                            }
                        }
                        Ok(None)
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            prefix.remove(*cursor);
                        }
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        if prefix.is_empty() {
                            // Skip mining, generate random
                            Ok(Some(AppAction::CancelMining))
                        } else {
                            Ok(Some(AppAction::StartMining(prefix.clone())))
                        }
                    }
                    KeyCode::Esc => {
                        // Skip mining
                        Ok(Some(AppAction::CancelMining))
                    }
                    _ => Ok(None),
                }
            }

            AppState::MiningActive { show_fullscreen, .. } => {
                match key.code {
                    KeyCode::Esc => {
                        *show_fullscreen = false;
                        Ok(None)
                    }
                    KeyCode::Char('m') if !*show_fullscreen => {
                        *show_fullscreen = true;
                        Ok(None)
                    }
                    KeyCode::Enter => Ok(Some(AppAction::AcceptMiningResult)),
                    KeyCode::Char('q') => Ok(Some(AppAction::CancelMining)),
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
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        state.handle_key(key).unwrap();
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
        use crate::db::queries::PendingFriendRequest;
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

    #[test]
    fn test_mining_prefix_input_typing() {
        let mut state = AppState::MiningPrefixInput {
            prefix: String::new(),
            cursor: 0,
        };
        state.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::MiningPrefixInput { prefix, .. } => assert_eq!(prefix, "chat"),
            _ => panic!("Expected MiningPrefixInput"),
        }
    }

    #[test]
    fn test_mining_prefix_rejects_invalid_chars() {
        let mut state = AppState::MiningPrefixInput {
            prefix: String::new(),
            cursor: 0,
        };
        // '8', '9', '0', '1' should be rejected
        state.handle_key(KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::MiningPrefixInput { prefix, .. } => assert_eq!(prefix, ""),
            _ => panic!("Expected MiningPrefixInput"),
        }
    }

    #[test]
    fn test_mining_prefix_enter_starts() {
        let mut state = AppState::MiningPrefixInput {
            prefix: "chat".to_string(),
            cursor: 4,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::StartMining("chat".to_string())));
    }

    #[test]
    fn test_mining_prefix_esc_cancels() {
        let mut state = AppState::MiningPrefixInput {
            prefix: "chat".to_string(),
            cursor: 4,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::CancelMining));
    }

    #[test]
    fn test_mining_active_esc_hides_fullscreen() {
        let mut state = AppState::MiningActive {
            prefix: "chat".to_string(),
            show_fullscreen: true,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::MiningActive { show_fullscreen, .. } => assert!(!show_fullscreen),
            _ => panic!("Expected MiningActive"),
        }
    }

    #[test]
    fn test_mining_active_m_shows_fullscreen() {
        let mut state = AppState::MiningActive {
            prefix: "chat".to_string(),
            show_fullscreen: false,
        };
        state.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::MiningActive { show_fullscreen, .. } => assert!(show_fullscreen),
            _ => panic!("Expected MiningActive"),
        }
    }

    #[test]
    fn test_mining_active_enter_accepts() {
        let mut state = AppState::MiningActive {
            prefix: "chat".to_string(),
            show_fullscreen: true,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::AcceptMiningResult));
    }
}
