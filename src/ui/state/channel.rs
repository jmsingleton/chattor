use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_viewing_channel_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
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
