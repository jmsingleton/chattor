use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent};

impl AppState {
    pub(super) fn handle_adding_friend_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        match self {
            AppState::AddingFriend { input, error } => match key.code {
                KeyCode::Enter => {
                    let text = input.text().trim().to_string();
                    if text.is_empty() {
                        *error = Some("Please enter a .onion address or friend code".to_string());
                        Ok(None)
                    } else {
                        Ok(Some(AppAction::SendFriendRequest(text)))
                    }
                }
                KeyCode::Esc => {
                    *self = AppState::default();
                    Ok(None)
                }
                _ => {
                    input.handle_key(key);
                    Ok(None)
                }
            },
            _ => unreachable!("handle_adding_friend_key requires AppState::AddingFriend"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::text_input::TextInput;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn adding(text: &str) -> AppState {
        AppState::AddingFriend {
            input: TextInput::single_line("").with_text(text),
            error: None,
        }
    }

    #[test]
    fn adding_friend_typing_goes_to_input() {
        let mut state = adding("");
        state
            .handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::AddingFriend { input, .. } => assert_eq!(input.text(), "a"),
            _ => panic!("Expected AddingFriend"),
        }
    }

    #[test]
    fn adding_friend_enter_sends() {
        let mut state = adding("friend.onion");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(
            action,
            Some(AppAction::SendFriendRequest("friend.onion".to_string()))
        );
    }

    #[test]
    fn adding_friend_enter_empty_sets_error() {
        let mut state = adding("");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::AddingFriend { error, .. } => assert!(error.is_some()),
            _ => panic!("Expected AddingFriend"),
        }
    }

    #[test]
    fn adding_friend_escape_returns_to_normal() {
        let mut state = adding("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        state.handle_key(key, 10).unwrap();
        assert!(matches!(state, AppState::Normal { .. }));
    }
}
