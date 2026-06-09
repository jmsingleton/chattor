use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(super) fn handle_adding_friend_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        match self {
            AppState::AddingFriend {
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
                KeyCode::Left => {
                    crate::ui::input::move_left(cursor);
                    Ok(None)
                }
                KeyCode::Right => {
                    crate::ui::input::move_right(input, cursor);
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
            },
            _ => unreachable!("handle_adding_friend_key requires AppState::AddingFriend"),
        }
    }
}
