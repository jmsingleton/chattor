# CLI, Daemon & MCP Server Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a headless daemon mode, CLI subcommands, and MCP server to chattor so AI agents can send and receive private P2P messages programmatically.

**Architecture:** Extract message handlers from main.rs into `src/handlers/`, create a daemon event loop using `tokio::select!`, add a JSON-RPC 2.0 Unix socket server, wrap CLI subcommands around socket requests, and expose MCP tools over stdio. The TUI remains unchanged and mutually exclusive with the daemon.

**Tech Stack:** Rust, tokio, serde_json (JSON-RPC), clap (subcommands), Unix domain sockets (tokio::net::UnixListener)

---

## Task 1: Extract Message Handlers from main.rs

The first step is to extract the ~900 lines of message handling functions from `main.rs` into a reusable `src/handlers/` module. This is pure refactoring — no behavior change. Both the TUI and the future daemon will call these same functions.

**Files:**
- Create: `src/handlers/mod.rs`
- Create: `src/handlers/friend_request.rs`
- Create: `src/handlers/messaging.rs`
- Create: `src/handlers/channels.rs`
- Modify: `src/main.rs` (replace inline functions with `handlers::` calls)

**Step 1: Create `src/handlers/mod.rs`**

```rust
pub mod friend_request;
pub mod messaging;
pub mod channels;
```

Add `mod handlers;` to `src/main.rs` (near the top, with the other `mod` declarations).

**Step 2: Extract friend request handlers to `src/handlers/friend_request.rs`**

Move these functions from `main.rs`:
- `handle_send_friend_request()` (lines ~1079-1132) — creates and queues/sends a friend request
- `handle_accept_friend_request()` (lines ~1134-1276) — generates PreKey bundle, stores private keys, queues accept
- `handle_reject_friend_request()` (lines ~1278-1294) — deletes the request from DB
- `handle_incoming_accept()` (lines ~1834-1994) — X3DH session establishment from received accept message
- `SendResult` enum (lines ~1070-1076)

The functions should keep their exact current signatures. Add necessary `use` imports at the top of the new file:

```rust
use crate::app::App;
use crate::error::{self, Result, ChattorError};
use crate::db;
use crate::protocol;
use crate::net;
use std::sync::Arc;
use base64::Engine;
```

In `main.rs`, replace the moved functions with:

```rust
use handlers::friend_request::{handle_send_friend_request, handle_accept_friend_request, handle_reject_friend_request, SendResult};
```

