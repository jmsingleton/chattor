# Network Layer MVP Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Complete Phase 1 MVP - send/receive friend requests over Tor with simple queue resilience and display user's own friend code.

**Architecture:** Hybrid send (direct with 5sec timeout, queue on failure), listener task via channel to main thread, simple 30sec retry logic, identity modal UI.

**Tech Stack:** Rust, tokio, arti (Tor), rusqlite, ratatui

---

## Task 1: Add Message Queue Database Schema

**Files:**
- Modify: `src/db/schema.rs`
- Test: `src/db/connection.rs` (verify migration)

**Step 1: Add message_queue table to schema**

Edit `src/db/schema.rs`, add to `CREATE_TABLES`:

```rust
CREATE TABLE IF NOT EXISTS message_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    peer_onion TEXT NOT NULL,
    message_json TEXT NOT NULL,
    priority TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    next_retry_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_queue_next_retry
    ON message_queue(next_retry_at, status)
    WHERE status = 'pending';
```

Also increment `SCHEMA_VERSION` to 4.

**Step 2: Run tests to verify schema is valid**

Run: `cargo test db::connection::tests::test_database_creation`
Expected: PASS (schema creates without errors)

**Step 3: Commit**

```bash
git add src/db/schema.rs
git commit -m "feat(db): add message_queue table schema v4"
```

---

## Task 2: Implement MessageQueue Methods

**Files:**
- Modify: `src/net/queue.rs` (currently stub)
- Test: Add unit tests in same file

**Step 1: Write failing test for enqueue**

Add to `src/net/queue.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::protocol::message::{Message, Payload, FriendRequestMessage};
    use tempfile::NamedTempFile;

    #[test]
    fn test_enqueue_message() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = Database::open(temp_db.path()).unwrap();
        let queue = MessageQueue::new();

        let msg = Message {
            id: uuid::Uuid::new_v4(),
            timestamp: 123456,
            payload: Payload::FriendRequest(FriendRequestMessage {
                from_onion: "abc.onion".to_string(),
                friend_code: "test-code".to_string(),
                timestamp: 123456,
                signature: "sig".to_string(),
            }),
        };

        let result = queue.enqueue(&db, "peer.onion", &msg, "high");
        assert!(result.is_ok());

        // Verify it's in database
        let count: i64 = db.connection()
            .query_row("SELECT COUNT(*) FROM message_queue", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test net::queue::tests::test_enqueue_message`
Expected: FAIL (enqueue method not implemented)

**Step 3: Implement enqueue method**

In `src/net/queue.rs`, replace stub implementation:

```rust
use crate::db::Database;
use crate::protocol::message::Message;
use crate::error::{Result, TorrentChatError};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct MessageQueue;

impl MessageQueue {
    pub fn new() -> Self {
        MessageQueue
    }

    pub fn enqueue(
        &self,
        db: &Database,
        peer_onion: &str,
        message: &Message,
        priority: &str,
    ) -> Result<i64> {
        let message_json = serde_json::to_string(message)
            .map_err(|e| TorrentChatError::Network(format!("Failed to serialize: {}", e)))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let id = db.connection().query_row(
            "INSERT INTO message_queue (peer_onion, message_json, priority, next_retry_at, created_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending')
             RETURNING id",
            (peer_onion, message_json, priority, now, now),
            |row| row.get(0),
        ).map_err(|e| TorrentChatError::Database(format!("Failed to enqueue: {}", e)))?;

        Ok(id)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test net::queue::tests::test_enqueue_message`
Expected: PASS

**Step 5: Add tests for get_pending_messages**

