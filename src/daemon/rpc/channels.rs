use super::RpcResponse;
use crate::app::App;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) async fn handle_channels_list(id: Option<Value>, app: &Arc<Mutex<App>>) -> RpcResponse {
    let app = app.lock().await;
    // Own channels are always id 1 (public) and id 2 (friends_only)
    let mut list: Vec<Value> = vec![
        serde_json::json!({
            "id": 1,
            "type": "public",
            "publisher": "self",
        }),
        serde_json::json!({
            "id": 2,
            "type": "friends_only",
            "publisher": "self",
        }),
    ];

    let subs = crate::db::queries::get_channel_subscriptions(&app.db).unwrap_or_default();
    for s in &subs {
        list.push(serde_json::json!({
            "id": s.id,
            "type": s.channel_type,
            "publisher": s.publisher_onion,
        }));
    }

    RpcResponse::success(id, Value::Array(list))
}

pub(super) async fn handle_channels_publish(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let channel_type = match params.get("channel").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'channel' parameter".into()),
    };
    let content = match params.get("content").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'content' parameter".into()),
    };

    let channel_id: i64 = match channel_type.as_str() {
        "public" => 1,
        "friends_only" => 2,
        _ => return RpcResponse::error(id, -32602, "Invalid channel type".into()),
    };

    let post_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let app = app.lock().await;

    // Sign the post content
    let signature = match &app.identity {
        Some(identity) => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .encode(identity.sign(content.as_bytes()).to_bytes())
        }
        None => return RpcResponse::error(id, -32000, "Identity not initialized".into()),
    };

    // Store locally
    if let Err(e) = crate::db::queries::store_channel_post(
        &app.db, channel_id, &content, &post_id, now, &signature,
    ) {
        return RpcResponse::error(id, -32000, format!("{}", e));
    }

    // Enforce retention
    crate::db::queries::enforce_channel_retention(&app.db, channel_id).ok();

    // Distribute to subscribers
    let subscribers =
        crate::db::queries::get_channel_subscribers(&app.db, &channel_type).unwrap_or_default();
    let own_onion = app.onion_address.as_deref().unwrap_or("").to_string();
    let wire_channel_type = if channel_type == "public" {
        crate::protocol::message::ChannelType::Public
    } else {
        crate::protocol::message::ChannelType::FriendsOnly
    };
    let post_msg = crate::protocol::message::Message::ChannelPost(
        crate::protocol::message::ChannelPostMessage {
            publisher_onion: own_onion,
            channel_type: wire_channel_type,
            post_id: uuid::Uuid::parse_str(&post_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            content,
            created_at: now,
            signature: signature.clone(),
        },
    );

    for sub in &subscribers {
        app.message_queue
            .enqueue(&app.db, sub, &post_msg, "normal")
            .ok();
    }

    RpcResponse::success(
        id,
        serde_json::json!({
            "status": "published",
            "post_id": post_id,
        }),
    )
}

pub(super) async fn handle_channels_subscribe(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let publisher = match params.get("publisher").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'publisher' parameter".into()),
    };
    let channel_type = match params.get("channel").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'channel' parameter".into()),
    };

    // Resolve publisher (onion or friend code)
    let publisher_onion = if publisher.ends_with(".onion") {
        publisher
    } else {
        match crate::protocol::friend_code::friend_code_to_onion(&publisher) {
            Ok(onion) => onion,
            Err(_) => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Invalid publisher address or friend code".into(),
                )
            }
        }
    };

    // Build the subscribe message and try to send, all under lock then release
    let (sub_msg, pool) = {
        let app = app.lock().await;

        // Store subscription locally
        if let Err(e) =
            crate::db::queries::add_channel_subscription(&app.db, &publisher_onion, &channel_type)
        {
            return RpcResponse::error(id, -32000, format!("{}", e));
        }

        let wire_channel_type = if channel_type == "public" {
            crate::protocol::message::ChannelType::Public
        } else {
            crate::protocol::message::ChannelType::FriendsOnly
        };
        let own_onion = app.onion_address.as_deref().unwrap_or("").to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let msg = crate::protocol::message::Message::ChannelSubscribe(
            crate::protocol::message::ChannelSubscribeMessage {
                subscriber_onion: own_onion,
                channel_type: wire_channel_type,
                timestamp: now,
            },
        );

        let pool = app.connection_pool.as_ref().map(Arc::clone);
        (msg, pool)
    };
    // Lock released here

    // Try direct send (no lock held)
    let mut sent = false;
    if let Some(pool) = pool {
        if pool.send(&publisher_onion, &sub_msg).await.is_ok() {
            sent = true;
        }
    }

    if !sent {
        // Queue for background delivery
        let app = app.lock().await;
        app.message_queue
            .enqueue(&app.db, &publisher_onion, &sub_msg, "normal")
            .ok();
    }

    RpcResponse::success(id, serde_json::json!({ "status": "subscribed" }))
}

pub(super) async fn handle_channels_feed(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let channel_id = match params.get("channel_id").and_then(|v| v.as_i64()) {
        Some(cid) => cid,
        None => return RpcResponse::error(id, -32602, "Missing 'channel_id' parameter".into()),
    };
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let app = app.lock().await;
    let posts =
        crate::db::queries::get_channel_posts(&app.db, channel_id, limit).unwrap_or_default();
    let list: Vec<Value> = posts
        .iter()
        .map(|p| {
            serde_json::json!({
                "post_id": p.post_id,
                "content": p.content,
                "created_at": p.created_at,
            })
        })
        .collect();
    RpcResponse::success(id, Value::Array(list))
}