And update the call to `handle_incoming_accept` inside `handle_incoming_message` (which we'll move next).

**Step 3: Extract messaging handlers to `src/handlers/messaging.rs`**

Move from `main.rs`:
- `handle_incoming_message()` (lines ~1316-1752) — the big 400+ line function that routes incoming messages by type
- `process_message_queue()` (lines ~1754-1832) — groups pending messages by peer, sends with semaphore
- `try_send_direct()` (lines ~1302-1314) — sends a message via connection pool

These call into `handle_incoming_accept` from the friend_request module, so add:

```rust
use super::friend_request::handle_incoming_accept;
```

**Step 4: Extract channel sync to `src/handlers/channels.rs`**

Move from `main.rs`:
- `collect_sync_requests()` (lines ~1996-2022) — generates sync requests for subscribed channels

**Step 5: Update main.rs to use handlers module**

Replace all moved functions with imports. The event loop in main.rs should now call:
- `handlers::friend_request::handle_send_friend_request(&app_lock, &code).await`
- `handlers::messaging::handle_incoming_message(&app_lock, incoming, &presence_map).await`
- `handlers::messaging::process_message_queue(&app_lock).await`
- etc.

**Step 6: Run tests and commit**

```bash
cargo test
cargo clippy -- -D warnings
git add src/handlers/ src/main.rs
git commit -m "refactor: extract message handlers from main.rs into src/handlers/"
```

---

## Task 2: Create Daemon Module with Event Loop

Create the daemon module that runs the same background tasks as the TUI but without terminal rendering. Uses `tokio::select!` for event-driven message processing.

**Files:**
- Create: `src/daemon/mod.rs`
- Create: `src/daemon/event_loop.rs`
- Create: `src/daemon/tasks.rs`
- Create: `src/daemon/pid.rs`

**Step 1: Create `src/daemon/mod.rs`**

```rust
pub mod event_loop;
pub mod tasks;
pub mod pid;

use crate::app::App;
use crate::error::Result;
use crate::config::Settings;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Start the daemon: bootstrap Tor, spawn background tasks, run event loop.
pub async fn run(settings: Settings) -> Result<()> {
    let app = Arc::new(Mutex::new(App::new_with_settings(settings.clone())?));

    // Ensure identity exists
    {
        let mut app_lock = app.lock().await;
        if app_lock.identity.is_none() {
            let identity = crate::crypto::IdentityKeypair::generate();
            identity.save_to_db(&app_lock.db)?;
            app_lock.identity = Some(identity);
        }
    }

    // PID file
    let pid_path = settings.data_dir.join("chattor.pid");
    pid::acquire(&pid_path)?;

    // Bootstrap Tor
    eprintln!("Bootstrapping Tor...");
    {
        let mut app_lock = app.lock().await;
        app_lock.init_tor().await?;
    }

    let onion = {
        let app_lock = app.lock().await;
        app_lock.onion_address.clone().unwrap_or_default()
    };
    eprintln!("Tor ready: {}", onion);

    // Pool watch channel for heartbeat
    let (pool_tx, pool_rx) = tokio::sync::watch::channel(None);
    {
        let app_lock = app.lock().await;
        if let Some(ref pool) = app_lock.connection_pool {
            let _ = pool_tx.send(Some(Arc::clone(pool)));
        }
    }

    // Spawn background tasks
    tasks::spawn_all(Arc::clone(&app), pool_rx).await;

    // Run event loop (blocks until shutdown signal)
    let result = event_loop::run(Arc::clone(&app)).await;

    // Cleanup
    pid::release(&pid_path);
    result
}
```

Add `mod daemon;` to `src/main.rs`.

**Step 2: Create `src/daemon/pid.rs`**

```rust
use crate::error::{Result, ChattorError};
use std::path::Path;
use std::fs;

/// Acquire a PID file. Returns error if another instance is running.
pub fn acquire(path: &Path) -> Result<()> {
    if path.exists() {
        let contents = fs::read_to_string(path)
            .map_err(|e| ChattorError::Io(e))?;
        if let Ok(pid) = contents.trim().parse::<u32>() {
            // Check if process is still running
            if process_exists(pid) {
                return Err(ChattorError::Tor(
                    format!("Another chattor instance is running (PID {}). Stop it first or remove {}", pid, path.display())
                ));
            }
        }
        // Stale PID file — remove it
        fs::remove_file(path).ok();
    }

    fs::write(path, format!("{}", std::process::id()))
        .map_err(|e| ChattorError::Io(e))?;
    Ok(())
}

/// Release the PID file.
pub fn release(path: &Path) {
    fs::remove_file(path).ok();
}

/// Check if a process with the given PID exists.
fn process_exists(pid: u32) -> bool {
    // On Unix, signal 0 checks existence without actually sending a signal
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
```

Note: add `libc` to Cargo.toml dependencies (it's likely already present as a transitive dep, but add it explicitly).

**Step 3: Create `src/daemon/tasks.rs`**

Extract background task spawning from main.rs (lines 217-288):

```rust
use crate::app::App;
use crate::net::pool::ConnectionPool;
use crate::handlers;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Spawn all background tasks (channel sync, heartbeat, etc.)
pub async fn spawn_all(
    app: Arc<Mutex<App>>,
    pool_rx: tokio::sync::watch::Receiver<Option<Arc<ConnectionPool>>>,
) {
    spawn_channel_sync(Arc::clone(&app));
    spawn_heartbeat(Arc::clone(&app), pool_rx);
}

fn spawn_channel_sync(app: Arc<Mutex<App>>) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        loop {
            {
                let app_lock = app.lock().await;
                if let Ok(requests) = handlers::channels::collect_sync_requests(&app_lock) {
                    for (peer_onion, sync_msg) in requests {
                        app_lock.message_queue.enqueue(&app_lock.db, &peer_onion, &sync_msg, "low").ok();
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        }
    });
}

fn spawn_heartbeat(
    app: Arc<Mutex<App>>,
    pool_rx: tokio::sync::watch::Receiver<Option<Arc<ConnectionPool>>>,
) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        let own_onion = {
            let app_lock = app.lock().await;
            app_lock.onion_address.clone().unwrap_or_default()
        };

        loop {
            let pool = { pool_rx.borrow().clone() };
            if let Some(pool) = pool {
                let peers = pool.connected_peers();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                let mut tasks = tokio::task::JoinSet::new();
                for peer in peers {
                    let msg = crate::protocol::message::Message::Presence(
                        crate::protocol::message::PresenceMessage {
                            from_onion: own_onion.clone(),
                            presence_type: crate::protocol::message::PresenceType::Heartbeat,
                            timestamp: now,
                        }
                    );
                    let pool = Arc::clone(&pool);
                    tasks.spawn(async move {
                        let _ = pool.send(&peer, &msg).await;
                    });
                }
                while let Some(_) = tasks.join_next().await {}
            }
            tokio::time::sleep(crate::presence::HEARTBEAT_INTERVAL).await;
        }
    });
}
```

**Step 4: Create `src/daemon/event_loop.rs`**

```rust
use crate::app::App;
use crate::error::Result;
use crate::handlers;
use crate::presence;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

/// Run the daemon event loop. Processes incoming messages and queue commands.
/// Blocks until SIGTERM/SIGINT.
pub async fn run(app: Arc<Mutex<App>>) -> Result<()> {
    let presence_map = presence::new_presence_map();

    // Take ownership of channels from app
    let mut incoming_rx = {
        let mut app_lock = app.lock().await;
        app_lock.incoming_message_rx.take()
    };
    let mut queue_rx = {
        let mut app_lock = app.lock().await;
        app_lock.queue_command_rx.take()
    };

    let mut presence_tick = tokio::time::interval(Duration::from_secs(1));
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    eprintln!("Daemon running. Press Ctrl+C to stop.");

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                eprintln!("Shutting down...");
                break;
            }

            msg = async { match incoming_rx.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                if let Some(incoming) = msg {
                    let app_lock = app.lock().await;
                    if let Err(e) = handlers::messaging::handle_incoming_message(&app_lock, incoming, &presence_map).await {
                        eprintln!("Incoming message error: {}", e);
                    }
                }
            }

            cmd = async { match queue_rx.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                if cmd.is_some() {
                    let app_lock = app.lock().await;
                    if let Err(e) = handlers::messaging::process_message_queue(&app_lock).await {
                        eprintln!("Queue processing error: {}", e);
                    }
                }
            }

            _ = presence_tick.tick() => {
                // Presence cleanup handled by in-memory expiry
            }
        }
    }

    Ok(())
}
```

**Step 5: Run tests and commit**

```bash
cargo test
cargo clippy -- -D warnings
git add src/daemon/ src/main.rs
git commit -m "feat: add daemon module with event loop, background tasks, PID file"
```

---

## Task 3: Add Unix Socket JSON-RPC Server

Add a JSON-RPC 2.0 server to the daemon that listens on a Unix domain socket for client commands.

**Files:**
- Create: `src/daemon/rpc.rs` (JSON-RPC types and dispatch)
- Create: `src/daemon/socket.rs` (Unix socket server)
- Modify: `src/daemon/mod.rs` (wire socket into daemon startup)
- Modify: `src/daemon/event_loop.rs` (add socket to select! loop)

**Step 1: Create `src/daemon/rpc.rs`**

Define JSON-RPC 2.0 request/response types and a dispatch function:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::app::App;
use crate::error::Result;
use crate::presence::PresenceMap;

#[derive(Deserialize)]
pub struct RpcRequest {
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
        RpcResponse { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    pub fn error(id: Option<Value>, code: i32, message: String) -> Self {
        RpcResponse { jsonrpc: "2.0".into(), id, result: None, error: Some(RpcError { code, message }) }
    }
}

/// Dispatch an RPC request to the appropriate handler.
pub async fn dispatch(
    req: &RpcRequest,
    app: &App,
    presence: &PresenceMap,
) -> RpcResponse {
    let id = req.id.clone();
    match req.method.as_str() {
        "status" => handle_status(id, app).await,
        "identity" => handle_identity(id, app).await,
        "friends_list" => handle_friends_list(id, app, presence).await,
        "friends_add" => handle_friends_add(id, app, &req.params).await,
        "friends_requests" => handle_friends_requests(id, app).await,
        "friends_accept" => handle_friends_accept(id, app, &req.params).await,
        "friends_reject" => handle_friends_reject(id, app, &req.params).await,
        "send_message" => handle_send_message(id, app, &req.params).await,
        "recv_messages" => handle_recv_messages(id, app, &req.params).await,
        "channels_list" => handle_channels_list(id, app).await,
        "channels_publish" => handle_channels_publish(id, app, &req.params).await,
        "channels_subscribe" => handle_channels_subscribe(id, app, &req.params).await,
        "channels_feed" => handle_channels_feed(id, app, &req.params).await,
        "ephemeral_set" => handle_ephemeral_set(id, app, &req.params).await,
        "notifications_toggle" => handle_notifications_toggle(id, app, &req.params).await,
        _ => RpcResponse::error(id, -32601, format!("Method not found: {}", req.method)),
    }
}
```

Then implement each handler. Example for `status`:

```rust
async fn handle_status(id: Option<Value>, app: &App) -> RpcResponse {
    RpcResponse::success(id, serde_json::json!({
        "daemon": true,
        "tor_connected": app.tor_client.is_some(),
        "onion_address": app.onion_address.as_deref().unwrap_or(""),
    }))
}
```

Example for `friends_list`:

```rust
async fn handle_friends_list(id: Option<Value>, app: &App, presence: &PresenceMap) -> RpcResponse {
    let friends = crate::db::queries::get_friends_with_unread(&app.db).unwrap_or_default();
    let presence_snapshot = crate::presence::get_presence_snapshot(presence).await;
    let list: Vec<Value> = friends.iter().map(|f| {
        let (online, typing) = presence_snapshot.get(&f.onion_address).copied().unwrap_or((false, false));
        serde_json::json!({
            "friend_id": f.friend_id,
            "onion_address": f.onion_address,
            "display_name": f.display(),
            "unread_count": f.unread_count,
            "online": online,
            "typing": typing,
        })
    }).collect();
    RpcResponse::success(id, Value::Array(list))
}
```

Example for `send_message`:

```rust
async fn handle_send_message(id: Option<Value>, app: &App, params: &Value) -> RpcResponse {
    let peer = match params.get("peer").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return RpcResponse::error(id, -32602, "Missing 'peer' parameter".into()),
    };
    let message = match params.get("message").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => return RpcResponse::error(id, -32602, "Missing 'message' parameter".into()),
    };

    // Resolve peer (onion or friend code)
    let peer_onion = if peer.ends_with(".onion") {
        peer.to_string()
    } else {
        match crate::protocol::friend_code::friend_code_to_onion(peer) {
            Ok(onion) => onion,
            Err(_) => return RpcResponse::error(id, -32602, "Invalid peer address or friend code".into()),
        }
    };

    // Find friend and conversation
    let friend_id = match crate::db::queries::find_friend_by_onion(&app.db, &peer_onion) {
        Some(id) => id,
        None => return RpcResponse::error(id, -32000, "Peer is not a friend".into()),
    };
    let conv_id = crate::db::queries::get_or_create_conversation(&app.db, friend_id)
        .unwrap_or(0);

    let own_onion = app.onion_address.as_deref().unwrap_or("");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let message_id = format!("msg-{}", now);

    // Try to encrypt and send
    let ttl = crate::db::queries::get_conversation_ephemeral_ttl(&app.db, conv_id).unwrap_or(None);

    // Store locally
    crate::db::queries::store_outgoing_message_with_ttl(
        &app.db, conv_id, own_onion, message, "sent", ttl,
    ).ok();

    // Encrypt with Signal Protocol
    match crate::crypto::signal::RatchetState::load(&app.db, &peer_onion) {
        Some(mut session) => {
            let (header, ciphertext, is_prekey) = session.encrypt(message.as_bytes());
            session.save(&app.db, &peer_onion);

            let msg = crate::protocol::message::Message::TextMessage(
                crate::protocol::message::TextMessageContent {
                    from_onion: own_onion.to_string(),
                    to_onion: peer_onion.clone(),
                    signal_header: crate::base64_encode(&header),
                    signal_ciphertext: crate::base64_encode(&ciphertext),
                    signal_type: if is_prekey { "PreKeyMessage".into() } else { "Message".into() },
                    timestamp: now,
                    message_id: message_id.clone(),
                    x3dh_init: None,
                }
            );

            // Try direct send, fall back to queue
            if let Some(ref pool) = app.connection_pool {
                if pool.send(&peer_onion, &msg).await.is_ok() {
                    return RpcResponse::success(id, serde_json::json!({
                        "status": "sent",
                        "message_id": message_id,
                    }));
                }
            }

            // Queue for later delivery
            app.message_queue.enqueue(&app.db, &peer_onion, &msg, "normal").ok();
            RpcResponse::success(id, serde_json::json!({
                "status": "queued",
                "message_id": message_id,
            }))
        }
        None => {
            RpcResponse::error(id, -32000, format!("No encryption session with {}", peer_onion))
        }
    }
}
```

Implement remaining handlers following the same pattern. Each handler:
1. Validates params
2. Calls existing `db::queries::*` or `handlers::*` functions
3. Returns JSON result or error

**Step 2: Create `src/daemon/socket.rs`**

```rust
use crate::app::App;
use crate::presence::PresenceMap;
use super::rpc;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::UnixListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Start the Unix socket server. Returns a JoinHandle.
pub async fn start(
    socket_path: &Path,
    app: Arc<Mutex<App>>,
    presence: PresenceMap,
) -> tokio::task::JoinHandle<()> {
    // Remove stale socket file
    if socket_path.exists() {
        std::fs::remove_file(socket_path).ok();
    }

    let listener = UnixListener::bind(socket_path)
        .expect("Failed to bind Unix socket");

    // Set permissions to owner-only (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600)).ok();
    }

    let socket_path = socket_path.to_path_buf();
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
                writer.write_all(format!("{}\n", json).as_bytes()).await.ok();
                continue;
            }
        };

        let app_lock = app.lock().await;
        let response = rpc::dispatch(&request, &app_lock, &presence).await;
        drop(app_lock);

        let json = serde_json::to_string(&response).unwrap_or_default();
        writer.write_all(format!("{}\n", json).as_bytes()).await.ok();
    }
}
```

**Step 3: Wire socket into daemon startup**

In `src/daemon/mod.rs`, add socket start before event loop:

```rust
// Start Unix socket server
let socket_path = settings.data_dir.join("chattor.sock");
let _socket_handle = daemon::socket::start(&socket_path, Arc::clone(&app), presence_map.clone()).await;
eprintln!("Listening on {}", socket_path.display());

