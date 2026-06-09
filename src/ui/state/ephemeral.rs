use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent};

impl AppState {
    pub(super) fn handle_setting_ephemeral_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<AppAction>> {
        // AppState::SettingEphemeral fields: conversation_id, selected_idx
        match self {
            AppState::SettingEphemeral {
                conversation_id,
                selected_idx,
            } => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected_idx > 0 {
                        *selected_idx -= 1;
                    }
                    Ok(None)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if *selected_idx < 4 {
                        *selected_idx += 1;
                    }
                    Ok(None)
                }
                KeyCode::Enter => {
                    let conv_id = *conversation_id;
                    let ttl = match *selected_idx {
                        0 => None,
                        1 => Some(300),
                        2 => Some(3600),
                        3 => Some(86400),
                        4 => Some(604800),
                        _ => None,
                    };
                    *self = AppState::default();
                    Ok(Some(AppAction::SetEphemeralTtl(conv_id, ttl)))
                }
                KeyCode::Esc => {
                    *self = AppState::default();
                    Ok(None)
                }
                _ => Ok(None),
            },
            _ => unreachable!("handle_setting_ephemeral_key requires AppState::SettingEphemeral"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_ephemeral_selection() {
        let mut state = AppState::SettingEphemeral {
            conversation_id: 42,
            selected_idx: 0,
        };
        // Down twice to select "1 hour" (index 2)
        state
            .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 10)
            .unwrap();
        state
            .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 10)
            .unwrap();
        match &state {
            AppState::SettingEphemeral { selected_idx, .. } => {
                assert_eq!(*selected_idx, 2);
            }
            _ => panic!("Expected SettingEphemeral state"),
        }
        // Enter to confirm
        let action = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 10)
            .unwrap();
        assert_eq!(action, Some(AppAction::SetEphemeralTtl(42, Some(3600))));
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn test_ephemeral_escape() {
        let mut state = AppState::SettingEphemeral {
            conversation_id: 42,
            selected_idx: 2,
        };
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = state.handle_key(key, 10).unwrap();
        assert!(action.is_none());
        assert!(matches!(state, AppState::Normal { .. }));
    }

    #[test]
    fn vim_jk_in_ephemeral_settings() {
        let mut state = AppState::SettingEphemeral {
            conversation_id: 42,
            selected_idx: 0,
        };

        // j moves down
        state
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::SettingEphemeral { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 1);
        } else {
            panic!("Wrong state");
        }

        // j again
        state
            .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::SettingEphemeral { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 2);
        } else {
            panic!("Wrong state");
        }

        // k moves up
        state
            .handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE), 10)
            .unwrap();
        if let AppState::SettingEphemeral { selected_idx, .. } = &state {
            assert_eq!(*selected_idx, 1);
        } else {
            panic!("Wrong state");
        }
    }
}
