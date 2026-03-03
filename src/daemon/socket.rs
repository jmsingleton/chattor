use super::rpc;
use crate::app::App;
use crate::presence::PresenceMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{broadcast, Mutex};

/// Constant-time byte comparison to prevent timing side-channel attacks on auth tokens.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Start the Unix socket server. Returns a JoinHandle for the accept loop.
pub async fn start(
    socket_path: &Path,
    app: Arc<Mutex<App>>,
    presence: PresenceMap,
    msg_broadcast_tx: broadcast::Sender<String>,
    auth_token: String,
) -> tokio::task::JoinHandle<()> {
    // Remove stale socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path).ok();
    }

    // Set restrictive umask BEFORE binding so the socket is created with 0600
    // from the start, eliminating the race window between bind and chmod.
    #[cfg(unix)]
    let old_umask = unsafe { libc::umask(0o077) };

    let listener = UnixListener::bind(socket_path).expect("Failed to bind Unix socket");

    // Restore original umask
    #[cfg(unix)]
    unsafe {
        libc::umask(old_umask);
    }

    // Explicitly set permissions as defense-in-depth (fail hard on error)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))
            .expect("Failed to set socket permissions to 0600");
    }

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let app = Arc::clone(&app);
                    let presence = presence.clone();
                    let broadcast_tx = msg_broadcast_tx.clone();
                    let auth_token = auth_token.clone();
                    tokio::spawn(async move {
                        handle_connection(stream, app, presence, broadcast_tx, auth_token).await;
                    });
                }
                Err(e) => {
                    eprintln!("Socket accept error: {}", e);
                }
            }
        }
    })
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    app: Arc<Mutex<App>>,
    presence: PresenceMap,
    msg_broadcast_tx: broadcast::Sender<String>,
    auth_token: String,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut authenticated = false;

    while let Ok(Some(line)) = lines.next_line().await {
        let request: rpc::RpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = rpc::RpcResponse::error(None, -32700, format!("Parse error: {}", e));
                let json = serde_json::to_string(&resp).unwrap_or_default();
                let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                continue;
            }
        };

        // Require auth on first request
        if !authenticated {
            let provided_token = request.params.get("auth").and_then(|v| v.as_str());
            match provided_token {
                Some(t) if constant_time_eq(t.as_bytes(), auth_token.as_bytes()) => {
                    authenticated = true;
                }
                _ => {
                    let resp = rpc::RpcResponse::error(
                        request.id.clone(),
                        -32001,
                        "Unauthorized: invalid or missing auth token".into(),
                    );
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                    return; // Disconnect unauthorized client
                }
            }
        }

        // Strip auth token from params before dispatching to RPC handlers
        let request = if let serde_json::Value::Object(mut map) = request.params {
            map.remove("auth");
            rpc::RpcRequest {
                jsonrpc: request.jsonrpc,
                id: request.id,
                method: request.method,
                params: serde_json::Value::Object(map),
            }
        } else {
            request
        };

        // Handle streaming `listen` method: subscribe to broadcast and
        // stream events until the client disconnects.
        if request.method == "listen" {
            let mut rx = msg_broadcast_tx.subscribe();

            // Send initial ACK response
            let ack = rpc::RpcResponse::success(
                request.id.clone(),
                serde_json::json!({"status": "listening"}),
            );
            if writer
                .write_all(
                    format!("{}\n", serde_json::to_string(&ack).unwrap_or_default()).as_bytes(),
                )
                .await
                .is_err()
            {
                return; // Client already disconnected
            }

            // Stream events until client disconnects or broadcast channel closes
            while let Ok(event) = rx.recv().await {
                if writer
                    .write_all(format!("{}\n", event).as_bytes())
                    .await
                    .is_err()
                {
                    break; // Client disconnected
                }
            }
            return; // Connection ends after listen
        }

        // dispatch acquires the app lock internally and releases it
        // before any async I/O (e.g. pool.send), avoiding Send issues.
        let response = rpc::dispatch(&request, &app, &presence).await;

        let json = serde_json::to_string(&response).unwrap_or_default();
        if writer
            .write_all(format!("{}\n", json).as_bytes())
            .await
            .is_err()
        {
            break; // Client disconnected
        }
    }
}
