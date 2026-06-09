use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent};

impl AppState {
    pub(super) fn handle_viewing_friend_requests_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<AppAction>> {
        match self {
            AppState::ViewingFriendRequests {
                requests,
                selected_idx,
            } => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected_idx > 0 {
                        *selected_idx -= 1;
                    }
                    Ok(None)
                }
                KeyCode::Down | KeyCode::Char('j') => {
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
            },
            _ => unreachable!(
                "handle_viewing_friend_requests_key requires AppState::ViewingFriendRequests"
            ),
        }
    }

    pub(super) fn handle_viewing_friend_request_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<AppAction>> {
        match self {
            AppState::ViewingFriendRequest {
                request_id,
                return_to_list,
                ..
            } => {
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
            _ => unreachable!(
                "handle_viewing_friend_request_key requires AppState::ViewingFriendRequest"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(action, Some(AppAction::AcceptFriendRequest(42)));
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn friend_requests_list_navigation() {
        use crate::db::queries::PendingFriendRequest;
        let mut state = AppState::ViewingFriendRequests {
            requests: vec![
                PendingFriendRequest {
                    id: 1,
                    from_onion: "a.onion".into(),
                    friend_code: "code-1".into(),
                    received_at: 1000,
                },
                PendingFriendRequest {
                    id: 2,
                    from_onion: "b.onion".into(),
                    friend_code: "code-2".into(),
                    received_at: 2000,
                },
            ],
            selected_idx: 0,
        };

        // Down
        state
            .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 1);
        } else {
            panic!("Wrong state");
        }

        // Down at bottom stays at bottom
        state
            .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 1);
        } else {
            panic!("Wrong state");
        }

        // Up
        state
            .handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 0);
        } else {
            panic!("Wrong state");
        }
    }

    #[test]
    fn friend_requests_list_enter_opens_modal() {
        use crate::db::queries::PendingFriendRequest;
        let mut state = AppState::ViewingFriendRequests {
            requests: vec![PendingFriendRequest {
                id: 42,
                from_onion: "a.onion".into(),
                friend_code: "code-1".into(),
                received_at: 1000,
            }],
            selected_idx: 0,
        };

        state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::ViewingFriendRequest {
                request_id,
                return_to_list,
                ..
            } => {
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

        state
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 10)
            .unwrap();
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

        let action = state
            .handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), 10)
            .unwrap();
        assert_eq!(action, Some(AppAction::AcceptFriendRequest(42)));
        assert!(matches!(state, AppState::ViewingFriendRequests { .. }));
    }

    #[test]
    fn vim_jk_in_friend_requests() {
        use crate::db::queries::PendingFriendRequest;
        let mut state = AppState::ViewingFriendRequests {
            requests: vec![
                PendingFriendRequest {
                    id: 1,
                    from_onion: "a.onion".into(),
                    friend_code: "code-1".into(),
                    received_at: 1000,
                },
                PendingFriendRequest {
                    id: 2,
                    from_onion: "b.onion".into(),
                    friend_code: "code-2".into(),
                    received_at: 2000,
                },
                PendingFriendRequest {
                    id: 3,
                    from_onion: "c.onion".into(),
                    friend_code: "code-3".into(),
                    received_at: 3000,
                },
            ],
            selected_idx: 0,
        };

        // j moves down
        state
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 1);
        } else {
            panic!("Wrong state");
        }

        // k moves up
        state
            .handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::ViewingFriendRequests { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 0);
        } else {
            panic!("Wrong state");
        }
    }
}
