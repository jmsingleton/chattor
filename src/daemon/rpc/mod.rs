mod channels;
mod friends;

use crate::app::App;
use crate::presence::PresenceMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Deserialize)]
pub struct RpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        RpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: String) -> Self {
        RpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError { code, message }),
        }
    }
}

/// Dispatch a JSON-RPC request to the appropriate handler.
///
/// Takes `Arc<Mutex<App>>` to allow handlers to acquire the lock, perform
/// synchronous work, and release it before any async I/O (e.g. pool.send).
pub async fn dispatch(
    req: &RpcRequest,
    app: &Arc<Mutex<App>>,
    presence: &PresenceMap,
) -> RpcResponse {
    let id = req.id.clone();
    match req.method.as_str() {
        "status" => handle_status(id, app).await,
        "identity" => handle_identity(id, app).await,
        "friends_list" => friends::handle_friends_list(id, app, presence).await,
        "friends_add" => friends::handle_friends_add(id, app, &req.params).await,
        "friends_requests" => friends::handle_friends_requests(id, app).await,
        "friends_accept" => friends::handle_friends_accept(id, app, &req.params).await,
        "friends_reject" => friends::handle_friends_reject(id, app, &req.params).await,
        "send_message" => handle_send_message(id, app, &req.params).await,
        "recv_messages" => handle_recv_messages(id, app, &req.params).await,
        "channels_list" => channels::handle_channels_list(id, app).await,
        "channels_publish" => channels::handle_channels_publish(id, app, &req.params).await,
        "channels_subscribe" => channels::handle_channels_subscribe(id, app, &req.params).await,
        "channels_feed" => channels::handle_channels_feed(id, app, &req.params).await,
        "ephemeral_set" => handle_ephemeral_set(id, app, &req.params).await,
        "notifications_toggle" => handle_notifications_toggle(id, app).await,
        _ => RpcResponse::error(id, -32601, format!("Method not found: {}", req.method)),
    }
}

// ---------------------------------------------------------------------------
// Handler implementations
// ---------------------------------------------------------------------------

async fn handle_status(id: Option<Value>, app: &Arc<Mutex<App>>) -> RpcResponse {
    let app = app.lock().await;
    RpcResponse::success(
        id,
        serde_json::json!({
            "daemon": true,
            "tor_connected": app.tor_client.is_some(),
            "onion_address": app.onion_address.as_deref().unwrap_or(""),
        }),
    )
}

async fn handle_identity(id: Option<Value>, app: &Arc<Mutex<App>>) -> RpcResponse {
    let app = app.lock().await;
    let onion = app.onion_address.as_deref().unwrap_or("");
    let friend_code = if !onion.is_empty() {
        crate::protocol::friend_code::onion_to_friend_code(onion).unwrap_or_default()
    } else {
        String::new()
    };
    RpcResponse::success(
        id,
        serde_json::json!({
            "friend_code": friend_code,
            "onion_address": onion,
        }),
    )
}

