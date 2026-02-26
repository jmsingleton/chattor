use crate::app::App;
use crate::db;
use crate::error::Result;
use crate::protocol;

/// Collect channel sync requests (synchronous, safe to call under lock)
pub fn collect_sync_requests(app: &App) -> Result<Vec<(String, protocol::message::Message)>> {
    let subscriptions = db::queries::get_channel_subscriptions(&app.db)?;
    let own_onion = app.onion_address.clone().unwrap_or_default();

    let mut requests = Vec::new();
    for sub in subscriptions {
        let since = sub.last_sync_at.unwrap_or(0);
        let channel_type = if sub.channel_type == "public" {
            protocol::message::ChannelType::Public
        } else {
            protocol::message::ChannelType::FriendsOnly
        };

        let sync_req = protocol::message::Message::ChannelSyncRequest(
            protocol::message::ChannelSyncRequestMessage {
                subscriber_onion: own_onion.clone(),
                channel_type,
                since_timestamp: since,
            },
        );

        requests.push((sub.publisher_onion, sync_req));
    }

    Ok(requests)
}
