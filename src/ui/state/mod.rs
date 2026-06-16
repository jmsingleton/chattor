mod adding_friend;
mod channel;
mod ephemeral;
mod friend_requests;
mod identity;
mod normal;

use crate::error::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What the sidebar cursor is on. Navigation order:
/// friends[0..n], OwnPublic, OwnFriends, subscriptions[0..m].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSelection {
    Friend(usize),
    OwnPublic,
    OwnFriends,
    Subscription(usize),
}

impl SidebarSelection {
    pub fn first(friend_count: usize) -> Self {
        if friend_count > 0 {
            SidebarSelection::Friend(0)
        } else {
            SidebarSelection::OwnPublic
        }
    }

    pub fn next(self, friend_count: usize, sub_count: usize) -> Self {
        use SidebarSelection::*;
        match self {
            Friend(i) if i + 1 < friend_count => Friend(i + 1),
            Friend(_) => OwnPublic,
            OwnPublic => OwnFriends,
            OwnFriends if sub_count > 0 => Subscription(0),
            OwnFriends => OwnFriends,
            Subscription(i) if i + 1 < sub_count => Subscription(i + 1),
            Subscription(i) => Subscription(i),
        }
    }

    pub fn prev(self, friend_count: usize, _sub_count: usize) -> Self {
        use SidebarSelection::*;
        match self {
            Friend(i) if i > 0 => Friend(i - 1),
            Friend(i) => Friend(i),
            OwnPublic if friend_count > 0 => Friend(friend_count - 1),
            OwnPublic => OwnPublic,
            OwnFriends => OwnPublic,
            Subscription(0) => OwnFriends,
            Subscription(i) => Subscription(i - 1),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppState {
    Normal {
        selected: Option<SidebarSelection>,
        conversation_id: Option<i64>,
        input: String,
        cursor: usize,
        input_focused: bool,
        scroll_offset: usize,
    },
    AddingFriend {
        input: Box<crate::ui::widgets::text_input::TextInput>,
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
        input: Box<crate::ui::widgets::text_input::TextInput>,
        error: Option<String>,
    },
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Normal {
            selected: None,
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
    SelectSubscription(usize),          // index into cached subscriptions
    ViewOwnChannel(String),             // channel_type ("public" or "friends_only")
    ToggleNotifications,
    #[allow(dead_code)]
    SendPresence(crate::protocol::message::PresenceType),
    Quit,
}

impl AppState {
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        friend_count: usize,
        sub_count: usize,
    ) -> Result<Option<AppAction>> {
        // Check global keys first
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(Some(AppAction::Quit));
        }

        match self {
            AppState::Normal { .. } => self.handle_normal_key(key, friend_count, sub_count),

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
    fn sidebar_selection_walks_friends_then_channels() {
        use SidebarSelection::*;
        assert_eq!(Friend(0).next(2, 1), Friend(1));
        assert_eq!(Friend(1).next(2, 1), OwnPublic);
        assert_eq!(OwnPublic.next(2, 1), OwnFriends);
        assert_eq!(OwnFriends.next(2, 1), Subscription(0));
        assert_eq!(Subscription(0).next(2, 1), Subscription(0)); // bottom stop
        assert_eq!(OwnFriends.next(2, 0), OwnFriends); // no subs
    }

    #[test]
    fn sidebar_selection_walks_back_up() {
        use SidebarSelection::*;
        assert_eq!(Subscription(0).prev(2, 1), OwnFriends);
        assert_eq!(OwnFriends.prev(2, 1), OwnPublic);
        assert_eq!(OwnPublic.prev(2, 1), Friend(1));
        assert_eq!(Friend(0).prev(2, 1), Friend(0)); // top stop
        assert_eq!(OwnPublic.prev(0, 1), OwnPublic); // no friends
    }

    #[test]
    fn sidebar_selection_first() {
        assert_eq!(SidebarSelection::first(3), SidebarSelection::Friend(0));
        assert_eq!(SidebarSelection::first(0), SidebarSelection::OwnPublic);
    }

    #[test]
    fn default_state_is_normal() {
        let state = AppState::default();
        match &state {
            AppState::Normal {
                selected,
                conversation_id,
                input,
                cursor,
                input_focused,
                scroll_offset,
            } => {
                assert_eq!(*selected, None);
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
        let action = state.handle_key(key, 10, 0).unwrap();
        assert_eq!(action, Some(AppAction::Quit));

        // From AddingFriend
        let mut state = AppState::AddingFriend {
            input: Box::new(crate::ui::widgets::text_input::TextInput::single_line("")),
            error: None,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key, 10, 0).unwrap();
        assert_eq!(action, Some(AppAction::Quit));

        // From ViewingMyIdentity
        let mut state = AppState::ViewingMyIdentity {
            friend_code: "test".to_string(),
            onion_address: "test.onion".to_string(),
            copied_field: None,
        };
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = state.handle_key(key, 10, 0).unwrap();
        assert_eq!(action, Some(AppAction::Quit));
    }
}
