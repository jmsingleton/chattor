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
    ViewingFriendRequest {
        request_id: i64,
        from_onion: String,
        friend_code: String,
        timestamp: i64,
    },
    ViewingMyIdentity {
        friend_code: String,
        onion_address: String,
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
    ViewMyIdentity,
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
                            *error = Some("Please enter a .onion address".to_string());
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

            AppState::ViewingFriendRequest { request_id, .. } => {
                match key.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        let id = *request_id;
                        *self = AppState::default();
                        Ok(Some(AppAction::AcceptFriendRequest(id)))
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        let id = *request_id;
                        *self = AppState::default();
                        Ok(Some(AppAction::RejectFriendRequest(id)))
                    }
                    KeyCode::Esc => {
                        *self = AppState::default();
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingMyIdentity { .. } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('i') => {
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
}