async fn handle_send_message(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let peer = match params.get("peer").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'peer' parameter".into()),
    };
    let message = match params.get("message").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'message' parameter".into()),
    };

    // Resolve peer (onion or friend code)
    let peer_onion = if peer.ends_with(".onion") {
        peer
    } else {
        match crate::protocol::friend_code::friend_code_to_onion(&peer) {
            Ok(onion) => onion,
            Err(_) => {
                return RpcResponse::error(id, -32602, "Invalid peer address or friend code".into())
            }
        }
    };

    // Do all DB work under the lock, build the wire message, then release
    // the lock before attempting the async pool.send().
    let (wire_msg, message_id, pool) = {
        let app = app.lock().await;

        // Find friend and conversation
        let friend_id = match crate::db::queries::find_friend_by_onion(&app.db, &peer_onion) {
            Ok(Some(fid)) => fid,
            Ok(None) => return RpcResponse::error(id, -32000, "Peer is not a friend".into()),
            Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
        };
        let conv_id = match crate::db::queries::get_or_create_conversation(&app.db, friend_id) {
            Ok(cid) => cid,
            Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
        };

        let own_onion = app.onion_address.as_deref().unwrap_or("").to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let msg_id = uuid::Uuid::new_v4();

        let ttl =
            crate::db::queries::get_conversation_ephemeral_ttl(&app.db, conv_id).unwrap_or(None);

        // Store locally
        crate::db::queries::store_outgoing_message_with_ttl(
            &app.db,
            conv_id,
            &own_onion,
            &message,
            &msg_id.to_string(),
            ttl,
        )
        .ok();

        // Encrypt with Signal Protocol
        let store = crate::crypto::SessionStore::new(&app.db);
        match store.load_session(&peer_onion) {
            Ok(Some(mut session)) => {
                let payload = crate::protocol::message::PlaintextPayload {
                    content: message.clone(),
                    sent_at: now,
                    message_type: "text".to_string(),
                    ephemeral_ttl: ttl,
                };
                let plaintext = match serde_json::to_vec(&payload) {
                    Ok(p) => p,
                    Err(e) => {
                        return RpcResponse::error(id, -32000, format!("Serialize error: {}", e))
                    }
                };

                let (header, ciphertext, is_prekey) = match session.encrypt(&plaintext) {
                    Ok(result) => result,
                    Err(e) => {
                        return RpcResponse::error(id, -32000, format!("Encrypt error: {}", e))
                    }
                };
                if let Err(e) = store.store_session(&session) {
                    return RpcResponse::error(id, -32000, format!("Session store error: {}", e));
                }

                use base64::Engine;
                let msg = crate::protocol::message::Message::TextMessage(
                    crate::protocol::message::TextMessage {
                        from_onion: own_onion,
                        to_onion: peer_onion.clone(),
                        signal_header: base64::engine::general_purpose::STANDARD.encode(&header),
                        signal_ciphertext: base64::engine::general_purpose::STANDARD
                            .encode(&ciphertext),
                        signal_type: if is_prekey {
                            crate::protocol::message::SignalMessageType::PrekeyMessage
                        } else {
                            crate::protocol::message::SignalMessageType::Message
                        },
                        timestamp: now,
                        message_id: msg_id,
                        x3dh_init: None,
                    },
                );

                let pool = app.connection_pool.as_ref().map(Arc::clone);
                (msg, msg_id, pool)
            }
            Ok(None) => {
                return RpcResponse::error(
                    id,
                    -32000,
                    format!("No encryption session with {}", peer_onion),
                )
            }
            Err(e) => return RpcResponse::error(id, -32000, format!("Session load error: {}", e)),
        }
    };
    // Lock released here

    // Try direct send (async, no lock held)
    if let Some(pool) = pool {
        if pool.send(&peer_onion, &wire_msg).await.is_ok() {
            return RpcResponse::success(
                id,
                serde_json::json!({
                    "status": "sent",
                    "message_id": message_id.to_string(),
                }),
            );
        }
    }

    // Queue for later delivery (needs lock briefly)
    {
        let app = app.lock().await;
        app.message_queue
            .enqueue(&app.db, &peer_onion, &wire_msg, "normal")
            .ok();
    }
    RpcResponse::success(
        id,
        serde_json::json!({
            "status": "queued",
            "message_id": message_id.to_string(),
        }),
    )
}

async fn handle_recv_messages(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let peer_filter = params
        .get("peer")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let app = app.lock().await;
    let friends = crate::db::queries::get_friends_with_unread(&app.db).unwrap_or_default();
    let mut all_messages: Vec<Value> = Vec::new();

    for f in &friends {
        if let Some(ref peer) = peer_filter {
            let peer_onion = if peer.ends_with(".onion") {
                peer.clone()
            } else {
                crate::protocol::friend_code::friend_code_to_onion(peer).unwrap_or_default()
            };
            if f.onion_address != peer_onion {
                continue;
            }
        }

        let conv_id =
            crate::db::queries::get_or_create_conversation(&app.db, f.friend_id).unwrap_or(0);
        if conv_id == 0 {
            continue;
        }

        let messages =
            crate::db::queries::get_messages(&app.db, conv_id, limit, 0).unwrap_or_default();
        for m in &messages {
            all_messages.push(serde_json::json!({
                "id": m.id,
                "from": m.sender_onion,
                "message": m.content,
                "timestamp": m.timestamp,
                "status": m.status,
            }));
        }
    }

    RpcResponse::success(id, Value::Array(all_messages))
}