// Run event loop
let result = event_loop::run(Arc::clone(&app)).await;

// Cleanup socket
std::fs::remove_file(&socket_path).ok();
```

**Step 4: Write tests for RPC dispatch**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_response_success() {
        let resp = RpcResponse::success(Some(Value::Number(1.into())), serde_json::json!({"ok": true}));
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
    }
}
```

**Step 5: Run tests and commit**

```bash
cargo test
cargo clippy -- -D warnings
git add src/daemon/rpc.rs src/daemon/socket.rs src/daemon/mod.rs
git commit -m "feat: add JSON-RPC Unix socket server to daemon"
```

---

## Task 4: Restructure CLI with Clap Subcommands

Add clap subcommands so `chattor` routes to TUI, daemon, or CLI client mode.

**Files:**
- Modify: `src/cli.rs` (add subcommands)
- Create: `src/client.rs` (CLI client that talks to daemon socket)
- Modify: `src/main.rs` (route based on subcommand)

**Step 1: Restructure `src/cli.rs`**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "chattor", about = "Private P2P chat over Tor")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,

    /// Override config directory
    #[arg(long)]
    pub config_dir: Option<String>,

    /// Override data directory
    #[arg(long)]
    pub data_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run interactive TUI (default if no subcommand)
    Tui {
        /// Theme preset name
        #[arg(long)]
        theme: Option<String>,
    },

    /// Run headless daemon
    Daemon,

    /// Show daemon status
    Status,

    /// Show own identity (friend code + .onion address)
    Identity,

    /// Friend management
    Friends {
        #[command(subcommand)]
        action: FriendsAction,
    },

    /// Send a message
    Send {
        /// Peer .onion address or friend code
        peer: String,
        /// Message text
        message: String,
    },

    /// Receive unread messages
    Recv {
        /// Filter by peer
        #[arg(long)]
        peer: Option<String>,
    },

    /// Stream incoming messages (blocking)
    Listen,

    /// Channel management
    Channels {
        #[command(subcommand)]
        action: ChannelsAction,
    },

    /// Set ephemeral message TTL
    Ephemeral {
        /// Peer .onion or friend code
        peer: String,
        /// TTL in seconds (0 to disable)
        ttl: i64,
    },

    /// Toggle notifications
    Notifications {
        /// on or off
        state: String,
    },

    /// Start MCP server (stdio transport)
    Mcp,
}

