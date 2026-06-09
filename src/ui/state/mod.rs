mod adding_friend;
mod channel;
mod ephemeral;
mod friend_requests;
mod identity;
mod normal;

use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub enum AppState {
    Normal {
        selected_friend_idx: Option<usize>,
        conversation_id: Option<i64>,
        input: String,
        cursor: usize,
        input_focused: bool,
        scroll_offset: usize,
    },
    AddingFriend {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
    ViewingFriendRequests {
        requests: Vec<crate::db::queries::PendingFriendRequest>,
        selected_idx: usize,
    },
    ViewingFriendRequest {
        request_id: i64,
        from_onion: String,
        friend_code: String,
        #[allow(dead_code)]
        timestamp: i64,
        return_to_list: bool,
    },
    ViewingMyIdentity {
        friend_code: String,
        onion_address: String,
        copied_field: Option<String>,
    },
    SettingEphemeral {
        conversation_id: i64,
        selected_idx: usize,
    },
    ViewingChannel {
        publisher_onion: String,
        channel_type: String, // "public" or "friends_only"
        is_own: bool,
        input: String, // for composing (own channels only)
        cursor: usize,
        scroll_offset: usize,
    },
    SubscribingToChannel {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Normal {
            selected_friend_idx: None,
            conversation_id: None,
            input: String::new(),
            cursor: 0,
            input_focused: false,
            scroll_offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    SendFriendRequest(String),
    AcceptFriendRequest(i64),
    RejectFriendRequest(i64),
    SelectFriend(usize),
    SendMessage(String),
    SetEphemeralTtl(i64, Option<i64>), // (conversation_id, ttl_seconds or None for off)
    ViewMyIdentity,
    ViewFriendRequests,
    PublishChannelPost(String, String), // (content, channel_type)
    SubscribeToChannel(String),         // publisher .onion address
    #[allow(dead_code)]
    SelectChannel(String, String, bool), // (publisher_onion, channel_type, is_own)
    ViewOwnChannel,
    ToggleNotifications,
    #[allow(dead_code)]
    SendPresence(crate::protocol::message::PresenceType),
    Quit,
}

impl AppState {
    pub fn handle_key(&mut self, key: KeyEvent, friend_count: usize) -> Result<Option<AppAction>> {
        // Check global keys first
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(Some(AppAction::Quit));
        }

        match self {
            AppState::Normal { .. } => self.handle_normal_key(key, friend_count),

            AppState::AddingFriend { .. } => self.handle_adding_friend_key(key),

            AppState::ViewingFriendRequests { .. } => self.handle_viewing_friend_requests_key(key),

            AppState::ViewingFriendRequest { .. } => self.handle_viewing_friend_request_key(key),

            AppState::ViewingMyIdentity { .. } => self.handle_viewing_my_identity_key(key),

            AppState::SettingEphemeral { .. } => self.handle_setting_ephemeral_key(key),

            AppState::ViewingChannel { .. } => self.handle_viewing_channel_key(key),

            AppState::SubscribingToChannel { .. } => self.handle_subscribing_to_channel_key(key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn default_state_is_normal() {
        let state = AppState::default();
        match &state {
            AppState::Normal {
                selected_friend_idx,
                conversation_id,
                input,
                cursor,
                input_focused,
                scroll_offset,
            } => {
                assert_eq!(*selected_friend_idx, None);
                assert_eq!(*conversation_id, None);
                assert_eq!(input, "");
                assert_eq!(*cursor, 0);
                assert!(!input_focused);
                assert_eq!(*scroll_offset, 0);
            }
            _ => panic!("Expected Normal state"),
        }
    }

    #[test]
    fn ctrl_c_quits_from_any_state() {
        // From Normal
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(action, Some(AppAction::Quit));

        // From AddingFriend
        let mut state = AppState::AddingFriend {
            input: String::new(),
            cursor: 0,
            error: None,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(action, Some(AppAction::Quit));

        // From ViewingMyIdentity
        let mut state = AppState::ViewingMyIdentity {
            friend_code: "test".to_string(),
            onion_address: "test.onion".to_string(),
            copied_field: None,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key, 10).unwrap();
        assert_eq!(action, Some(AppAction::Quit));
    }
}