```rust
#[test]
fn test_get_pending_messages() {
    let temp_db = NamedTempFile::new().unwrap();
    let db = Database::open(temp_db.path()).unwrap();
    let queue = MessageQueue::new();

    // Enqueue a message
    let msg = create_test_message();
    queue.enqueue(&db, "peer.onion", &msg, "high").unwrap();

    // Get pending messages (now = future, should return it)
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64 + 100;
    let pending = queue.get_pending_messages(&db, now).unwrap();

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].peer_onion, "peer.onion");
}

fn create_test_message() -> Message {
    Message {
        id: uuid::Uuid::new_v4(),
        timestamp: 123456,
        payload: Payload::FriendRequest(FriendRequestMessage {
            from_onion: "test.onion".to_string(),
            friend_code: "code".to_string(),
            timestamp: 123456,
            signature: "sig".to_string(),
        }),
    }
}
```

**Step 6: Implement get_pending_messages**

```rust
pub struct QueuedMessage {
    pub id: i64,
    pub peer_onion: String,
    pub message: Message,
    pub retry_count: i64,
    pub priority: String,
}

impl MessageQueue {
    pub fn get_pending_messages(&self, db: &Database, now: i64) -> Result<Vec<QueuedMessage>> {
        let conn = db.connection();
        let mut stmt = conn.prepare(
            "SELECT id, peer_onion, message_json, retry_count, priority
             FROM message_queue
             WHERE status = 'pending' AND next_retry_at <= ?1
             ORDER BY priority DESC, created_at ASC"
        ).map_err(|e| TorrentChatError::Database(format!("Query failed: {}", e)))?;

        let rows = stmt.query_map([now], |row| {
            let id: i64 = row.get(0)?;
            let peer_onion: String = row.get(1)?;
            let message_json: String = row.get(2)?;
            let retry_count: i64 = row.get(3)?;
            let priority: String = row.get(4)?;

            let message: Message = serde_json::from_str(&message_json)
                .map_err(|e| rusqlite::Error::InvalidQuery)?;

            Ok(QueuedMessage {
                id,
                peer_onion,
                message,
                retry_count,
                priority,
            })
        }).map_err(|e| TorrentChatError::Database(format!("Query failed: {}", e)))?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(|e| TorrentChatError::Database(format!("Row failed: {}", e)))?);
        }

        Ok(messages)
    }
}
```

**Step 7: Implement mark_delivered and schedule_retry**

```rust
impl MessageQueue {
    pub fn mark_delivered(&self, db: &Database, id: i64) -> Result<()> {
        db.connection().execute(
            "UPDATE message_queue SET status = 'delivered' WHERE id = ?1",
            [id],
        ).map_err(|e| TorrentChatError::Database(format!("Failed to mark delivered: {}", e)))?;
        Ok(())
    }

    pub fn mark_failed(&self, db: &Database, id: i64) -> Result<()> {
        db.connection().execute(
            "UPDATE message_queue SET status = 'failed' WHERE id = ?1",
            [id],
        ).map_err(|e| TorrentChatError::Database(format!("Failed to mark failed: {}", e)))?;
        Ok(())
    }

    pub fn schedule_retry(&self, db: &Database, id: i64, next_retry_at: i64) -> Result<()> {
        db.connection().execute(
            "UPDATE message_queue SET retry_count = retry_count + 1, next_retry_at = ?2 WHERE id = ?1",
            (id, next_retry_at),
        ).map_err(|e| TorrentChatError::Database(format!("Failed to schedule retry: {}", e)))?;
        Ok(())
    }
}
```

**Step 8: Add tests for all methods**

```rust
#[test]
fn test_mark_delivered() {
    let temp_db = NamedTempFile::new().unwrap();
    let db = Database::open(temp_db.path()).unwrap();
    let queue = MessageQueue::new();

    let msg = create_test_message();
    let id = queue.enqueue(&db, "peer.onion", &msg, "high").unwrap();
    queue.mark_delivered(&db, id).unwrap();

    let status: String = db.connection()
        .query_row("SELECT status FROM message_queue WHERE id = ?1", [id], |row| row.get(0))
        .unwrap();
    assert_eq!(status, "delivered");
}

#[test]
fn test_schedule_retry() {
    let temp_db = NamedTempFile::new().unwrap();
    let db = Database::open(temp_db.path()).unwrap();
    let queue = MessageQueue::new();

    let msg = create_test_message();
    let id = queue.enqueue(&db, "peer.onion", &msg, "high").unwrap();

    let future_time = 999999;
    queue.schedule_retry(&db, id, future_time).unwrap();

    let (retry_count, next_retry): (i64, i64) = db.connection()
        .query_row(
            "SELECT retry_count, next_retry_at FROM message_queue WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?))
        )
        .unwrap();

    assert_eq!(retry_count, 1);
    assert_eq!(next_retry, future_time);
}
```