#[derive(Subcommand, Debug)]
pub enum FriendsAction {
    /// List friends
    List,
    /// Add a friend
    Add { code: String },
    /// Remove a friend
    Remove { onion: String },
    /// List pending requests
    Requests,
    /// Accept a friend request
    Accept { id: i64 },
    /// Reject a friend request
    Reject { id: i64 },
}

#[derive(Subcommand, Debug)]
pub enum ChannelsAction {
    /// List channels
    List,
    /// Publish to a channel
    Publish {
        /// "public" or "friends"
        channel_type: String,
        /// Post content
        message: String,
    },
    /// Subscribe to a channel
    Subscribe { onion: String },
    /// Read channel feed
    Feed {
        #[arg(long)]
        channel: Option<i64>,
    },
}
```

**Step 2: Create `src/client.rs`**

A simple client that connects to the daemon socket and sends JSON-RPC:

```rust
use crate::error::{Result, ChattorError};
use serde_json::Value;
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Send an RPC request to the daemon and return the result.
pub async fn rpc_call(data_dir: &PathBuf, method: &str, params: Value) -> Result<Value> {
    let socket_path = data_dir.join("chattor.sock");

    let stream = UnixStream::connect(&socket_path).await
        .map_err(|_| ChattorError::Network(
            "Cannot connect to daemon. Is it running? Start with: chattor daemon".into()
        ))?;

    let (reader, mut writer) = stream.into_split();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let line = format!("{}\n", serde_json::to_string(&request).unwrap());
    writer.write_all(line.as_bytes()).await
        .map_err(|e| ChattorError::Network(format!("Socket write error: {}", e)))?;

    let mut lines = BufReader::new(reader).lines();
    let response_line = lines.next_line().await
        .map_err(|e| ChattorError::Network(format!("Socket read error: {}", e)))?
        .ok_or_else(|| ChattorError::Network("Daemon closed connection".into()))?;

    let response: Value = serde_json::from_str(&response_line)
        .map_err(|e| ChattorError::Network(format!("Invalid response: {}", e)))?;

    if let Some(error) = response.get("error") {
        let msg = error.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
        return Err(ChattorError::Network(msg.to_string()));
    }

    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}
