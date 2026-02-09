# Phase 2b Production Implementation - Design Document

**Date:** 2026-02-09
**Status:** Approved
**Goal:** Replace MVP stubs with production-ready Signal Protocol, Tor connections, and interactive UI

---

## Overview

Phase 2b MVP is complete with functional stubs. This phase upgrades three core subsystems to production quality:

1. **Signal Protocol** - Replace placeholder crypto with real libsignal-dezire (X3DH + Double Ratchet)
2. **Tor Connections** - Replace localhost with real arti native streams to .onion addresses
3. **Interactive UI** - Add keyboard handling via state machine architecture

---

## Architecture Overview

### High-Level Changes

**Signal Protocol (Hybrid Architecture)**
- Use libsignal-dezire internally for real cryptography
- Maintain custom serialization layer for stable database schema
- Clean break: no migration from stub sessions
- Dependency: `libsignal-protocol` crate

**Tor Connections (Native Arti Streams)**
- Use `TorClient::connect()` to get arti `DataStream`
- Refactor framing layer to be generic over `AsyncRead + AsyncWrite`
- Works with both `TcpStream` (tests) and `DataStream` (production)
- Tests require local Tor daemon for realistic integration testing

**UI State Machine**
- Add `AppState` enum for clear state management
- Keyboard events dispatch to state-specific handlers
- Modals become views of current state
- Scales well for future UI complexity (Phase 3+)

### Integration Points

```
User Input → AppState → KeyEvent Handler → AppAction
                ↓
          UI Rendering (modals/views)

Message Send → MessageSender → SignalSession::encrypt()
                ↓
          TorConnection::connect() → DataStream → Framing → Network

Message Receive → Listener → Framing → MessageReceiver
                ↓
          SignalSession::decrypt() → Database
```

---

## Signal Protocol Implementation

### libsignal-dezire Integration

**Core Components:**

```rust
// Real key generation
use libsignal_protocol::{IdentityKeyPair, KeyPair, PreKeyRecord, SignedPreKeyRecord};

impl PreKeyBundle {
    pub fn generate(identity: &IdentityKeyPair) -> Result<Self> {
        // Generate real Curve25519 keys
        let signed_prekey = SignedPreKeyRecord::new(
            key_id,
            timestamp,
            &keypair,
            &identity.private_key()
        )?;

        let one_time_prekey = PreKeyRecord::new(key_id, &keypair)?;

        Ok(PreKeyBundle {
            identity_key: identity.public_key().serialize(),
            signed_prekey: SignedPreKey {
                key_id: signed_prekey.id(),
                public_key: signed_prekey.public_key().serialize(),
                signature: signed_prekey.signature().to_vec(),
            },
            prekey: Some(PreKey {
                key_id: one_time_prekey.id(),
                public_key: one_time_prekey.public_key().serialize(),
            }),
        })
    }
}
```

**Session Management:**

```rust
pub struct SignalSession {
    remote_onion: String,
    cipher: SessionCipher, // Real libsignal cipher with Double Ratchet
}

impl SignalSession {
    /// Create session from PreKey bundle (X3DH key agreement)
    pub fn from_prekey_bundle(
        remote_onion: String,
        bundle: &PreKeyBundle,
        identity: &IdentityKeyPair,
        store: &dyn SignalProtocolStore,
    ) -> Result<Self> {
        // Process bundle, perform X3DH, initialize session
        let address = ProtocolAddress::new(remote_onion.clone(), 1);
        process_prekey_bundle(&address, &mut store.session_store(), bundle)?;

        let cipher = SessionCipher::new(store, &address)?;

        Ok(SignalSession {
            remote_onion,
            cipher,
        })
    }

    /// Encrypt plaintext with Double Ratchet
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<CiphertextMessage> {
        self.cipher.encrypt(plaintext)
            .map_err(|e| TorrentChatError::SignalProtocol(e))
    }

    /// Decrypt ciphertext with Double Ratchet (forward secrecy)
    pub fn decrypt(&mut self, ciphertext: &CiphertextMessage) -> Result<Vec<u8>> {
        self.cipher.decrypt(ciphertext)
            .map_err(|e| TorrentChatError::SignalProtocol(e))
    }
}
```

