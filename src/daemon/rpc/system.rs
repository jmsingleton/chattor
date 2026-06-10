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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::rpc::{dispatch, RpcRequest};

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
}