**Step 9: Run all queue tests**

Run: `cargo test net::queue::tests`
Expected: All tests PASS

**Step 10: Commit**

```bash
git add src/net/queue.rs
git commit -m "feat(net): implement MessageQueue with database persistence"
```

---

## Task 3: Add try_send_direct Helper Function

**Files:**
- Modify: `src/main.rs`
- Test: Manual testing (unit tests would require mocking Tor)

**Step 1: Add try_send_direct helper**

Add to `src/main.rs` after the handler functions:

```rust
use tokio::time::{timeout, Duration};
use crate::tor::connection::TorConnection;

/// Try to send message directly with timeout
async fn try_send_direct(
    app: &App,
    peer_onion: &str,
    message: &protocol::message::Message,
) -> Result<()> {
    // Check Tor is ready
    let tor_client = app.tor_client.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Tor not initialized".into()))?;

    // Connect with 5-second timeout
    let mut conn = timeout(
        Duration::from_secs(5),
        TorConnection::connect(tor_client, peer_onion)
    )
    .await
    .map_err(|_| error::TorrentChatError::Network("Connection timeout".into()))??;

    // Send message
    conn.send(message).await?;

    Ok(())
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(net): add try_send_direct helper with timeout"
```

---

## Task 4: Update handle_send_friend_request to Use Hybrid Send

**Files:**
- Modify: `src/main.rs:142-174` (handle_send_friend_request)

**Step 1: Update handler to use hybrid approach**

Replace existing `handle_send_friend_request` function:

```rust
/// Handle sending a friend request
async fn handle_send_friend_request(app: &App, friend_code: &str) -> Result<SendResult> {
    use crate::protocol::friend_code::{validate_friend_code, friend_code_to_onion};
    use crate::protocol::friend_request::FriendRequestHandler;

    // Validate friend code format
    validate_friend_code(friend_code)?;

    // Get our .onion address
    let own_onion = app.onion_address.as_ref()
        .ok_or_else(|| error::TorrentChatError::Tor("Tor not initialized yet".into()))?;

    // Create friend request message
    let request_msg = FriendRequestHandler::create_request(
        &app.identity,
        own_onion,
        friend_code,
    )?;

    // Convert to Message
    let message = protocol::message::Message {
        id: uuid::Uuid::new_v4(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        payload: protocol::message::Payload::FriendRequest(request_msg),
    };

    // Convert friend code to .onion address
    let peer_onion = friend_code_to_onion(friend_code)?;

    // Try direct send first
    match try_send_direct(app, &peer_onion, &message).await {
        Ok(_) => Ok(SendResult::SentImmediately),
        Err(_) => {
            // Queue for background delivery
            app.message_queue.enqueue(&app.db, &peer_onion, &message, "high")?;
            Ok(SendResult::Queued)
        }
    }
}

pub enum SendResult {
    SentImmediately,
    Queued,
}
```

**Step 2: Update UI handler to use SendResult**

In main event loop, update the SendFriendRequest case:

```rust
Some(AppAction::SendFriendRequest(code)) => {
    let app_lock = app.lock().await;

    match handle_send_friend_request(&*app_lock, &code).await {
        Ok(SendResult::SentImmediately) => {
            app_state = AppState::Normal;
        }
        Ok(SendResult::Queued) => {
            app_state = AppState::AddingFriend {
                input: code,
                cursor: 0,
                error: Some("Queued for delivery 🕐".into()),
            };
        }
        Err(e) => {
            app_state = AppState::AddingFriend {
                input: code,
                cursor: 0,
                error: Some(format!("Failed: {}", e)),
            };
        }
    }
    drop(app_lock);
}
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(net): implement hybrid send for friend requests"
```