**Persistence Layer (Custom Format):**

```rust
// Store libsignal session state in database
impl SessionStore {
    pub fn store_session(&self, session: &SignalSession) -> Result<()> {
        let bytes = session.cipher.serialize()?;

        self.db.connection().execute(
            "INSERT OR REPLACE INTO signal_sessions (remote_onion, session_state, updated_at)
             VALUES (?1, ?2, ?3)",
            (&session.remote_onion, &bytes, current_timestamp()),
        )?;

        Ok(())
    }

    pub fn load_session(&self, remote_onion: &str) -> Result<Option<SignalSession>> {
        let bytes: Option<Vec<u8>> = self.db.connection()
            .query_row(
                "SELECT session_state FROM signal_sessions WHERE remote_onion = ?1",
                [remote_onion],
                |row| row.get(0),
            )
            .optional()?;

        match bytes {
            Some(data) => {
                let cipher = SessionCipher::deserialize(&data)?;
                Ok(Some(SignalSession {
                    remote_onion: remote_onion.to_string(),
                    cipher,
                }))
            }
            None => Ok(None),
        }
    }
}
```

**X3DH Key Agreement Flow:**

1. **Initiator (Alice):**
   - Fetches Bob's PreKey bundle
   - Performs X3DH calculation
   - Sends initial PreKeyMessage
   - Session established with shared secret

2. **Recipient (Bob):**
   - Receives PreKeyMessage
   - Extracts Alice's identity key
   - Performs X3DH calculation
   - Session established with same shared secret
   - Can now send/receive with Double Ratchet

**Schema Changes:**
- `signal_sessions.session_state` BLOB now contains serialized `SessionCipher`
- Incompatible with old stub format
- Migration: `DELETE FROM signal_sessions` (clean break)

---

## Tor Native Streams

### Arti DataStream Integration

**Generic Framing Layer:**

Refactor `src/net/framing.rs` to work with any async stream:

```rust
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

pub async fn send_message<S>(stream: &mut S, message: &Message) -> Result<()>
where
    S: AsyncWrite + Unpin
{
    // Serialize to JSON
    let json = serde_json::to_vec(message)
        .map_err(|e| TorrentChatError::Network(format!("Failed to serialize: {}", e)))?;

    if json.len() > 10_000_000 {
        return Err(TorrentChatError::Network("Message too large".into()));
    }

    // Write length prefix (4 bytes, big-endian)
    let len = (json.len() as u32).to_be_bytes();
    stream.write_all(&len).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to write length: {}", e)))?;

    // Write message payload
    stream.write_all(&json).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to write payload: {}", e)))?;

    // Flush to ensure sent
    stream.flush().await
        .map_err(|e| TorrentChatError::Network(format!("Failed to flush: {}", e)))?;

    Ok(())
}

pub async fn receive_message<S>(stream: &mut S) -> Result<Message>
where
    S: AsyncRead + Unpin
{
    // Read length prefix (4 bytes, big-endian)
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to read length: {}", e)))?;

    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 10_000_000 {
        return Err(TorrentChatError::Network("Message too large".into()));
    }

    // Read message payload
    let mut json_bytes = vec![0u8; len];
    stream.read_exact(&mut json_bytes).await
        .map_err(|e| TorrentChatError::Network(format!("Failed to read payload: {}", e)))?;

    // Deserialize message
    let message: Message = serde_json::from_slice(&json_bytes)
        .map_err(|e| TorrentChatError::Network(format!("Failed to parse message: {}", e)))?;

    Ok(message)
}
```

**Real Tor Connections:**

Replace `TorConnection::connect()` implementation:

```rust
use arti_client::DataStream;

pub struct TorConnection {
    pub remote_onion: String,
    stream: DataStream, // Changed from TcpStream
}

impl TorConnection {
    /// Connect to peer via Tor
    pub async fn connect(
        tor_client: &TorClient,
        remote_onion: &str,
    ) -> Result<Self> {
        // Use arti's native connect (returns DataStream)
        let stream = tor_client.inner()
            .connect((remote_onion, 9051))
            .await
            .map_err(|e| TorrentChatError::Tor(format!("Failed to connect to {}: {}", remote_onion, e)))?;

        Ok(TorConnection {
            remote_onion: remote_onion.to_string(),
            stream,
        })
    }

    /// Send message over connection
    pub async fn send(&mut self, message: &Message) -> Result<()> {
        send_message(&mut self.stream, message).await
    }

    /// Receive message from connection
    pub async fn receive(&mut self) -> Result<Message> {
        receive_message(&mut self.stream).await
    }
}
```

