mod channels;
mod friends;
mod messaging;
mod system;

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
        "status" => system::handle_status(id, app).await,
        "identity" => system::handle_identity(id, app).await,
        "friends_list" => friends::handle_friends_list(id, app, presence).await,
        "friends_add" => friends::handle_friends_add(id, app, &req.params).await,
        "friends_requests" => friends::handle_friends_requests(id, app).await,
        "friends_accept" => friends::handle_friends_accept(id, app, &req.params).await,
        "friends_reject" => friends::handle_friends_reject(id, app, &req.params).await,
        "send_message" => messaging::handle_send_message(id, app, &req.params).await,
        "recv_messages" => messaging::handle_recv_messages(id, app, &req.params).await,
        "channels_list" => channels::handle_channels_list(id, app).await,
        "channels_publish" => channels::handle_channels_publish(id, app, &req.params).await,
        "channels_subscribe" => channels::handle_channels_subscribe(id, app, &req.params).await,
        "channels_feed" => channels::handle_channels_feed(id, app, &req.params).await,
        "ephemeral_set" => messaging::handle_ephemeral_set(id, app, &req.params).await,
        "notifications_toggle" => system::handle_notifications_toggle(id, app).await,
        _ => RpcResponse::error(id, -32601, format!("Method not found: {}", req.method)),
    }
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
}