async fn handle_ephemeral_set(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let peer = match params.get("peer").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'peer' parameter".into()),
    };
    let ttl = params.get("ttl").and_then(|v| v.as_i64());

    // Resolve peer
    let peer_onion = if peer.ends_with(".onion") {
        peer
    } else {
        match crate::protocol::friend_code::friend_code_to_onion(&peer) {
            Ok(onion) => onion,
            Err(_) => {
                return RpcResponse::error(id, -32602, "Invalid peer address or friend code".into())
            }
        }
    };

    let app = app.lock().await;
    let friend_id = match crate::db::queries::find_friend_by_onion(&app.db, &peer_onion) {
        Ok(Some(fid)) => fid,
        Ok(None) => return RpcResponse::error(id, -32000, "Peer is not a friend".into()),
        Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
    };
    let conv_id = match crate::db::queries::get_or_create_conversation(&app.db, friend_id) {
        Ok(cid) => cid,
        Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
    };

    match crate::db::queries::set_conversation_ephemeral_ttl(&app.db, conv_id, ttl) {
        Ok(()) => RpcResponse::success(
            id,
            serde_json::json!({
                "status": "ok",
                "ttl": ttl,
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("{}", e)),
    }
}

async fn handle_notifications_toggle(id: Option<Value>, app: &Arc<Mutex<App>>) -> RpcResponse {
    let app = app.lock().await;
    let new_state = crate::notifications::toggle(&app.db);
    RpcResponse::success(
        id,
        serde_json::json!({
            "enabled": new_state,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> (Arc<Mutex<App>>, tempfile::TempDir, tempfile::TempDir) {
        let temp_config = tempfile::tempdir().unwrap();
        let temp_data = tempfile::tempdir().unwrap();
        let settings = crate::config::Settings {
            config_dir: temp_config.path().to_path_buf(),
            data_dir: temp_data.path().to_path_buf(),
            db_path: temp_data.path().join("test.db"),
            debug: false,
            tor_socks_port: 9050,
        };
        let app = App::new_with_settings(settings, None).unwrap();
        (Arc::new(Mutex::new(app)), temp_config, temp_data)
    }

    #[test]
    fn test_rpc_response_success() {
        let resp = RpcResponse::success(
            Some(Value::Number(1.into())),
            serde_json::json!({"ok": true}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_rpc_response_error() {
        let resp = RpcResponse::error(Some(Value::Number(1.into())), -32601, "Not found".into());
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_parse_rpc_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"status","params":{}}"#;
        let req: RpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "status");
        assert_eq!(req.id, Some(Value::Number(1.into())));
    }

    #[test]
    fn test_parse_rpc_request_no_params() {
        let json = r#"{"jsonrpc":"2.0","id":null,"method":"identity"}"#;
        let req: RpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "identity");
        assert_eq!(req.params, Value::Null);
    }

    #[test]
    fn test_rpc_response_null_id() {
        let resp = RpcResponse::success(None, serde_json::json!({"ok": true}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"id\":null"));
    }

    #[test]
    fn test_rpc_error_codes() {
        let resp = RpcResponse::error(Some(Value::Number(1.into())), -32700, "Parse error".into());
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("-32700"));
        assert!(json.contains("Parse error"));
    }

    #[tokio::test]
    async fn test_dispatch_unknown_method() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "nonexistent".into(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_dispatch_status() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "status".into(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["daemon"], true);
        assert_eq!(result["tor_connected"], false);
    }

    #[tokio::test]
    async fn test_dispatch_identity() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "identity".into(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        // No Tor = empty onion_address
        assert_eq!(result["onion_address"], "");
        assert_eq!(result["friend_code"], "");
    }

    #[tokio::test]
    async fn test_dispatch_friends_list_empty() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "friends_list".into(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), Value::Array(vec![]));
    }

    #[tokio::test]
    async fn test_dispatch_friends_requests_empty() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "friends_requests".into(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), Value::Array(vec![]));
    }

    #[tokio::test]
    async fn test_dispatch_channels_list() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "channels_list".into(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        let list = resp.result.unwrap();
        // Should have at least the two own channels (public + friends_only)
        assert!(list.as_array().unwrap().len() >= 2);
    }

    #[tokio::test]
    async fn test_dispatch_notifications_toggle() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        // Default is enabled, toggle should disable
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "notifications_toggle".into(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["enabled"], false);
    }

    #[tokio::test]
    async fn test_dispatch_recv_messages_empty() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "recv_messages".into(),
            params: serde_json::json!({}),
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), Value::Array(vec![]));
    }

    #[tokio::test]
    async fn test_dispatch_channels_feed_empty() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "channels_feed".into(),
            params: serde_json::json!({"channel_id": 1}),
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), Value::Array(vec![]));
    }

    #[tokio::test]
    async fn test_dispatch_send_message_missing_params() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "send_message".into(),
            params: serde_json::json!({}),
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn test_dispatch_ephemeral_set_missing_params() {
        let (app, _c, _d) = test_app();
        let presence = crate::presence::new_presence_map();

        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "ephemeral_set".into(),
            params: serde_json::json!({}),
        };
        let resp = dispatch(&req, &app, &presence).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32602);
    }
}