**Testing Support:**

```rust
// For tests that need TcpStream
#[cfg(test)]
impl TorConnection {
    pub async fn connect_direct(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).await
            .map_err(|e| TorrentChatError::Network(format!("Failed to connect: {}", e)))?;

        // Wrap TcpStream in a way that works with generic framing
        Ok(TorConnection {
            remote_onion: "test.onion".to_string(),
            stream: stream.into(), // Convert via trait
        })
    }
}
```

**Connection Flow:**

```
App wants to send message
    ↓
TorConnection::connect(tor_client, "abc123...xyz.onion")
    ↓
tor_client.inner().connect() → Tor SOCKS connection
    ↓
Returns DataStream (implements AsyncRead + AsyncWrite)
    ↓
send_message(&mut stream, message) → Generic framing
    ↓
Length prefix + JSON → Network via Tor circuit
```

**Testing Requirements:**
- Unit tests: Use `TcpStream::connect("127.0.0.1:...")` for fast tests
- Integration tests: Require Tor daemon on localhost:9050
- Full E2E tests: Two instances with real .onion addresses

---

## UI State Machine

### AppState Architecture

**State Definition:**

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
    // Future states for Phase 3+
    // Chatting { conversation_id: i64 },
    // ViewingChannel { channel_id: i64 },
    // Settings,
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Normal
    }
}
```

**Event Handling:**

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum AppAction {
    SendFriendRequest(String),      // friend_code
    AcceptFriendRequest(i64),        // request_id
    RejectFriendRequest(i64),        // request_id
    Quit,
}

impl AppState {
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<Option<AppAction>> {
        match (self, key.code, key.modifiers) {
            // Global: Ctrl+C to quit
            (_, KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                Ok(Some(AppAction::Quit))
            }

            // Normal mode
            (AppState::Normal, KeyCode::Char('a'), _) => {
                *self = AppState::AddingFriend {
                    input: String::new(),
                    cursor: 0,
                    error: None,
                };
                Ok(None)
            }
            (AppState::Normal, KeyCode::Char('q'), _) => {
                Ok(Some(AppAction::Quit))
            }

            // Adding friend mode
            (AppState::AddingFriend { input, cursor, .. }, KeyCode::Char(c), _) => {
                input.insert(*cursor, c);
                *cursor += 1;
                Ok(None)
            }
            (AppState::AddingFriend { input, cursor, .. }, KeyCode::Backspace, _) => {
                if *cursor > 0 {
                    input.remove(*cursor - 1);
                    *cursor -= 1;
                }
                Ok(None)
            }
            (AppState::AddingFriend { input, cursor, .. }, KeyCode::Left, _) => {
                if *cursor > 0 {
                    *cursor -= 1;
                }
                Ok(None)
            }
            (AppState::AddingFriend { input, cursor, .. }, KeyCode::Right, _) => {
                if *cursor < input.len() {
                    *cursor += 1;
                }
                Ok(None)
            }
            (AppState::AddingFriend { input, .. }, KeyCode::Enter, _) => {
                if input.is_empty() {
                    // Update error in state
                    if let AppState::AddingFriend { error, .. } = self {
                        *error = Some("Friend code cannot be empty".to_string());
                    }
                    Ok(None)
                } else {
                    Ok(Some(AppAction::SendFriendRequest(input.clone())))
                }
            }
            (AppState::AddingFriend { .. }, KeyCode::Esc, _) => {
                *self = AppState::Normal;
                Ok(None)
            }

            // Viewing friend request
            (AppState::ViewingFriendRequest { request_id, .. }, KeyCode::Char('a'), _) |
            (AppState::ViewingFriendRequest { request_id, .. }, KeyCode::Char('A'), _) => {
                Ok(Some(AppAction::AcceptFriendRequest(*request_id)))
            }
            (AppState::ViewingFriendRequest { request_id, .. }, KeyCode::Char('r'), _) |
            (AppState::ViewingFriendRequest { request_id, .. }, KeyCode::Char('R'), _) => {
                Ok(Some(AppAction::RejectFriendRequest(*request_id)))
            }
            (AppState::ViewingFriendRequest { .. }, KeyCode::Esc, _) => {
                *self = AppState::Normal;
                Ok(None)
            }

            // Unhandled key in current state
            _ => Ok(None),
        }
    }
}
```

