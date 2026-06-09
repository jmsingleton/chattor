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