```

**Step 3: Update `src/main.rs` to route subcommands**

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut settings = config::Settings::default()?;
    if let Some(ref dir) = cli.config_dir {
        settings.config_dir = std::path::PathBuf::from(dir);
    }
    if let Some(ref dir) = cli.data_dir {
        settings.data_dir = std::path::PathBuf::from(dir);
        settings.db_path = settings.data_dir.join("messages.db");
    }

    match cli.command {
        None | Some(Command::Tui { .. }) => {
            // Existing TUI code (extract into a function)
            run_tui(cli, settings).await
        }
        Some(Command::Daemon) => {
            daemon::run(settings).await
        }
        Some(Command::Status) => {
            let result = client::rpc_call(&settings.data_dir, "status", serde_json::json!({})).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Identity) => {
            let result = client::rpc_call(&settings.data_dir, "identity", serde_json::json!({})).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Send { peer, message }) => {
            let result = client::rpc_call(&settings.data_dir, "send_message", serde_json::json!({
                "peer": peer,
                "message": message,
            })).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Recv { peer }) => {
            let mut params = serde_json::json!({});
            if let Some(p) = peer {
                params["peer"] = Value::String(p);
            }
            let result = client::rpc_call(&settings.data_dir, "recv_messages", params).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Listen) => {
            // Connect to socket and stream messages
            let socket_path = settings.data_dir.join("chattor.sock");
            let stream = tokio::net::UnixStream::connect(&socket_path).await
                .map_err(|_| error::ChattorError::Network("Cannot connect to daemon".into()))?;
            let (reader, mut writer) = stream.into_split();

            let request = serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "listen", "params": {}
            });
            writer.write_all(format!("{}\n", serde_json::to_string(&request).unwrap()).as_bytes()).await?;

            let mut lines = tokio::io::BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{}", line);
            }
            Ok(())
        }
        Some(Command::Friends { action }) => {
            let (method, params) = match action {
                FriendsAction::List => ("friends_list", serde_json::json!({})),
                FriendsAction::Add { code } => ("friends_add", serde_json::json!({"code": code})),
                FriendsAction::Remove { onion } => ("friends_remove", serde_json::json!({"onion": onion})),
                FriendsAction::Requests => ("friends_requests", serde_json::json!({})),
                FriendsAction::Accept { id } => ("friends_accept", serde_json::json!({"id": id})),
                FriendsAction::Reject { id } => ("friends_reject", serde_json::json!({"id": id})),
            };
            let result = client::rpc_call(&settings.data_dir, method, params).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Channels { action }) => {
            let (method, params) = match action {
                ChannelsAction::List => ("channels_list", serde_json::json!({})),
                ChannelsAction::Publish { channel_type, message } => ("channels_publish", serde_json::json!({"channel_type": channel_type, "message": message})),
                ChannelsAction::Subscribe { onion } => ("channels_subscribe", serde_json::json!({"onion": onion})),
                ChannelsAction::Feed { channel } => ("channels_feed", serde_json::json!({"channel_id": channel})),
            };
            let result = client::rpc_call(&settings.data_dir, method, params).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Ephemeral { peer, ttl }) => {
            let ttl_value = if ttl == 0 { Value::Null } else { Value::Number(ttl.into()) };
            let result = client::rpc_call(&settings.data_dir, "ephemeral_set", serde_json::json!({
                "peer": peer, "ttl": ttl_value,
            })).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Notifications { state }) => {
            let enabled = state == "on";
            let result = client::rpc_call(&settings.data_dir, "notifications_toggle", serde_json::json!({
                "enabled": enabled,
            })).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            Ok(())
        }
        Some(Command::Mcp) => {
            // MCP server — handled in Task 5
            todo!("MCP server")
        }
    }
}
```

