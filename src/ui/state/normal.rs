use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_normal_key(
        &mut self,
        key: KeyEvent,
        friend_count: usize,
    ) -> Result<Option<AppAction>> {
        match self {
            AppState::Normal {
                selected_friend_idx,
                conversation_id,
                input,
                cursor,
                input_focused,
                scroll_offset,
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
                        KeyCode::Left => {
                            crate::ui::input::move_left(cursor);
                            Ok(None)
                        }
                        KeyCode::Right => {
                            crate::ui::input::move_right(input, cursor);
                            Ok(None)
                        }
                        KeyCode::PageUp => {
                            *scroll_offset = scroll_offset.saturating_add(10);
                            Ok(None)
                        }
                        KeyCode::PageDown => {
                            *scroll_offset = scroll_offset.saturating_sub(10);
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
                        KeyCode::Char('n') => Ok(Some(AppAction::ToggleNotifications)),
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
                                    *scroll_offset = 0;
                                }
                            }
                            Ok(None)
                        }
                        KeyCode::Down => {
                            if let Some(idx) = selected_friend_idx {
                                if *idx + 1 < friend_count {
                                    *idx += 1;
                                    *scroll_offset = 0;
                                }
                            }
                            Ok(None)
                        }
                        KeyCode::Char('k') => {
                            // Vim-style up navigation
                            if let Some(idx) = selected_friend_idx {
                                if *idx > 0 {
                                    *idx -= 1;
                                    *scroll_offset = 0;
                                }
                            }
                            Ok(None)
                        }
                        KeyCode::Char('j') => {
                            // Vim-style down navigation
                            if let Some(idx) = selected_friend_idx {
                                if *idx + 1 < friend_count {
                                    *idx += 1;
                                    *scroll_offset = 0;
                                }
                            }
                            Ok(None)
                        }
                        KeyCode::Enter => {
                            if let Some(idx) = *selected_friend_idx {
                                *input_focused = true;
                                *scroll_offset = 0;
                                Ok(Some(AppAction::SelectFriend(idx)))
                            } else {
                                Ok(None)
                            }
                        }
                        KeyCode::PageUp => {
                            *scroll_offset = scroll_offset.saturating_add(10);
                            Ok(None)
                        }
                        KeyCode::PageDown => {
                            *scroll_offset = scroll_offset.saturating_sub(10);
                            Ok(None)
                        }
                        _ => Ok(None),
                    }
                }
            }
            _ => unreachable!("handle_normal_key requires AppState::Normal"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn normal_nav_mode_quit() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(action, Some(AppAction::Quit));
    }

    #[test]
    fn normal_nav_mode_add_friend() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::AddingFriend { .. }));
    }

    #[test]
    fn normal_nav_mode_view_identity() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
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
        state.handle_key(key, 10).unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
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
        let action = state.handle_key(key, 10).unwrap();
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
        state
            .handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE), 10)
            .unwrap();
        state
            .handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE), 10)
            .unwrap();
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
        let action = state.handle_key(key, 10).unwrap();
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
        state.handle_key(key, 10).unwrap();
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
        state.handle_key(key, 10).unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "h");
                assert_eq!(*cursor, 1);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn tab_initializes_friend_selection() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        state.handle_key(key, 10).unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(0));
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn normal_nav_mode_view_friend_requests() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(action, Some(AppAction::ViewFriendRequests));
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
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::Normal { input, .. } => {
                assert_eq!(input, "");
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn input_focused_emoji_typing() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(
                KeyEvent::new(KeyCode::Char('\u{1F600}'), KeyModifiers::NONE),
                10,
            )
            .unwrap();
        state
            .handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "\u{1F600}a");
                assert_eq!(*cursor, 2); // char count, not byte count
            }
            _ => panic!("Expected Normal state"),
        }
        state
            .handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "\u{1F600}");
                assert_eq!(*cursor, 1);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn down_arrow_bounded_by_friend_count() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(2),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        // With 3 friends (indices 0,1,2), down from index 2 should stay at 2
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        state.handle_key(key, 3).unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(2));
            }
            _ => panic!("Expected Normal state"),
        }
        // With 5 friends, down from 2 should go to 3
        state
            .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 5)
            .unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(3));
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn page_up_increases_scroll_offset() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), 1)
            .unwrap();
        match &state {
            AppState::Normal { scroll_offset, .. } => {
                assert_eq!(*scroll_offset, 10);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn page_down_decreases_scroll_offset() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 20,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 1)
            .unwrap();
        match &state {
            AppState::Normal { scroll_offset, .. } => {
                assert_eq!(*scroll_offset, 10);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn page_down_does_not_underflow() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 5,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 1)
            .unwrap();
        match &state {
            AppState::Normal { scroll_offset, .. } => {
                assert_eq!(*scroll_offset, 0); // saturating_sub prevents underflow
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn page_up_works_while_input_focused() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: "typing...".to_string(),
            cursor: 9,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), 1)
            .unwrap();
        match &state {
            AppState::Normal {
                scroll_offset,
                input,
                ..
            } => {
                assert_eq!(*scroll_offset, 10);
                assert_eq!(input, "typing..."); // input unchanged
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn scroll_resets_on_friend_change() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: Some(1),
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 30,
        };
        // Move down to next friend
        state
            .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 5)
            .unwrap();
        match &state {
            AppState::Normal {
                scroll_offset,
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(1));
                assert_eq!(*scroll_offset, 0); // reset on conversation change
            }
            _ => panic!("Expected Normal state"),
        }
    }

    // ── Text editing keybinding tests ─────────────────────────────

    #[test]
    fn ctrl_a_moves_to_start() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello".to_string(),
            cursor: 5,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL), 10)
            .unwrap();
        match &state {
            AppState::Normal { cursor, .. } => assert_eq!(*cursor, 0),
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn ctrl_e_moves_to_end() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello".to_string(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL), 10)
            .unwrap();
        match &state {
            AppState::Normal { cursor, .. } => assert_eq!(*cursor, 5),
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn ctrl_w_deletes_word_backward() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello world".to_string(),
            cursor: 11,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL), 10)
            .unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "hello ");
                assert_eq!(*cursor, 6);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn ctrl_u_deletes_to_start() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello world".to_string(),
            cursor: 6,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL), 10)
            .unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "world");
                assert_eq!(*cursor, 0);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn delete_key_forward_deletes() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello".to_string(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::Normal { input, cursor, .. } => {
                assert_eq!(input, "ello");
                assert_eq!(*cursor, 0);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn home_key_moves_to_start() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello".to_string(),
            cursor: 5,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::Normal { cursor, .. } => assert_eq!(*cursor, 0),
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn end_key_moves_to_end() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: "hello".to_string(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::Normal { cursor, .. } => assert_eq!(*cursor, 5),
            _ => panic!("Expected Normal state"),
        }
    }

    // ── Vim j/k navigation tests ─────────────────────────────

    #[test]
    fn vim_j_navigates_down() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), 5)
            .unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(1));
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn vim_k_navigates_up() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(2),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE), 5)
            .unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(1));
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn vim_j_bounded_by_friend_count() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(2),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), 3)
            .unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(2)); // stays at last
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn vim_k_bounded_at_zero() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE), 5)
            .unwrap();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(0)); // stays at first
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn vim_j_resets_scroll_offset() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 30,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), 5)
            .unwrap();
        match &state {
            AppState::Normal {
                scroll_offset,
                selected_friend_idx,
                ..
            } => {
                assert_eq!(*selected_friend_idx, Some(1));
                assert_eq!(*scroll_offset, 0);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn vim_jk_not_active_when_input_focused() {
        let mut state = AppState::Normal {
            selected_friend_idx: Some(0),
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: true,
            scroll_offset: 0,
        };
        state
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), 5)
            .unwrap();
        match &state {
            AppState::Normal { input, .. } => {
                assert_eq!(input, "j"); // j is typed as text, not navigation
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
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::SettingEphemeral {
                conversation_id,
                selected_idx,
            } => {
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
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_view_own_channel_hotkey() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(action, Some(AppAction::ViewOwnChannel));
    }

    #[test]
    fn test_subscribe_channel_hotkey() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::SubscribingToChannel { .. }));
    }
}
