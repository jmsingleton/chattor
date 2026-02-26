use super::rpc;
use crate::app::App;
use crate::presence::PresenceMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

/// Start the Unix socket server. Returns a JoinHandle for the accept loop.
pub async fn start(
    socket_path: &Path,
    app: Arc<Mutex<App>>,
    presence: PresenceMap,
) -> tokio::task::JoinHandle<()> {
    // Remove stale socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path).ok();
    }

    let listener = UnixListener::bind(socket_path).expect("Failed to bind Unix socket");

    // Restrict permissions to owner only (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600)).ok();
    }

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let app = Arc::clone(&app);
                    let presence = presence.clone();
                    tokio::spawn(async move {
                        handle_connection(stream, app, presence).await;
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
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

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