**Step 4: Run tests and commit**

```bash
cargo test
cargo clippy -- -D warnings
git add src/cli.rs src/client.rs src/main.rs
git commit -m "feat: add CLI subcommands with daemon client"
```

---

## Task 5: Implement the `listen` RPC Method (Streaming)

The `listen` method keeps the socket connection open and sends JSON events as messages arrive.

**Files:**
- Modify: `src/daemon/socket.rs` (handle streaming connections)
- Modify: `src/daemon/event_loop.rs` (broadcast incoming messages to listeners)

**Step 1: Add a broadcast channel for incoming messages**

In the daemon event loop, when a message is received, broadcast it to all connected `listen` clients:

```rust
use tokio::sync::broadcast;

// In daemon::run():
let (msg_broadcast_tx, _) = broadcast::channel::<String>(100);
```

Pass `msg_broadcast_tx` to both the event loop (to send events) and the socket server (for listen clients to subscribe).

**Step 2: In event loop, broadcast formatted messages**

When `handle_incoming_message` succeeds for a TextMessage, broadcast:

```rust
let event = serde_json::json!({
    "jsonrpc": "2.0",
    "method": "message_received",
    "params": {
        "from": incoming.remote_addr,
        "timestamp": now,
    }
});
let _ = msg_broadcast_tx.send(serde_json::to_string(&event).unwrap());
```

**Step 3: In socket handler, subscribe to broadcast for `listen`**

```rust
if request.method == "listen" {
    let mut rx = msg_broadcast_tx.subscribe();
    // Send initial ACK
    let ack = RpcResponse::success(request.id.clone(), serde_json::json!({"status": "listening"}));
    writer.write_all(format!("{}\n", serde_json::to_string(&ack).unwrap()).as_bytes()).await.ok();

    // Stream events until client disconnects
    while let Ok(event) = rx.recv().await {
        if writer.write_all(format!("{}\n", event).as_bytes()).await.is_err() {
            break; // Client disconnected
        }
    }
    return; // Connection ends after listen
}
```

**Step 4: Run tests and commit**

```bash
cargo test
cargo clippy -- -D warnings
git add src/daemon/
git commit -m "feat: add streaming listen method for real-time message events"
```

---

## Task 6: Create MCP Server

Add an MCP server that agent frameworks can launch as a subprocess. Communicates via stdio, translates MCP tool calls to daemon socket requests.

