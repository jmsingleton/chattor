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