**Main Loop Integration:**

```rust
// In main.rs
let mut app_state = AppState::default();

loop {
    // Render current state
    terminal.draw(|f| {
        render_app(f, &app_state, &app);
    })?;

    // Handle events
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            match app_state.handle_key(key)? {
                Some(AppAction::SendFriendRequest(code)) => {
                    match app.send_friend_request(&code) {
                        Ok(_) => app_state = AppState::Normal,
                        Err(e) => {
                            // Update error in state
                            if let AppState::AddingFriend { error, .. } = &mut app_state {
                                *error = Some(format_error(&e));
                            }
                        }
                    }
                }
                Some(AppAction::AcceptFriendRequest(id)) => {
                    app.accept_friend_request(id)?;
                    app_state = AppState::Normal;
                }
                Some(AppAction::RejectFriendRequest(id)) => {
                    app.reject_friend_request(id)?;
                    app_state = AppState::Normal;
                }
                Some(AppAction::Quit) => break,
                None => {} // Just state change, no action
            }
        }
    }
}
```

**Rendering:**

```rust
fn render_app(f: &mut Frame, state: &AppState, app: &App) {
    // Base layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Footer
        ])
        .split(f.size());

    // Render based on state
    match state {
        AppState::Normal => {
            render_main_view(f, chunks[1], app);
        }
        AppState::AddingFriend { input, cursor, error } => {
            render_main_view(f, chunks[1], app);
            render_add_friend_modal(f, input, *cursor, error.as_deref());
        }
        AppState::ViewingFriendRequest { from_onion, friend_code, .. } => {
            render_main_view(f, chunks[1], app);
            render_friend_request_modal(f, from_onion, friend_code);
        }
    }
}
```

**Benefits:**
- Clear state transitions (easy to reason about)
- Testable in isolation (no UI framework needed)
- Scales to many states (chat, channels, settings)
- Action-based side effects (clean separation of concerns)

---

## Testing Strategy

### Local Tor Testing

**Requirements:**
- Developers must have Tor daemon running locally (port 9050)
- Tests use real Tor connections to real .onion addresses
- Slower but high confidence in correctness

**Test Categories:**

**1. Unit Tests (Fast)**
```rust
// Use TcpStream with localhost, no Tor required
#[tokio::test]
async fn test_message_framing() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Test framing without Tor
    // ...
}

// Mock Signal Protocol for speed
#[test]
fn test_session_storage() {
    let mock_session = create_mock_session();
    let store = SessionStore::new(&db);
    store.store_session(&mock_session).unwrap();
    // ...
}
```

**2. Integration Tests (Slow, Require Tor)**
```rust
fn require_tor() {
    if !tor_is_running() {
        eprintln!("⚠️  Integration tests require Tor daemon");
        eprintln!("   Install: brew install tor (macOS) or apt install tor (Linux)");
        eprintln!("   Start: brew services start tor (macOS) or sudo systemctl start tor (Linux)");
        panic!("Tor daemon not available on localhost:9050");
    }
}

fn tor_is_running() -> bool {
    std::net::TcpStream::connect("127.0.0.1:9050").is_ok()
}

#[tokio::test]
async fn test_real_tor_connection() {
    require_tor();

    let tor_client = TorClient::new().await.unwrap();
    let conn = TorConnection::connect(&tor_client, "some.onion").await;

    assert!(conn.is_ok());
}
```

