use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_viewing_channel_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<AppAction>> {
        match self {
            AppState::ViewingChannel {
                input,
                cursor,
                is_own,
                channel_type,
                ..
            } => {
                if *is_own {
                    match key.code {
                        KeyCode::Home => {
                            crate::ui::input::move_to_start(cursor);
                            Ok(None)
                        }
                        KeyCode::End => {
                            crate::ui::input::move_to_end(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Delete => {
                            crate::ui::input::delete_forward(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            crate::ui::input::move_to_start(cursor);
                            Ok(None)
                        }
                        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            crate::ui::input::move_to_end(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            crate::ui::input::delete_word_backward(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            crate::ui::input::delete_to_start(input, cursor);
                            Ok(None)
                        }
                        KeyCode::Char(c) => {
                            crate::ui::input::insert_char(input, cursor, c);
                            Ok(None)
                        }
                        KeyCode::Backspace => {
                            crate::ui::input::backspace(input, cursor);
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
            _ => unreachable!("handle_viewing_channel_key requires AppState::ViewingChannel"),
        }
    }

    pub(super) fn handle_subscribing_to_channel_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<AppAction>> {
        match self {
            AppState::SubscribingToChannel {
                input,
                cursor,
                error,
            } => match key.code {
                KeyCode::Home => {
                    crate::ui::input::move_to_start(cursor);
                    Ok(None)
                }
                KeyCode::End => {
                    crate::ui::input::move_to_end(input, cursor);
                    Ok(None)
                }
                KeyCode::Delete => {
                    crate::ui::input::delete_forward(input, cursor);
                    Ok(None)
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    crate::ui::input::move_to_start(cursor);
                    Ok(None)
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    crate::ui::input::move_to_end(input, cursor);
                    Ok(None)
                }
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    crate::ui::input::delete_word_backward(input, cursor);
                    Ok(None)
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    crate::ui::input::delete_to_start(input, cursor);
                    Ok(None)
                }
                KeyCode::Char(c) => {
                    crate::ui::input::insert_char(input, cursor, c);
                    Ok(None)
                }
                KeyCode::Backspace => {
                    crate::ui::input::backspace(input, cursor);
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
            },
            _ => unreachable!(
                "handle_subscribing_to_channel_key requires AppState::SubscribingToChannel"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_subscribing_to_channel_typing() {
        let mut state = AppState::SubscribingToChannel {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), 10)
            .unwrap();
        state
            .handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE), 10)
            .unwrap();
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
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
        assert_eq!(
            action,
            Some(AppAction::SubscribeToChannel("peer.onion".to_string()))
        );
    }

    #[test]
    fn test_subscribing_to_channel_enter_empty_shows_error() {
        let mut state = AppState::SubscribingToChannel {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
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
        state
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 10)
            .unwrap();
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
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
        assert_eq!(
            action,
            Some(AppAction::PublishChannelPost(
                "hello world".to_string(),
                "public".to_string()
            ))
        );
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
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
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
        state
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 10)
            .unwrap();
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
        state
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 10)
            .unwrap();
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
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), 10)
            .unwrap();
        assert!(action.is_none());
        match &state {
            AppState::ViewingChannel { input, .. } => {
                assert_eq!(input, "");
            }
            _ => panic!("Expected ViewingChannel state"),
        }
    }
}
