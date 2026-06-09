use super::RpcResponse;
use crate::app::App;
use crate::presence::PresenceMap;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) async fn handle_friends_list(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    presence: &PresenceMap,
) -> RpcResponse {
    let friends = {
        let app = app.lock().await;
        crate::db::queries::get_friends_with_unread(&app.db).unwrap_or_default()
    };
    let presence_snapshot = crate::presence::get_presence_snapshot(presence).await;
    let list: Vec<Value> = friends
        .iter()
        .map(|f| {
            let (online, typing) = presence_snapshot
                .get(&f.onion_address)
                .copied()
                .unwrap_or((false, false));
            serde_json::json!({
                "friend_id": f.friend_id,
                "onion_address": f.onion_address,
                "display_name": f.display(),
                "unread_count": f.unread_count,
                "online": online,
                "typing": typing,
            })
        })
        .collect();
    RpcResponse::success(id, Value::Array(list))
}

pub(super) async fn handle_friends_add(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let code = match params.get("code").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'code' parameter".into()),
    };

    // Build the friend request message and extract pool under the lock,
    // then release the lock before the async pool.send().
    let (message, peer_onion, pool) = {
        let app_lock = app.lock().await;

        let trimmed = code.trim();
        let resolved = if trimmed.ends_with(".onion") {
            trimmed.to_string()
        } else {
            match crate::protocol::friend_code::friend_code_to_onion(trimmed) {
                Ok(onion) => onion,
                Err(_) => {
                    return RpcResponse::error(
                        id,
                        -32000,
                        "Enter a .onion address or friend code".into(),
                    )
                }
            }
        };

        let own_onion = match app_lock.onion_address.as_ref() {
            Some(o) => o.clone(),
            None => return RpcResponse::error(id, -32000, "Tor not initialized yet".into()),
        };

        let own_friend_code = crate::tor::address::onion_to_friend_code(&own_onion)
            .unwrap_or_else(|_| "unknown".to_string());

        let identity = match app_lock.identity.as_ref() {
            Some(i) => i,
            None => return RpcResponse::error(id, -32000, "Identity not initialized".into()),
        };

        let request_msg =
            match crate::protocol::friend_request::FriendRequestHandler::create_request(
                identity,
                &own_onion,
                &own_friend_code,
            ) {
                Ok(msg) => msg,
                Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
            };

        let msg = crate::protocol::message::Message::FriendRequest(request_msg);
        let pool = app_lock.connection_pool.as_ref().map(Arc::clone);
        (msg, resolved, pool)
    };
    // Lock released here

    // Try direct send (no lock held)
    if let Some(pool) = pool {
        if pool.send(&peer_onion, &message).await.is_ok() {
            return RpcResponse::success(id, serde_json::json!({ "status": "sent" }));
        }
    }

    // Queue for background delivery
    {
        let app_lock = app.lock().await;
        app_lock
            .message_queue
            .enqueue(&app_lock.db, &peer_onion, &message, "high")
            .ok();
    }
    RpcResponse::success(id, serde_json::json!({ "status": "queued" }))
}

pub(super) async fn handle_friends_requests(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
) -> RpcResponse {
    let app = app.lock().await;
    let requests = crate::db::queries::get_pending_friend_requests(&app.db).unwrap_or_default();
    let list: Vec<Value> = requests
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "from_onion": r.from_onion,
                "friend_code": r.friend_code,
                "received_at": r.received_at,
            })
        })
        .collect();
    RpcResponse::success(id, Value::Array(list))
}

pub(super) async fn handle_friends_accept(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let req_id = match params.get("id").and_then(|v| v.as_i64()) {
        Some(i) => i,
        None => return RpcResponse::error(id, -32602, "Missing 'id' parameter".into()),
    };
    let app = app.lock().await;
    match crate::handlers::friend_request::handle_accept_friend_request(&app, req_id) {
        Ok(()) => RpcResponse::success(id, serde_json::json!({ "status": "accepted" })),
        Err(e) => RpcResponse::error(id, -32000, format!("{}", e)),
    }
}

pub(super) async fn handle_friends_reject(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let req_id = match params.get("id").and_then(|v| v.as_i64()) {
        Some(i) => i,
        None => return RpcResponse::error(id, -32602, "Missing 'id' parameter".into()),
    };
    let app = app.lock().await;
    match crate::handlers::friend_request::handle_reject_friend_request(&app, req_id) {
        Ok(()) => RpcResponse::success(id, serde_json::json!({ "status": "rejected" })),
        Err(e) => RpcResponse::error(id, -32000, format!("{}", e)),
    }
}
