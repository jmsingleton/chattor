use super::{AppAction, AppState};
use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent};

impl AppState {
    pub(super) fn handle_viewing_my_identity_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<AppAction>> {
        match self {
            AppState::ViewingMyIdentity {
                ref onion_address,
                ref friend_code,
                ref mut copied_field,
            } => match key.code {
                KeyCode::Char('i') | KeyCode::Esc => {
                    *self = AppState::default();
                    Ok(None)
                }
                KeyCode::Char('o') | KeyCode::Char('1') => {
                    if !onion_address.starts_with('(')
                        && crate::ui::copy_to_clipboard(onion_address)
                    {
                        *copied_field = Some("onion".into());
                    }
                    Ok(None)
                }
                KeyCode::Char('c') | KeyCode::Char('2') => {
                    if !friend_code.starts_with('(') && crate::ui::copy_to_clipboard(friend_code) {
                        *copied_field = Some("code".into());
                    }
                    Ok(None)
                }
                _ => Ok(None),
            },
            _ => unreachable!("handle_viewing_my_identity_key requires AppState::ViewingMyIdentity"),
        }
    }
}