**Files:**
- Create: `src/mcp/mod.rs`
- Create: `src/mcp/server.rs`
- Create: `src/mcp/tools.rs`
- Modify: `src/main.rs` (wire `Command::Mcp`)

**Step 1: Create `src/mcp/mod.rs`**

```rust
pub mod server;
pub mod tools;
```

Add `mod mcp;` to `src/main.rs`.

**Step 2: Create `src/mcp/tools.rs`**

Define MCP tool schemas:

```rust
use serde_json::{json, Value};

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "send_message",
            "description": "Send a private encrypted message to a peer over Tor",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "peer": { "type": "string", "description": "Peer .onion address or friend code" },
                    "message": { "type": "string", "description": "Message text to send" }
                },
                "required": ["peer", "message"]
            }
        }),
        json!({
            "name": "receive_messages",
            "description": "Get unread messages, optionally filtered by peer",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "peer": { "type": "string", "description": "Optional: filter by peer onion/code" },
                    "since": { "type": "integer", "description": "Optional: Unix timestamp, return messages after this time" }
                }
            }
        }),
        json!({
            "name": "list_friends",
            "description": "List all friends with their online/typing status and unread counts",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "add_friend",
            "description": "Send a friend request to a peer via their .onion address or friend code",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": ".onion address or friend code" }
                },
                "required": ["code"]
            }
        }),
        json!({
            "name": "accept_friend_request",
            "description": "Accept a pending friend request by ID",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Friend request ID" }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "get_identity",
            "description": "Get own friend code and .onion address for sharing with peers",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_status",
            "description": "Check daemon status, Tor connection, and .onion address",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "publish_channel_post",
            "description": "Publish a post to own broadcast channel",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel_type": { "type": "string", "enum": ["public", "friends"], "description": "Channel type" },
                    "message": { "type": "string", "description": "Post content" }
                },
                "required": ["channel_type", "message"]
            }
        }),
        json!({
            "name": "list_channel_posts",
            "description": "Read posts from a broadcast channel",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel_id": { "type": "integer", "description": "Optional: specific channel ID" }
                }
            }
        }),
    ]
}

/// Map MCP tool name to daemon RPC method + params.
pub fn tool_to_rpc(tool_name: &str, arguments: &Value) -> Option<(&str, Value)> {
    match tool_name {
        "send_message" => Some(("send_message", arguments.clone())),
        "receive_messages" => Some(("recv_messages", arguments.clone())),
        "list_friends" => Some(("friends_list", json!({}))),
        "add_friend" => Some(("friends_add", arguments.clone())),
        "accept_friend_request" => Some(("friends_accept", arguments.clone())),
        "get_identity" => Some(("identity", json!({}))),
        "get_status" => Some(("status", json!({}))),
        "publish_channel_post" => Some(("channels_publish", arguments.clone())),
        "list_channel_posts" => Some(("channels_feed", arguments.clone())),
        _ => None,
    }
}
```

**Step 3: Create `src/mcp/server.rs`**

MCP server over stdio:

```rust
use crate::client;
use super::tools;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run(data_dir: PathBuf) -> crate::error::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "chattor",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                })
            }
            "notifications/initialized" => continue, // No response needed
            "tools/list" => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": tools::tool_definitions()
                    }
                })
            }
            "tools/call" => {
                let tool_name = request["params"]["name"].as_str().unwrap_or("");
                let arguments = request["params"].get("arguments").cloned().unwrap_or(json!({}));

                match tools::tool_to_rpc(tool_name, &arguments) {
                    Some((rpc_method, rpc_params)) => {
                        match client::rpc_call(&data_dir, rpc_method, rpc_params).await {
                            Ok(result) => {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [{
                                            "type": "text",
                                            "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                                        }]
                                    }
                                })
                            }
                            Err(e) => {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [{
                                            "type": "text",
                                            "text": format!("Error: {}", e)
                                        }],
                                        "isError": true
                                    }
                                })
                            }
                        }
                    }
                    None => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": format!("Unknown tool: {}", tool_name)
                            }
                        })
                    }
                }
            }
            _ => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("Method not found: {}", method) }
                })
            }
        };

        let output = serde_json::to_string(&response).unwrap();
        stdout.write_all(format!("{}\n", output).as_bytes()).await.ok();
        stdout.flush().await.ok();
    }

    Ok(())
}
```

**Step 4: Wire into main.rs**

Replace the `todo!()` in the `Command::Mcp` match arm:

```rust
Some(Command::Mcp) => {
    mcp::server::run(settings.data_dir).await
}
```

**Step 5: Run tests and commit**

```bash
cargo test
cargo clippy -- -D warnings
git add src/mcp/ src/main.rs
git commit -m "feat: add MCP server over stdio for agent integration"
```

---

## Task 7: Integration Tests

Test the full daemon → socket → CLI flow.

**Files:**
- Create: `tests/daemon_test.rs`

**Step 1: Write daemon lifecycle test**

