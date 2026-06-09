use super::RpcResponse;
use crate::app::App;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) async fn handle_status(id: Option<Value>, app: &Arc<Mutex<App>>) -> RpcResponse {
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

pub(super) async fn handle_identity(id: Option<Value>, app: &Arc<Mutex<App>>) -> RpcResponse {
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

pub(super) async fn handle_notifications_toggle(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
) -> RpcResponse {
    let app = app.lock().await;
    let new_state = crate::notifications::toggle(&app.db);
    RpcResponse::success(
        id,
        serde_json::json!({
            "enabled": new_state,
        }),
    )
}