---

## Task 5: Implement Listener Task for Incoming Messages

**Files:**
- Modify: `src/app.rs` (add incoming_message_rx field)
- Modify: `src/net/listener.rs` (update for DataStream)
- Modify: `src/main.rs` (poll channel)

**Step 1: Add channel to App struct**

Edit `src/app.rs`:

```rust
use tokio::sync::mpsc;

pub struct App {
    pub settings: Settings,
    pub db: Database,
    pub identity: IdentityKeypair,
    pub tor_client: Option<Arc<TorClient>>,
    pub hidden_service: Option<HiddenService>,
    pub message_queue: MessageQueue,
    pub onion_address: Option<String>,
    pub incoming_message_rx: Option<mpsc::Receiver<IncomingMessage>>, // NEW
}

impl App {
    pub fn new() -> Result<Self> {
        // ... existing code ...
        Ok(App {
            settings,
            db,
            identity,
            tor_client,
            hidden_service,
            message_queue,
            onion_address,
            incoming_message_rx: None, // NEW
        })
    }
}
```

**Step 2: Create listener task in init_tor**

Edit `src/app.rs`, in `init_tor()` method, after hidden service creation:

```rust
// Spawn listener task
let (msg_tx, msg_rx) = mpsc::channel(100);
let hidden_service_clone = hidden_service.clone();
tokio::spawn(async move {
    if let Err(e) = crate::net::listener::listen_for_tor_connections(hidden_service_clone, msg_tx).await {
        eprintln!("Listener task error: {}", e);
    }
});

self.incoming_message_rx = Some(msg_rx);
```

**Step 3: Update listener.rs for Tor DataStream**

Edit `src/net/listener.rs`, add new function:

```rust
use crate::tor::hidden_service::HiddenService;

/// Listen for incoming Tor connections
pub async fn listen_for_tor_connections(
    hidden_service: HiddenService,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    loop {
        match hidden_service.accept().await {
            Ok(stream) => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_tor_connection(stream, tx).await {
                        eprintln!("Connection handler error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Handle single incoming Tor connection
async fn handle_tor_connection(
    mut stream: arti_client::DataStream,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    use crate::net::framing::receive_message;

    // Receive message using existing framing
    let message = receive_message(&mut stream).await?;

    // Send to main thread
    tx.send(IncomingMessage {
        message,
        remote_addr: "tor-peer".to_string(), // Can't get real .onion from DataStream
    }).await
    .map_err(|e| crate::error::TorrentChatError::Network(format!("Failed to send to app: {}", e)))?;

    Ok(())
}
```

**Step 4: Poll channel in main loop**

Edit `src/main.rs`, in main loop after event polling:

```rust
// Check for incoming messages
if let Some(rx) = &mut app.lock().await.incoming_message_rx {
    while let Ok(incoming) = rx.try_recv() {
        let app_lock = app.lock().await;
        if let Err(e) = handle_incoming_message(&*app_lock, incoming) {
            eprintln!("Failed to handle incoming message: {}", e);
        }
        drop(app_lock);
    }
}
```

**Step 5: Add handle_incoming_message function**

Add to `src/main.rs`:

```rust
use crate::net::listener::IncomingMessage;

fn handle_incoming_message(app: &App, incoming: IncomingMessage) -> Result<()> {
    use crate::protocol::message::Payload;

    match incoming.message.payload {
        Payload::FriendRequest(req) => {
            // Insert into database
            let conn = app.db.connection();
            conn.execute(
                "INSERT INTO friend_requests (from_onion, friend_code, timestamp, status)
                 VALUES (?1, ?2, ?3, 'pending')",
                (&req.from_onion, &req.friend_code, req.timestamp),
            ).map_err(|e| error::TorrentChatError::Database(format!("Failed to save request: {}", e)))?;

            eprintln!("Received friend request from {}", req.from_onion);
        }
        Payload::FriendRequestAccept(accept) => {
            // Handle accept (initialize session, add friend)
            handle_incoming_accept(app, &accept)?;
        }
        Payload::FriendRequestReject(_) => {
            // Just log it for now
            eprintln!("Friend request was rejected");
        }
        _ => {
            // Other message types not implemented yet
        }
    }

    Ok(())
}

fn handle_incoming_accept(app: &App, accept: &protocol::message::FriendRequestAcceptMessage) -> Result<()> {
    use crate::crypto::{PreKeyBundle, SignalSession, SessionStore};

    // Deserialize PreKey bundle
    let bundle: PreKeyBundle = serde_json::from_str(&accept.signal_prekey_bundle)
        .map_err(|e| error::TorrentChatError::Crypto(format!("Failed to parse bundle: {}", e)))?;

    // Initialize Signal session
    let session = SignalSession::from_prekey_bundle(
        accept.from_onion.clone(),
        &bundle
    )?;

    // Store session
    let store = SessionStore::new(&app.db);
    store.store_session(&session)?;

    // Add friend to database
    let conn = app.db.connection();
    conn.execute(
        "INSERT INTO friends (onion_address, display_name, added_at, status)
         VALUES (?1, ?2, ?3, 'active')",
        (
            &accept.from_onion,
            &accept.from_onion[..10],
            accept.timestamp,
        ),
    ).map_err(|e| error::TorrentChatError::Database(format!("Failed to add friend: {}", e)))?;

    eprintln!("Friend request accepted by {}", accept.from_onion);

    Ok(())
}
```

**Step 6: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors

**Step 7: Commit**

```bash
git add src/app.rs src/net/listener.rs src/main.rs
git commit -m "feat(net): add listener task for incoming Tor connections"
```

---

## Task 6: Update Accept Handler to Send Accept Message

**Files:**
- Modify: `src/main.rs:176-241` (handle_accept_friend_request)

**Step 1: Update handler to actually send accept message**

Replace the TODO comment with actual send logic:

```rust
async fn handle_accept_friend_request(app: &App, request_id: i64) -> Result<()> {
    // ... existing code to generate bundle and create accept_msg ...

    // Convert accept message to Message envelope
    let message = protocol::message::Message {
        id: uuid::Uuid::new_v4(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        payload: protocol::message::Payload::FriendRequestAccept(_accept_msg),
    };

    // Try to send directly, queue on failure
    match try_send_direct(app, &from_onion, &message).await {
        Ok(_) => {
            eprintln!("Accept message sent to {}", from_onion);
        }
        Err(_) => {
            app.message_queue.enqueue(&app.db, &from_onion, &message, "high")?;
            eprintln!("Accept message queued for {}", from_onion);
        }
    }

    // ... rest of existing code (add friend, mark accepted) ...

    Ok(())
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(net): send accept messages over Tor"
```

---

## Task 7: Implement Queue Processor Task

**Files:**
- Modify: `src/app.rs` (spawn queue processor)
- Modify: `src/main.rs` (handle ProcessQueue command)

**Step 1: Add command channel to App**

Edit `src/app.rs`:

```rust
pub struct App {
    // ... existing fields ...
    pub incoming_message_rx: Option<mpsc::Receiver<IncomingMessage>>,
    pub queue_command_rx: Option<mpsc::Receiver<QueueCommand>>, // NEW
}

pub enum QueueCommand {
    ProcessQueue,
}
```

**Step 2: Spawn queue processor in init_tor**

Edit `src/app.rs`, in `init_tor()`:

```rust
// Spawn queue processor task
let (cmd_tx, cmd_rx) = mpsc::channel(10);
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let _ = cmd_tx.send(QueueCommand::ProcessQueue).await;
    }
});

self.queue_command_rx = Some(cmd_rx);
```

**Step 3: Poll command channel in main loop**

Edit `src/main.rs`, in main loop:

```rust
// Check for queue processing commands
if let Some(rx) = &mut app.lock().await.queue_command_rx {
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            app::QueueCommand::ProcessQueue => {
                let app_lock = app.lock().await;
                if let Err(e) = process_message_queue(&*app_lock).await {
                    eprintln!("Failed to process queue: {}", e);
                }
                drop(app_lock);
            }
        }
    }
}
```

**Step 4: Implement process_message_queue**

Add to `src/main.rs`:

```rust
async fn process_message_queue(app: &App) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let pending = app.message_queue.get_pending_messages(&app.db, now)?;

    for queued in pending {
        match try_send_direct(app, &queued.peer_onion, &queued.message).await {
            Ok(_) => {
                app.message_queue.mark_delivered(&app.db, queued.id)?;
                eprintln!("Queued message {} delivered", queued.id);
            }
            Err(_) => {
                if queued.retry_count >= 10 {
                    app.message_queue.mark_failed(&app.db, queued.id)?;
                    eprintln!("Message {} failed after 10 retries", queued.id);
                } else {
                    // Retry in 30 seconds
                    let next_retry = now + 30;
                    app.message_queue.schedule_retry(&app.db, queued.id, next_retry)?;
                }
            }
        }
    }

    Ok(())
}
```

**Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors

**Step 6: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat(net): add queue processor for background retries"
```

---

## Task 8: Add "My Identity" UI State and Modal

**Files:**
- Modify: `src/ui/state.rs` (add ViewingMyIdentity state)
- Modify: `src/ui/app_ui.rs` (render modal)

**Step 1: Add ViewingMyIdentity to AppState**

Edit `src/ui/state.rs`:

```rust
pub enum AppState {
    Normal,
    AddingFriend {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
    ViewingFriendRequest {
        request_id: i64,
        from_onion: String,
        friend_code: String,
        timestamp: i64,
    },
    ViewingMyIdentity {  // NEW
        friend_code: String,
        onion_address: String,
    },
}
```

**Step 2: Add key handler for 'i' key**

Edit `src/ui/state.rs`, in `handle_key` method:

```rust
impl AppState {
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        match self {
            AppState::Normal => match key.code {
                // ... existing handlers ...
                KeyCode::Char('i') => {
                    // Transition handled in main.rs (needs app.onion_address)
                    return Ok(Some(AppAction::ViewMyIdentity));
                }
                // ... rest ...
            },
            AppState::ViewingMyIdentity { .. } => match key.code {
                KeyCode::Esc | KeyCode::Char('i') => {
                    *self = AppState::Normal;
                    Ok(None)
                }
                _ => Ok(None),
            },
            // ... rest ...
        }
    }
}
```

**Step 3: Add ViewMyIdentity action**

Edit `src/ui/state.rs`:

```rust
pub enum AppAction {
    SendFriendRequest(String),
    AcceptFriendRequest(i64),
    RejectFriendRequest(i64),
    ViewMyIdentity,  // NEW
    Quit,
}
```

**Step 4: Handle ViewMyIdentity in main loop**

Edit `src/main.rs`, in event loop:

```rust
Some(AppAction::ViewMyIdentity) => {
    let app_lock = app.lock().await;

    if let Some(onion) = &app_lock.onion_address {
        use crate::protocol::friend_code::generate_friend_code;

        match generate_friend_code(onion) {
            Ok(friend_code) => {
                app_state = AppState::ViewingMyIdentity {
                    friend_code,
                    onion_address: onion.clone(),
                };
            }
            Err(e) => {
                eprintln!("Failed to generate friend code: {}", e);
            }
        }
    }

    drop(app_lock);
}
```

**Step 5: Render identity modal**

Edit `src/ui/app_ui.rs`, in `render_app` function, add case:

```rust
pub fn render_app(frame: &mut Frame, app_state: &AppState, app: &App) {
    match app_state {
        // ... existing cases ...
        AppState::ViewingMyIdentity { friend_code, onion_address } => {
            render_identity_modal(frame, friend_code, onion_address);
        }
    }
}