```rust
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn test_pid_file_lifecycle() {
    let temp = TempDir::new().unwrap();
    let pid_path = temp.path().join("chattor.pid");

    // Acquire PID file
    chattor::daemon::pid::acquire(&pid_path).unwrap();
    assert!(pid_path.exists());

    // Read PID
    let contents = std::fs::read_to_string(&pid_path).unwrap();
    assert_eq!(contents, format!("{}", std::process::id()));

    // Release
    chattor::daemon::pid::release(&pid_path);
    assert!(!pid_path.exists());
}

#[tokio::test]
async fn test_pid_file_prevents_double_start() {
    let temp = TempDir::new().unwrap();
    let pid_path = temp.path().join("chattor.pid");

    // Write a PID file with our own PID (simulates running process)
    std::fs::write(&pid_path, format!("{}", std::process::id())).unwrap();

    // Acquiring should fail
    let result = chattor::daemon::pid::acquire(&pid_path);
    assert!(result.is_err());

    // Cleanup
    std::fs::remove_file(&pid_path).ok();
}
```

**Step 2: Write RPC round-trip test**

```rust
#[tokio::test]
async fn test_rpc_request_response_parsing() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"status","params":{}}"#;
    let req: chattor::daemon::rpc::RpcRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.method, "status");
    assert_eq!(req.jsonrpc, "2.0");
}
```

**Step 3: Write CLI argument parsing test**

```rust
#[test]
fn test_cli_send_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "send", "abc.onion", "hello"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Send { peer, message }) => {
            assert_eq!(peer, "abc.onion");
            assert_eq!(message, "hello");
        }
        _ => panic!("Expected Send command"),
    }
}

#[test]
fn test_cli_daemon_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "daemon"]).unwrap();
    assert!(matches!(args.command, Some(chattor::cli::Command::Daemon)));
}

#[test]
fn test_cli_no_subcommand_defaults_to_tui() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor"]).unwrap();
    assert!(args.command.is_none()); // None = TUI
}

#[test]
fn test_cli_friends_list_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "friends", "list"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Friends { action: chattor::cli::FriendsAction::List }) => {},
        _ => panic!("Expected Friends List"),
    }
}
```

**Step 4: Run tests and commit**

```bash
cargo test
cargo clippy -- -D warnings
git add tests/daemon_test.rs
git commit -m "test: add daemon, RPC, and CLI integration tests"
```

---

## Task 8: Documentation and CLAUDE.md Update

Update project documentation for the new daemon/CLI/MCP functionality.

**Files:**
- Modify: `CLAUDE.md` (add daemon/CLI/MCP sections)

**Step 1: Add to CLAUDE.md**

Add a new section after "Development Commands":

```markdown
### Daemon & CLI
```bash
chattor daemon                   # Run headless daemon
chattor status                   # Check daemon status
chattor identity                 # Show friend code + .onion
chattor friends list             # List friends (JSON)
chattor friends add <code>       # Send friend request
chattor send <peer> <message>    # Send encrypted message
chattor recv                     # Poll unread messages
chattor listen                   # Stream incoming messages
chattor mcp                      # Start MCP server (stdio)
```
```

Add to Architecture Overview:

```markdown
### Daemon Mode
- `src/daemon/mod.rs` — Daemon entry point and lifecycle
- `src/daemon/event_loop.rs` — tokio::select! event loop (no TUI)
- `src/daemon/tasks.rs` — Background task spawning (heartbeat, sync)
- `src/daemon/rpc.rs` — JSON-RPC 2.0 request dispatch
- `src/daemon/socket.rs` — Unix domain socket server
- `src/daemon/pid.rs` — PID file management for mutual exclusion

### CLI Client
- `src/cli.rs` — Clap subcommand definitions
- `src/client.rs` — Unix socket client for daemon communication

### MCP Server
- `src/mcp/mod.rs` — MCP server module
- `src/mcp/server.rs` — stdio JSON-RPC server for agent frameworks
- `src/mcp/tools.rs` — MCP tool definitions and RPC mapping
```

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add daemon, CLI, and MCP server documentation"
```

---

## Summary

| Task | Description | Est. Lines | Key Files |
|------|-------------|-----------|-----------|
| 1 | Extract message handlers | ~50 new, ~900 moved | `src/handlers/` |
| 2 | Daemon module + event loop | ~300 | `src/daemon/` |
| 3 | Unix socket JSON-RPC server | ~400 | `src/daemon/rpc.rs`, `socket.rs` |
| 4 | CLI subcommands + client | ~300 | `src/cli.rs`, `src/client.rs`, `src/main.rs` |
| 5 | Streaming listen method | ~60 | `src/daemon/socket.rs`, `event_loop.rs` |
| 6 | MCP server | ~250 | `src/mcp/` |
| 7 | Integration tests | ~100 | `tests/daemon_test.rs` |
| 8 | Documentation | ~50 | `CLAUDE.md` |
| **Total** | | **~1500 new + ~900 moved** | |

Each task is independently testable and committable. Run `cargo test` and `cargo clippy -- -D warnings` after every task.
