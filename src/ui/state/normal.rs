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
