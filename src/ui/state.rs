use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::error::Result;

#[derive(Debug, Clone)]
pub enum AppState {
    Normal,
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
        AppState::Normal
    }
}

#[derive(Debug)]
pub enum AppAction {
    SendFriendRequest(String),
    AcceptFriendRequest(i64),
    RejectFriendRequest(i64),
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
            AppState::Normal => {
                match key.code {
                    KeyCode::Char('a') => {
                        *self = AppState::AddingFriend {
                            input: String::new(),
                            cursor: 0,
                            error: None,
                        };
                        Ok(None)
                    }
                    KeyCode::Char('i') => {
                        return Ok(Some(AppAction::ViewMyIdentity));
                    }
                    KeyCode::Char('q') => Ok(Some(AppAction::Quit)),
                    _ => Ok(None),
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
                            input.remove(*cursor - 1);
                            *cursor -= 1;
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
                        *self = AppState::Normal;
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingFriendRequest { request_id, .. } => {
                match key.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        Ok(Some(AppAction::AcceptFriendRequest(*request_id)))
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        Ok(Some(AppAction::RejectFriendRequest(*request_id)))
                    }
                    KeyCode::Esc => {
                        *self = AppState::Normal;
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }

            AppState::ViewingMyIdentity { .. } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('i') => {
                        *self = AppState::Normal;
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
    fn test_normal_to_adding_friend() {
        let mut state = AppState::Normal;

        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();

        assert!(action.is_none()); // Just state change
        assert!(matches!(state, AppState::AddingFriend { .. }));
    }

    #[test]
    fn test_adding_friend_input() {
        let mut state = AppState::AddingFriend {
            input: String::new(),
            cursor: 0,
            error: None,
        };

        let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        state.handle_key(key).unwrap();

        if let AppState::AddingFriend { input, cursor, .. } = &state {
            assert_eq!(input, "h");
            assert_eq!(*cursor, 1);
        } else {
            panic!("Expected AddingFriend state");
        }
    }

    #[test]
    fn test_escape_returns_to_normal() {
        let mut state = AppState::AddingFriend {
            input: "test".into(),
            cursor: 4,
            error: None,
        };

        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        state.handle_key(key).unwrap();

        assert!(matches!(state, AppState::Normal));
    }

    #[test]
    fn test_backspace_removes_character() {
        let mut state = AppState::AddingFriend {
            input: "hello".into(),
            cursor: 5,
            error: None,
        };

        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        state.handle_key(key).unwrap();

        if let AppState::AddingFriend { input, cursor, .. } = &state {
            assert_eq!(input, "hell");
            assert_eq!(*cursor, 4);
        } else {
            panic!("Expected AddingFriend state");
        }
    }

    #[test]
    fn test_arrow_keys_move_cursor() {
        let mut state = AppState::AddingFriend {
            input: "test".into(),
            cursor: 2,
            error: None,
        };

        // Move left
        let key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        state.handle_key(key).unwrap();

        if let AppState::AddingFriend { cursor, .. } = &state {
            assert_eq!(*cursor, 1);
        } else {
            panic!("Expected AddingFriend state");
        }

        // Move right
        let key = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        state.handle_key(key).unwrap();

        if let AppState::AddingFriend { cursor, .. } = &state {
            assert_eq!(*cursor, 2);
        } else {
            panic!("Expected AddingFriend state");
        }
    }

    #[test]
    fn test_enter_with_empty_input_shows_error() {
        let mut state = AppState::AddingFriend {
            input: String::new(),
            cursor: 0,
            error: None,
        };

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();

        assert!(action.is_none()); // No action when empty

        if let AppState::AddingFriend { error, .. } = &state {
            assert!(error.is_some());
            assert_eq!(error.as_ref().unwrap(), "Please enter a .onion address");
        } else {
            panic!("Expected AddingFriend state");
        }
    }

    #[test]
    fn test_enter_with_input_returns_action() {
        let mut state = AppState::AddingFriend {
            input: "friend-1234-code-5678".into(),
            cursor: 21,
            error: None,
        };

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();

        assert!(action.is_some());
        match action.unwrap() {
            AppAction::SendFriendRequest(code) => {
                assert_eq!(code, "friend-1234-code-5678");
            }
            _ => panic!("Expected SendFriendRequest action"),
        }
    }

    #[test]
    fn test_ctrl_c_quits_from_any_state() {
        // From Normal
        let mut state = AppState::Normal;
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key).unwrap();
        assert!(matches!(action, Some(AppAction::Quit)));

        // From AddingFriend
        let mut state = AppState::AddingFriend {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key).unwrap();
        assert!(matches!(action, Some(AppAction::Quit)));
    }

    #[test]
    fn test_viewing_friend_request_accept() {
        let mut state = AppState::ViewingFriendRequest {
            request_id: 42,
            from_onion: "test.onion".into(),
            friend_code: "friend-1234-code-5678".into(),
            timestamp: 1234567890,
        };

        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();

        assert!(action.is_some());
        match action.unwrap() {
            AppAction::AcceptFriendRequest(id) => {
                assert_eq!(id, 42);
            }
            _ => panic!("Expected AcceptFriendRequest action"),
        }
    }

    #[test]
    fn test_viewing_friend_request_reject() {
        let mut state = AppState::ViewingFriendRequest {
            request_id: 42,
            from_onion: "test.onion".into(),
            friend_code: "friend-1234-code-5678".into(),
            timestamp: 1234567890,
        };

        let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();

        assert!(action.is_some());
        match action.unwrap() {
            AppAction::RejectFriendRequest(id) => {
                assert_eq!(id, 42);
            }
            _ => panic!("Expected RejectFriendRequest action"),
        }
    }

    #[test]
    fn test_viewing_friend_request_escape_returns_to_normal() {
        let mut state = AppState::ViewingFriendRequest {
            request_id: 42,
            from_onion: "test.onion".into(),
            friend_code: "friend-1234-code-5678".into(),
            timestamp: 1234567890,
        };

        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        state.handle_key(key).unwrap();

        assert!(matches!(state, AppState::Normal));
    }

    #[test]
    fn test_unhandled_keys_return_none() {
        let mut state = AppState::Normal;

        // Random key that doesn't do anything
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = state.handle_key(key).unwrap();

        assert!(action.is_none());
        assert!(matches!(state, AppState::Normal)); // State unchanged
    }

    #[test]
    fn test_default_state_is_normal() {
        let state = AppState::default();
        assert!(matches!(state, AppState::Normal));
    }

    #[test]
    fn test_view_my_identity() {
        let mut state = AppState::Normal;
        let key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        let result = state.handle_key(key).unwrap();
        assert!(matches!(result, Some(AppAction::ViewMyIdentity)));
    }

    #[test]
    fn test_identity_modal_escape() {
        let mut state = AppState::ViewingMyIdentity {
            friend_code: "test-1234-code-5678".to_string(),
            onion_address: "test.onion".to_string(),
        };
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = state.handle_key(key).unwrap();
        assert!(result.is_none());
        assert!(matches!(state, AppState::Normal));
    }

    #[test]
    fn test_identity_modal_close_with_i() {
        let mut state = AppState::ViewingMyIdentity {
            friend_code: "test-1234-code-5678".to_string(),
            onion_address: "test.onion".to_string(),
        };
        let key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        let result = state.handle_key(key).unwrap();
        assert!(result.is_none());
        assert!(matches!(state, AppState::Normal));
    }
}