**3. E2E Tests (Manual, #[ignore])**
```rust
#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn test_two_instance_encrypted_messaging() {
    require_tor();

    // Full two-instance test with real crypto
    let alice = create_test_instance("alice").await;
    let bob = create_test_instance("bob").await;

    // Alice sends friend request
    alice.send_friend_request(&bob.friend_code()).await.unwrap();

    // Bob accepts
    bob.accept_friend_request(alice.onion_address()).await.unwrap();

    // Alice sends encrypted message
    alice.send_message(&bob.onion_address(), "Hello Bob!").await.unwrap();

    // Bob receives and decrypts
    let messages = bob.get_messages().await.unwrap();
    assert_eq!(messages[0].content, "Hello Bob!");
}
```

**CI Strategy:**

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: dtolnay/rust-toolchain@stable
      - name: Run unit tests
        run: cargo test --lib

  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: dtolnay/rust-toolchain@stable
      - name: Install Tor
        run: |
          sudo apt-get update
          sudo apt-get install -y tor
      - name: Start Tor daemon
        run: |
          tor &
          sleep 5
      - name: Run integration tests
        run: cargo test --test integration_tests
        continue-on-error: true  # Don't fail build if Tor flaky
```

**Documentation:**

Add to `docs/Testing-Phase2b.md`:
```markdown
## Running Tests

### Unit Tests (Fast)
```bash
cargo test --lib
```

### Integration Tests (Require Tor)
```bash
# Install Tor
brew install tor          # macOS
apt install tor           # Linux

# Start Tor daemon
brew services start tor   # macOS
sudo systemctl start tor  # Linux

# Run integration tests
cargo test --test integration_tests
```

### E2E Tests (Manual)
```bash
cargo test -- --ignored --nocapture
```
```

---

## Error Handling

### Production Error Types

**Expanded Error Enum:**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TorrentChatError {
    // Existing errors
    #[error("Database error: {0}")]
    Database(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // New: Signal Protocol errors
    #[error("Signal Protocol error: {0}")]
    SignalProtocol(#[from] libsignal_protocol::Error),

    #[error("Session not found for {0}")]
    SessionNotFound(String),

    #[error("Invalid PreKey bundle: {0}")]
    InvalidPreKeyBundle(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Untrusted identity for {0}")]
    UntrustedIdentity(String),

    // New: Tor errors
    #[error("Tor connection error: {0}")]
    TorConnection(String),

    #[error("Failed to bootstrap Tor: {0}")]
    TorBootstrap(String),

    #[error("Invalid .onion address: {0}")]
    InvalidOnionAddress(String),

    // Network errors
    #[error("Network error: {0}")]
    Network(String),

    #[error("Connection timeout to {0}")]
    ConnectionTimeout(String),
}
```

**Graceful Degradation:**

```rust
// In MessageReceiver
impl MessageReceiver {
    pub async fn handle_incoming(&mut self, msg: TextMessage) -> Result<()> {
        match self.decrypt_message(&msg) {
            Ok(plaintext) => {
                self.store_message(plaintext)?;
                Ok(())
            }
            Err(TorrentChatError::SessionNotFound(onion)) => {
                // Don't crash, just log and skip
                warn!("No session for {}, message dropped. User should re-add friend.", onion);
                Ok(())
            }
            Err(TorrentChatError::DecryptionFailed(reason)) => {
                // Possible replay attack or corrupted message
                error!("Decryption failed for {}: {}", msg.from_onion, reason);
                // Store as "failed" message for debugging?
                Ok(())
            }
            Err(e) => {
                // Propagate unexpected errors
                Err(e)
            }
        }
    }
}
```

**User-Facing Error Messages:**

```rust
pub fn format_error_for_user(err: &TorrentChatError) -> String {
    match err {
        TorrentChatError::SignalProtocol(_) =>
            "Encryption error. Try re-adding this friend.".into(),

        TorrentChatError::SessionNotFound(_) =>
            "No secure session found. Please send a friend request first.".into(),

        TorrentChatError::TorConnection(_) | TorrentChatError::TorBootstrap(_) =>
            "Tor connection failed. Please check your internet connection and Tor status.".into(),

        TorrentChatError::InvalidOnionAddress(addr) =>
            format!("Invalid friend address: {}", addr),

        TorrentChatError::ConnectionTimeout(addr) =>
            format!("Connection timeout. {} may be offline.", addr),

        _ => format!("Error: {}", err),
    }
}

// Use in UI state
impl AppState {
    pub fn set_error(&mut self, err: TorrentChatError) {
        let user_message = format_error_for_user(&err);

        match self {
            AppState::AddingFriend { error, .. } => {
                *error = Some(user_message);
            }
            _ => {
                // Log but don't show in UI
                error!("Error in state {:?}: {}", self, user_message);
            }
        }
    }
}
```

**Structured Logging:**

```rust
use tracing::{error, warn, info, debug};

// In TorConnection::connect()
info!(
    onion_address = %remote_onion,
    "Connecting to peer via Tor"
);

match tor_client.inner().connect((remote_onion, 9051)).await {
    Ok(stream) => {
        info!(
            onion_address = %remote_onion,
            "Connected successfully"
        );
        Ok(stream)
    }
    Err(e) => {
        error!(
            onion_address = %remote_onion,
            error = %e,
            "Failed to connect"
        );
        Err(TorrentChatError::TorConnection(e.to_string()))
    }
}

// In SignalSession::decrypt()
debug!(
    remote_onion = %self.remote_onion,
    message_type = ?ciphertext.message_type(),
    "Decrypting message"
);

match self.cipher.decrypt(ciphertext) {
    Ok(plaintext) => {
        debug!(
            remote_onion = %self.remote_onion,
            plaintext_len = plaintext.len(),
            "Decryption successful"
        );
        Ok(plaintext)
    }
    Err(e) => {
        error!(
            remote_onion = %self.remote_onion,
            error = %e,
            "Decryption failed"
        );
        Err(TorrentChatError::SignalProtocol(e))
    }
}
```

---

## Migration & Deployment

### Database Schema Changes

**Schema Version 3:**

```sql
-- Bump schema version
UPDATE schema_version SET version = 3;

-- Clear old stub sessions (incompatible format)
DELETE FROM signal_sessions;

-- Add metadata column for debugging
ALTER TABLE signal_sessions ADD COLUMN protocol_version INTEGER DEFAULT 1;
ALTER TABLE signal_sessions ADD COLUMN last_error TEXT;
```

**Migration Code:**

```rust
// In src/db/connection.rs
impl Database {
    fn migrate_to_v3(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 3 {
            info!("🔄 Migrating database to schema v3 (production Signal Protocol)");

            let conn = self.connection();

            // Clear old stub sessions
            let deleted = conn.execute("DELETE FROM signal_sessions", [])?;
            info!("   Cleared {} old Signal sessions", deleted);

            // Add new columns
            conn.execute(
                "ALTER TABLE signal_sessions ADD COLUMN IF NOT EXISTS protocol_version INTEGER DEFAULT 1",
                []
            )?;
            conn.execute(
                "ALTER TABLE signal_sessions ADD COLUMN IF NOT EXISTS last_error TEXT",
                []
            )?;

            // Update version
            conn.execute(
                "UPDATE schema_version SET version = 3",
                []
            )?;

            warn!("⚠️  Schema upgraded to v3. All Signal sessions cleared.");
            warn!("   You'll need to re-establish sessions by:");
            warn!("   1. Re-sending friend requests to existing contacts");
            warn!("   2. Waiting for them to accept again");
            warn!("   This is a one-time migration for production crypto.");

            info!("✅ Migration to schema v3 complete");
        }

        Ok(())
    }
}
```

**User Communication:**

On startup, if migration occurs:
```
╔════════════════════════════════════════════════════════════╗
║           Database Upgraded to v3                          ║
╠════════════════════════════════════════════════════════════╣
║  Your database has been upgraded to use production-grade   ║
║  Signal Protocol encryption. As a result:                  ║
║                                                            ║
║  • All previous sessions have been cleared                 ║
║  • You'll need to re-add existing friends                  ║
║  • Messages will now use real end-to-end encryption        ║
║                                                            ║
║  This is a one-time migration. Future updates will         ║
║  preserve your sessions.                                   ║
╚════════════════════════════════════════════════════════════╝
```

### Rollout Plan

**Phase 1: Development (This Phase)**
- [ ] Implement libsignal integration
- [ ] Implement arti native streams
- [ ] Implement UI state machine
- [ ] Test with local Tor daemon
- [ ] Validate with two-instance e2e tests
- [ ] Update documentation

**Phase 2: Alpha Testing (Future)**
- [ ] Deploy to small group of testers
- [ ] Monitor error rates and logs
- [ ] Gather feedback on UX
- [ ] Fix bugs and rough edges
- [ ] Performance tuning

**Phase 3: Beta Release (Future)**
- [ ] Public beta announcement
- [ ] Comprehensive documentation
- [ ] Setup guides for Tor daemon
- [ ] Support channels (GitHub issues)

**Phase 4: Stable Release (Future)**
- [ ] Release v0.2.0 with production crypto
- [ ] Announce on privacy forums
- [ ] Package for distributions (Homebrew, AUR, etc.)

### Feature Flags (Optional)

For gradual rollout during development:

```rust
// In Cargo.toml
[features]
default = ["real-crypto"]
real-crypto = []
stub-crypto = []

// In code
#[cfg(feature = "real-crypto")]
use libsignal_protocol as signal;

#[cfg(feature = "stub-crypto")]
mod signal {
    // Stub implementations for testing
}
```

### Backward Compatibility

**Not supported** - this is a clean break:
- Old Signal sessions cannot be migrated
- Users must re-establish all friendships
- No compatibility mode or gradual migration

**Rationale:**
- No users yet (MVP stage)
- Stub sessions contained no real crypto state
- Clean break is simpler and safer
- Future migrations will be more careful (after v1.0)

---

## Implementation Tasks

### Task Breakdown

**Task 1: libsignal Integration (5-7 days)**
- Add libsignal-protocol dependency
- Implement real PreKeyBundle generation
- Implement SessionCipher wrapper
- Implement X3DH key agreement
- Update session storage layer
- Write comprehensive crypto tests

**Task 2: Arti Native Streams (3-4 days)**
- Make framing layer generic over AsyncRead/AsyncWrite
- Update TorConnection to use DataStream
- Test with real Tor daemon
- Add connection timeout handling
- Update integration tests

**Task 3: UI State Machine (2-3 days)**
- Define AppState enum
- Implement keyboard event handlers
- Implement state transitions
- Update rendering based on state
- Add error display in states

**Task 4: Error Handling (2 days)**
- Expand TorrentChatError types
- Add graceful degradation logic
- Implement user-facing error messages
- Add structured logging

**Task 5: Migration & Testing (2-3 days)**
- Implement schema v3 migration
- Write integration tests with Tor
- Update e2e tests for real crypto
- Update documentation

**Total Estimated Time:** 14-19 days

---

## Success Criteria

### Must Have
- [ ] Real Signal Protocol encryption (X3DH + Double Ratchet)
- [ ] Real Tor connections to .onion addresses
- [ ] Interactive UI with keyboard handling
- [ ] All unit tests passing
- [ ] Integration tests passing (with Tor daemon)
- [ ] Database migration successful

### Should Have
- [ ] Graceful error handling
- [ ] Structured logging
- [ ] User-friendly error messages
- [ ] Two-instance e2e test passing

### Nice to Have
- [ ] CI with Tor integration tests
- [ ] Performance benchmarks
- [ ] Connection pooling optimizations

---

## Risks & Mitigations

### Risk: libsignal Complexity
**Mitigation:** Start with simple wrapper, iterate based on actual usage patterns

### Risk: Tor Daemon Dependency
**Mitigation:** Clear documentation, fallback to localhost for development, CI allows failures

### Risk: Session Compatibility
**Mitigation:** Clean break now, careful versioning after v1.0

### Risk: Performance
**Mitigation:** Real Tor is slower, but acceptable for P2P chat. Profile if needed.

---

## Next Steps

1. **Review & Approve Design** ✅
2. **Create Implementation Plan** - Break down into detailed tasks with TDD approach
3. **Set Up Worktree** - Isolated workspace for implementation
4. **Execute Tasks** - Follow implementation plan with testing

---

## References

- [libsignal-protocol Rust docs](https://docs.rs/libsignal-protocol)
- [Arti documentation](https://docs.rs/arti-client)
- [Signal Protocol specification](https://signal.org/docs/)
- [Tor v3 onion services](https://community.torproject.org/onion-services/)
- Phase 2b MVP: `docs/Phase2b-Progress.md`