fn render_identity_modal(frame: &mut Frame, friend_code: &str, onion_address: &str) {
    use ratatui::{
        layout::{Constraint, Direction, Layout, Rect},
        widgets::{Block, Borders, Paragraph, Wrap},
        style::{Color, Modifier, Style},
    };

    let area = centered_rect(60, 50, frame.area());

    let block = Block::default()
        .title(" My Identity ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    // Friend code label
    let label1 = Paragraph::new("Share this friend code:")
        .style(Style::default().fg(Color::White));
    frame.render_widget(label1, chunks[0]);

    // Friend code value
    let friend_code_widget = Paragraph::new(friend_code)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        .wrap(Wrap { trim: false });
    frame.render_widget(friend_code_widget, chunks[1]);

    // Onion address label
    let label2 = Paragraph::new("Onion Address (advanced):")
        .style(Style::default().fg(Color::White));
    frame.render_widget(label2, chunks[2]);

    // Onion address value
    let onion_widget = Paragraph::new(onion_address)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Yellow))
        .wrap(Wrap { trim: false });
    frame.render_widget(onion_widget, chunks[3]);

    // Help text
    let help = Paragraph::new("[i/Esc] Close")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[4]);
}

// Helper function (add if not exists)
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
```

**Step 6: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors

**Step 7: Commit**

```bash
git add src/ui/state.rs src/ui/app_ui.rs src/main.rs
git commit -m "feat(ui): add identity modal to display friend code"
```

---

## Task 9: Integration Testing

**Files:**
- Manual testing with two instances

**Step 1: Build release binary**

Run: `cargo build --release`

**Step 2: Test with two instances**

Terminal 1:
```bash
./target/release/torrent-chat --config-dir /tmp/alice
```

Terminal 2:
```bash
./target/release/torrent-chat --config-dir /tmp/bob
```

**Step 3: Test friend request flow**

1. In Alice's terminal, press `i` → verify friend code displays
2. Copy Alice's friend code
3. In Bob's terminal, press `a` → enter Alice's friend code → Enter
4. Verify Bob sees "Sent ✓" or "Queued 🕐"
5. In Alice's terminal, press `r` → verify Bob's request appears
6. Press `y` to accept
7. Verify both terminals show success messages

**Step 4: Verify database state**

```bash
sqlite3 /tmp/alice/.local/share/torrent-chat/chat.db "SELECT * FROM friends"
sqlite3 /tmp/bob/.local/share/torrent-chat/chat.db "SELECT * FROM friends"
```

Expected: Both show each other as friends

**Step 5: Test queue resilience**

1. Stop Bob's instance
2. In Alice, send friend request to Bob
3. Verify it shows "Queued 🕐"
4. Start Bob's instance
5. Wait 30 seconds
6. Verify message delivers and Bob sees request

**Step 6: Document results**

Create `docs/testing/manual-test-results.md` with findings.

**Step 7: Commit**

```bash
git add docs/testing/manual-test-results.md
git commit -m "docs: add manual testing results"
```

---

## Task 10: Run Full Test Suite

**Files:**
- All test files

**Step 1: Run all unit tests**

Run: `cargo test --lib`
Expected: All tests PASS

**Step 2: Run all integration tests**

Run: `cargo test --test '*'`
Expected: All tests PASS

**Step 3: Check for warnings**

Run: `cargo clippy`
Expected: No errors or warnings

**Step 4: Commit if any fixes needed**

```bash
git add .
git commit -m "fix: address clippy warnings"
```

---

## Success Criteria

✅ **Phase 1 MVP Complete When:**
- [ ] User can press `i` to see their friend code
- [ ] User can send friend request → it sends immediately (if peer online) or queues
- [ ] User receives friend requests in real-time
- [ ] User can accept friend request → both establish Signal session
- [ ] User can reject friend request → removed from database
- [ ] Queued messages retry every 30 seconds
- [ ] After 10 retries, messages marked as failed
- [ ] All tests passing (89+ unit tests)
- [ ] Two instances can complete full friend request flow

**Ready to move to Phase 2 after this!**
