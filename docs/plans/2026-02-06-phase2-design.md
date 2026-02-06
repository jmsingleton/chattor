# Phase 2: Tor Messaging & Encryption - Design Document

**Date:** 2026-02-06
**Status:** Approved
**Phase:** 2 of 5

## Overview

Phase 2 adds Tor hidden service networking and end-to-end encrypted peer-to-peer messaging to torrent-chat. This completes the original "Phase 1" vision from the design document, delivering a functional encrypted chat application over Tor.

**Goal:** Enable two users to exchange encrypted messages over Tor hidden services without any central servers.

**Deliverable:** MVP with functional 1-on-1 encrypted chat over Tor.

---

## Section 1: Architecture Overview

### Core Architecture

Each user runs a Tor hidden service (via `arti` library) that starts with the app. Friend codes map deterministically to .onion addresses. Direct P2P connections flow from Alice's hidden service → Tor network → Bob's hidden service.

**Technology Stack:**
- **Networking:** Tor hidden services via `arti` (Rust Tor implementation)
- **Protocol:** JSON-over-TCP with length-prefixed framing
- **Encryption:** Signal Protocol (Double Ratchet) via `libsignal-protocol-rust`
- **Storage:** SQLCipher for encrypted local message queue and Signal sessions
- **Async:** Tokio for concurrent connection handling

**Component Stack:**
```
┌─────────────────────────────────────┐
│         TUI (ratatui)               │
├─────────────────────────────────────┤
│    App State & Message Queue        │
├─────────────────────────────────────┤
│  Signal Protocol (E2E Encryption)   │
├─────────────────────────────────────┤
│    JSON Protocol (TCP framing)      │
├─────────────────────────────────────┤
│   Tor Hidden Service (arti)         │
├─────────────────────────────────────┤
│          Tor Network                │
└─────────────────────────────────────┘
```

**Key Properties:**
- **No central servers:** Pure peer-to-peer architecture
- **Double encryption:** Signal Protocol E2E encryption + Tor transport encryption
- **Offline resilience:** Messages queued locally and delivered on reconnect
- **Manual approval:** Friend requests require explicit user acceptance
- **Privacy-first:** No metadata leaks, no phone numbers, no registration

### Design Decisions

**Why full hidden service per user?**
- Maximum privacy: No relay sees metadata
- True peer-to-peer: No dependencies on infrastructure
- Resource usage acceptable: ~50-100MB memory, modest CPU
- Latency acceptable: 300-800ms for text chat

**Why Signal Protocol?**
- Battle-tested: Used by WhatsApp, Signal, etc.
- Forward secrecy: Past messages secure even if keys compromised
- Future secrecy: Keys rotate with each message
- Well-maintained Rust library available

**Why JSON over TCP?**
- Simple and debuggable for MVP
- Easy to extend with new message types
- Human-readable during development
- Good enough performance for text chat

---

## Section 2: Tor Hidden Service Integration

### Hidden Service Lifecycle

**On App Startup:**
1. Initialize `arti` Tor client in background async task (tokio runtime)
2. Load Ed25519 identity keypair for hidden service from database
   - If first run, generate new keypair and store encrypted
3. Derive .onion address from identity key (deterministic, v3 format)
4. Bootstrap Tor connection (typically 30-60 seconds)
   - Show progress bar in TUI: "Connecting to Tor... 45%"
5. Publish hidden service descriptor to Tor directory servers
6. Start listening on local port (127.0.0.1:9051)
7. Map friend code ↔ .onion address bidirectionally in memory

**During Runtime:**
- Hidden service remains online continuously while app runs
- Accepts incoming connections from friends' hidden services
- Initiates outgoing connections to send messages or friend requests
- Maintains Tor circuits (arti handles this automatically)
- Handle connection failures gracefully:
  - Retry with exponential backoff (1s, 2s, 4s, 8s, max 60s)
  - Update UI with connection status
  - Queue messages for retry

**On App Shutdown:**
1. Close all active connections cleanly (send TCP FIN)
2. Stop accepting new connections
3. Wait for in-flight messages to complete (timeout 5s)
4. Stop hidden service
5. Save state: Signal sessions, message queue, settings

### Address Management

**Identity Key Storage:**
- Ed25519 keypair stored in encrypted database
- Key derivation: Master password → Argon2id → database key
- Never stored in plaintext
- Same key used for .onion and message signing

**.onion Address Generation:**
- Deterministic from identity key (v3 onion format)
- 56-character address: `base32(public_key | checksum | version).onion`
- Example: `alice2k3b4d5e6f7g8h9j1m2n4p6q8r9s1t3v5w7x9y2a3b5c7d9e.onion`

**Friend Code Mapping:**
- Friend code format: `word-NNNN-word-NNNN` (e.g., `happy-1234-tiger-5678`)
- Encoding: Hash of .onion → 4 bytes → split into two 16-bit values
- Each 16-bit value → word index (first byte) + number (second byte)
- Includes checksum to detect typos
- Reversible: Friend code → .onion address lookup

### Module Structure

**Files to create:**
- `src/tor/mod.rs` - Module exports
- `src/tor/hidden_service.rs` - Hidden service management (start, stop, status)
- `src/tor/client.rs` - Arti client wrapper and configuration
- `src/tor/connection.rs` - Connection handling (incoming/outgoing TCP streams)
- `src/tor/address.rs` - .onion ↔ friend code mapping

**Key Types:**
```rust
pub struct TorService {
    client: Arc<TorClient>,
    hidden_service: HiddenService,
    listener: TcpListener,
    connections: HashMap<String, Connection>,
}

pub struct Connection {
    stream: TcpStream,
    remote_onion: String,
    state: ConnectionState,
}
```

---

## Section 3: Friend Request Protocol

### Friend Request Flow

**Initiation (Alice adds Bob):**

1. **User Input:** Alice enters Bob's friend code in TUI
2. **Validation:** App validates format using checksum
3. **Resolve:** Map friend code → Bob's .onion address
4. **Connect:** Alice's hidden service initiates TCP connection to Bob's .onion
   - Tor establishes 6-hop circuit (3 sender + 3 receiver)
   - Typical time: 2-5 seconds
5. **Send Request:** Alice sends `FriendRequest` message:

```json
{
  "type": "friend_request",
  "from_onion": "alice2k3b4d5e6f7g8h9j1m2n4p6q8r9s1t3v5w7x9y2a3b5c7d9e.onion",
  "from_friendcode": "happy-1234-tiger-5678",
  "timestamp": 1738886400,
  "signature": "base64_ed25519_signature"
}
```

6. **Signature:** Sign entire message with identity key (anti-spoofing)

**Reception (Bob receives):**

1. **Accept Connection:** Bob's hidden service accepts incoming TCP connection
2. **Receive Message:** Parse JSON from TCP stream (length-prefixed framing)
3. **Validate:**
   - Verify signature against `from_onion` public key (embedded in .onion)
   - Check timestamp (reject if > 5 minutes old or in future)
   - Verify friend code maps to `from_onion`
4. **Rate Limit:** Check max 5 requests per hour from same .onion
   - Store in memory: `(onion, timestamp)` tuples
   - Reject if rate exceeded
5. **Store:** Insert into `friend_requests` table (status: "pending")
6. **Notify:** Show notification in TUI: "Friend request from `alice2k3...`"

**Approval (Bob accepts):**

1. **User Action:** Bob reviews request in TUI, presses 'a' to accept
2. **Generate PreKey Bundle:** Bob creates Signal Protocol PreKey Bundle
   - Identity key, signed pre-key, one-time pre-keys
3. **Send Response:** Bob sends `FriendRequestAccept` to Alice:

```json
{
  "type": "friend_request_accept",
  "from_onion": "bob7x8y9z1a2b3c4d5e6f7g8h9j1k2m3n4p5q6r7s8t9u1v2w3x.onion",
  "to_onion": "alice2k3b4d5e6f7g8h9j1m2n4p6q8r9s1t3v5w7x9y2a3b5c7d9e.onion",
  "signal_prekey_bundle": {
    "identity_key": "base64_public_key",
    "signed_prekey": {
      "key_id": 1,
      "public_key": "base64_key",
      "signature": "base64_sig"
    },
    "prekeys": [
      {"key_id": 1, "public_key": "base64_key"},
      {"key_id": 2, "public_key": "base64_key"}
    ]
  },
  "timestamp": 1738886460,
  "signature": "base64_signature"
}
```

4. **Store Friend:** Both sides insert into `friends` table (status: "active")
5. **Establish Session:** Alice initializes Signal session using Bob's PreKey Bundle
6. **Ready:** Both parties can now send encrypted messages

**Rejection (Bob rejects):**

1. **User Action:** Bob presses 'r' to reject
2. **Send Response:** Bob sends `FriendRequestReject`:

```json
{
  "type": "friend_request_reject",
  "from_onion": "bob7x8y...",
  "to_onion": "alice2k3b...",
  "timestamp": 1738886460
}
```

3. **Close:** Connection closes, no data stored
4. **Notify Alice:** Alice's UI shows "Friend request rejected by bob7x8y..."

### Rate Limiting & Spam Protection

**Per-Onion Rate Limits:**
- Max 5 friend requests per hour from same .onion
- Tracked in memory (no persistence needed)
- Reset after 1 hour

**Global Rate Limits:**
- Max 20 total pending friend requests at once
- Prevents request flooding

**Blocklist:**
- User can block specific .onion addresses
- Stored in `blocked_onions` table
- Blocked connections rejected immediately

---

## Section 4: Message Protocol & Encryption

### Message Format

**Wire Format (JSON over TCP):**

After Signal Protocol encryption, messages sent as JSON:

```json
{
  "type": "message",
  "from_onion": "alice2k3b4d5e6f7g8h9j1m2n4p6q8r9s1t3v5w7x9y2a3b5c7d9e.onion",
  "to_onion": "bob7x8y9z1a2b3c4d5e6f7g8h9j1k2m3n4p5q6r7s8t9u1v2w3x.onion",
  "signal_ciphertext": "base64_encrypted_payload",
  "signal_type": "prekey_message",  // or "message"
  "timestamp": 1738886500,
  "message_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Plaintext Payload (before encryption):**

```json
{
  "content": "Hello, Bob!",
  "sent_at": 1738886500,
  "message_type": "text"
}
```

**Message Types:**
- `text` - Text message (Phase 2)
- `typing_indicator` - User is typing (future)
- `read_receipt` - Message read confirmation (future)
- `delivery_receipt` - Message delivered confirmation (Phase 2)

### Signal Protocol Integration

**Session Establishment (First Message):**

1. **Alice → Bob (First Message):**
   - Alice retrieves Bob's PreKey Bundle (from friend request accept)
   - Initializes Signal session: `SessionBuilder::new(bob_prekey_bundle)`
   - Encrypts first message as `PreKeySignalMessage`
   - Message includes Alice's identity key and ephemeral key
   - Stores session state in database

2. **Bob Receives:**
   - Bob sees `signal_type: "prekey_message"`
   - Initializes session from PreKey message: `SessionBuilder::process_prekey()`
   - Decrypts message
   - Stores session state

**Subsequent Messages:**

1. **Encrypt:**
   - Load existing Signal session from database
   - Encrypt: `SessionCipher::encrypt(plaintext) → SignalMessage`
   - Signal automatically ratchets keys (Double Ratchet algorithm)
   - Update session state in database

2. **Decrypt:**
   - Receive encrypted message
   - Load Signal session for sender
   - Decrypt: `SessionCipher::decrypt(ciphertext) → plaintext`
   - Verify sender's identity (signature on ciphertext)
   - Update session state
   - Store plaintext in messages table

### Encryption Properties

**Forward Secrecy:**
- Compromising current keys does NOT reveal past messages
- Each message encrypted with unique key
- Old keys deleted after use

**Future Secrecy (Break-in Recovery):**
- Keys rotate with each message exchange
- Compromise of current state does NOT reveal future messages
- Ratchet mechanism generates new keys

**Authentication:**
- Each message cryptographically authenticated
- Recipient verifies sender owns the .onion address
- Prevents impersonation attacks

**Replay Protection:**
- Message IDs (UUIDs) prevent duplicate delivery
- Timestamps detect out-of-order messages
- Session state tracks message counter

### Module Structure

**Files to create:**
- `src/protocol/mod.rs` - Protocol exports
- `src/protocol/message.rs` - Message types, serialization, validation
- `src/crypto/signal.rs` - Signal Protocol wrapper (session management)
- `src/crypto/session.rs` - Session state persistence

**Key Types:**
```rust
pub struct Message {
    pub from_onion: String,
    pub to_onion: String,
    pub signal_ciphertext: Vec<u8>,
    pub signal_type: SignalMessageType,
    pub timestamp: i64,
    pub message_id: Uuid,
}

pub struct SignalSession {
    pub remote_onion: String,
    pub session_cipher: SessionCipher,
    pub state: SessionState,
}
```

---

## Section 5: Message Delivery & Queueing

### Sending Flow

**When Alice sends to Bob (Bob is online):**

1. **Compose:** Alice types message in TUI
2. **Encrypt:** App encrypts with Signal Protocol
3. **Connect:** Look up Bob's active connection or establish new one
4. **Send:** Transmit JSON message over TCP
5. **Receive:** Bob's hidden service receives, decrypts
6. **Store:** Bob inserts into `messages` table
7. **Acknowledge:** Bob sends `DeliveryReceipt` back to Alice

```json
{
  "type": "delivery_receipt",
  "message_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": 1738886505
}
```

8. **Update UI:** Alice's UI updates status:
   - "Composing" → "Encrypting" → "Sent" → "Delivered"

**When Bob is offline:**

1. **Connection Fails:** TCP connection to Bob's .onion times out (~60s)
2. **Queue:** Message stored in Alice's local queue (`message_queue` table)
3. **UI Shows "Queued":** Alice sees message with "Queued" status
4. **Background Retry:** Background task retries delivery every 2-3 minutes
5. **Bob Comes Online:** Alice's next connection attempt succeeds
6. **Deliver Queued:** All queued messages sent in FIFO order
7. **Update Status:** Status changes from "Queued" → "Delivered"

### Queue Management

**Database Schema (addition to Phase 1 schema):**

```sql
-- Message queue for offline delivery
CREATE TABLE message_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    to_onion TEXT NOT NULL,
    conversation_id INTEGER NOT NULL,
    encrypted_message BLOB NOT NULL,  -- Full JSON message, already encrypted
    created_at INTEGER NOT NULL,
    retry_count INTEGER DEFAULT 0,
    last_retry_at INTEGER,
    max_retries INTEGER DEFAULT 50,   -- ~2.5 hours of retries (50 * 3min)
    FOREIGN KEY (conversation_id) REFERENCES conversations(id)
);

CREATE INDEX idx_queue_to_onion ON message_queue(to_onion);
CREATE INDEX idx_queue_retry ON message_queue(retry_count, last_retry_at);
```

**Queue Processing:**

**Background Task (async):**
```rust
async fn process_message_queue(db: &Database, tor: &TorService) {
    loop {
        tokio::time::sleep(Duration::from_secs(180)).await; // 3 minutes

        let queued = db.get_queued_messages().await?;

        for msg in queued {
            if msg.retry_count >= msg.max_retries {
                db.mark_message_failed(msg.id).await?;
                continue;
            }

            match tor.send_message(&msg.to_onion, &msg.encrypted_message).await {
                Ok(_) => {
                    db.remove_from_queue(msg.id).await?;
                    db.update_message_status(msg.conversation_id, "delivered").await?;
                }
                Err(_) => {
                    db.increment_retry_count(msg.id).await?;
                }
            }
        }
    }
}
```

**Retry Strategy:**
- Constant interval: Every 3 minutes
- Max retries: 50 (configurable)
- Total retry window: ~2.5 hours
- After max retries: Mark as "Failed" in UI
- User can manually retry failed messages

### Message Status States

**Status Progression:**
1. **Composing** - User typing in input box
2. **Encrypting** - Signal Protocol encryption in progress
3. **Queued** - Waiting for delivery (recipient offline or connection failed)
4. **Sent** - Successfully transmitted over Tor
5. **Delivered** - Recipient's hidden service confirmed receipt (DeliveryReceipt)
6. **Failed** - Max retries exceeded or permanent error

**UI Indicators:**
- Composing: Input cursor visible
- Encrypting: Brief spinner (usually <100ms)
- Queued: Clock icon
- Sent: Single checkmark ✓
- Delivered: Double checkmark ✓✓
- Failed: Red X with "Retry" button

### Delivery Guarantees

**At-Least-Once Delivery:**
- Messages may be delivered more than once (network failures, retries)
- Deduplication using `message_id` (UUID)
- Recipient tracks seen message IDs to prevent duplicates

**Order Preservation:**
- FIFO queue per conversation
- Messages delivered in order sent
- Out-of-order detection via timestamp + message counter

**Persistence:**
- Queue persisted in SQLCipher database
- Survives app crashes and restarts
- No message loss due to crashes

**Failure Handling:**
- Network errors: Retry with exponential backoff
- Tor circuit failures: Establish new circuit
- Recipient offline: Queue locally
- Permanent failures: Mark failed, allow manual retry

---

## Section 6: UI Integration

### Updated TUI Layout

```
┌─────────────────────────────────────────────────────────────┐
│  torrent-chat  ⬤ Online  [@alice2k3...]  [Tor: Connected]  │
├──────────────┬──────────────────────────────────────────────┤
│              │                                              │
│ Friends (3)  │         Active Conversation: Bob            │
│  ⬤ Bob (2)   │  ┌────────────────────────────────────────┐ │
│  ⬤ Carol     │  │ Bob: Hey!                   [Delivered] │ │
│  ○ Dave      │  │ You: Hi Bob!                [Delivered] │ │
│              │  │ Bob: How are you?           [Delivered] │ │
│ Requests (1) │  │ You: Good!                     [Queued] │ │
│  📬 Eve      │  └────────────────────────────────────────┘ │
│              │                                              │
│ [/] Search   │                                              │
├──────────────┼──────────────────────────────────────────────┤
│ [Tab] Nav    │  [Type message...] [Enter] Send [Esc] Quit │
└──────────────┴──────────────────────────────────────────────┘
```

### Key UI Components

**Header Bar:**
- App name and version
- Connection status: ⬤ Online / ⬤ Connecting / ○ Offline
- Your .onion address (truncated): `[@alice2k3...]`
- Tor status: `[Tor: Connected]` / `[Tor: Bootstrapping 45%]` / `[Tor: Error]`

**Sidebar (Left Panel):**

**Friends List:**
- Online status: ⬤ green (online), ○ gray (offline)
- Unread count badge: `Bob (2)` = 2 unread messages
- Display name or .onion (first 8 chars)
- Sorted: Online friends first, then offline

**Friend Requests:**
- Separate section below friends
- Shows count: `Requests (1)`
- Expand to see pending requests

**Search:**
- Shortcut hint: `[/] Search`
- Triggers global message search modal

**Conversation View (Main Panel):**

**Message Bubbles:**
- Sent messages: Right-aligned, cyan background
- Received messages: Left-aligned, gray background
- Format: `Sender: Content [Status]`
- Timestamps on hover (relative: "2m ago" or absolute: "14:32")

**Delivery Status Indicators:**
- Queued: 🕐 clock icon
- Sent: ✓ single check
- Delivered: ✓✓ double check
- Failed: ❌ red X

**Auto-Scroll:**
- Automatically scroll to latest message on new message
- User can scroll up to view history
- "Jump to bottom" button appears when scrolled up

**Visual Distinction:**
- Alternate background colors for sent vs received
- Sender name in bold
- Timestamp in gray, smaller font

### Modal Dialogs

**Friend Request Modal:**
```
┌─────────────────────────────────────────────────────────┐
│              Friend Request from eve4x2y...             │
├─────────────────────────────────────────────────────────┤
│  Friend code: flame-8392-solar-1647                     │
│  Received: 2 minutes ago                                │
│                                                         │
│  This person wants to connect with you.                │
│                                                         │
│  [A]ccept        [R]eject        [Esc] Back            │
└─────────────────────────────────────────────────────────┘
```

**Add Friend Modal:**
```
┌─────────────────────────────────────────────────────────┐
│                    Add New Friend                       │
├─────────────────────────────────────────────────────────┤
│  Enter friend code:                                     │
│  ┌───────────────────────────────────────────────────┐ │
│  │ happy-1234-tiger-5678_                            │ │
│  └───────────────────────────────────────────────────┘ │
│                                                         │
│  Friend codes are 4 words/numbers like:                │
│  word-NNNN-word-NNNN                                   │
│                                                         │
│  [Enter] Send Request        [Esc] Cancel              │
└─────────────────────────────────────────────────────────┘
```

**Search Modal:**
```
┌─────────────────────────────────────────────────────────┐
│  Search: "hello"___               [3 results]           │
│  [Tab] All conversations | Current conversation         │
├─────────────────────────────────────────────────────────┤
│  ▶ Bob - 2 hours ago                                    │
│    "Hello! How are you doing?"                          │
│                                                         │
│    Carol - 1 day ago                                    │
│    "Hello there"                                        │
│                                                         │
│    Dave - 3 days ago                                    │
│    "Say hello to everyone for me"                       │
│                                                         │
│  [↑↓] Navigate  [Enter] Jump to message  [Esc] Close   │
└─────────────────────────────────────────────────────────┘
```

### Message Search

**Search Functionality:**
- Trigger: `/` or `Ctrl+F` from any view
- Full-text search using SQLite FTS5 (Full-Text Search)
- Search scope toggle (Tab key):
  - All conversations (default)
  - Current conversation only
- Results show: friend name, timestamp, message preview (50 chars)
- Navigate: Arrow keys move through results
- Action: Enter jumps to conversation and highlights message
- Close: Esc returns to previous view

**Database Schema for Search:**
```sql
-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE messages_fts USING fts5(
    content,
    sender_onion,
    conversation_id,
    content='messages',
    content_rowid='id'
);

-- Triggers to keep FTS index in sync
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content, sender_onion, conversation_id)
    VALUES (new.id, new.content, new.sender_onion, new.conversation_id);
END;

CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
END;

CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
    INSERT INTO messages_fts(rowid, content, sender_onion, conversation_id)
    VALUES (new.id, new.content, new.sender_onion, new.conversation_id);
END;
```

**Search Query:**
```sql
SELECT
    m.id, m.content, m.sender_onion, m.conversation_id, m.timestamp,
    f.display_name
FROM messages_fts fts
JOIN messages m ON fts.rowid = m.id
JOIN conversations c ON m.conversation_id = c.id
JOIN friends f ON c.friend_id = f.id
WHERE messages_fts MATCH ?
ORDER BY m.timestamp DESC
LIMIT 50;
```

### Status Notifications

**Toast Notifications (Bottom-Right):**
- Friend requests: "New friend request from eve4x2y..."
- New messages: "New message from Bob" (when not viewing that conversation)
- Connection issues: "Tor connection lost - reconnecting..."
- Delivery failures: "Message to Dave failed - queued for retry"

**Toast Behavior:**
- Auto-dismiss after 5 seconds
- Stack up to 3 toasts (oldest auto-dismissed)
- Click to dismiss early
- Non-intrusive, semi-transparent background

**System Tray Notifications (Optional, Future):**
- OS-specific desktop notifications
- Configurable in settings (on/off)
- Privacy mode: "New message" without sender name/content

### Keyboard Shortcuts

**Global:**
- `Tab` / `Shift+Tab` - Navigate between friends in sidebar
- `/` or `Ctrl+F` - Open search modal
- `n` - Add new friend (open modal)
- `q` or `Ctrl+C` - Quit application
- `Esc` - Close modal / Go back

**In Conversation:**
- `Enter` - Send message
- `↑` / `↓` - Scroll message history
- `Home` / `End` - Jump to oldest/newest message

**In Friend Request:**
- `a` - Accept friend request
- `r` - Reject friend request

**In Search:**
- `Tab` - Toggle search scope (all / current conversation)
- `↑` / `↓` - Navigate results
- `Enter` - Jump to selected message
- `Esc` - Close search

### Visual Theme (Consistent with Phase 1)

**Colors:**
- Background: Deep blue-black gradient (`#0a0e27` → `#1a1e3a`)
- Primary accent: Cyan/teal (`#00d4ff`)
- Text: Soft white (`#e0e0e0`)
- Online indicator: Green (`#00ff88`)
- Offline indicator: Gray (`#666666`)
- Delivered status: Cyan checkmarks
- Queued status: Amber clock icon
- Failed status: Red X

**Typography:**
- Unicode box-drawing characters for borders
- Monospace font (terminal default)
- Bold for sender names
- Smaller font for timestamps and status

---

## Section 7: Testing Strategy

### Unit Tests

**Tor Module (`src/tor/`):**

- **Hidden Service Initialization:**
  - Test hidden service starts successfully with mock arti client
  - Test .onion address derived correctly from identity key
  - Test error handling when Tor bootstrap fails

- **Address Mapping:**
  - Test friend code → .onion conversion
  - Test .onion → friend code generation
  - Test checksum validation detects errors
  - Test round-trip: friend code → .onion → friend code

- **Connection Management:**
  - Test connection establishment (mock TCP streams)
  - Test connection teardown (graceful close)
  - Test concurrent connections (multiple friends)
  - Test connection failure handling (retry logic)

**Protocol Module (`src/protocol/`):**

- **Message Serialization:**
  - Test JSON serialization/deserialization
  - Test all message types (friend_request, message, receipt)
  - Test invalid JSON handling
  - Test length-prefixed framing

- **Message Validation:**
  - Test signature verification (valid and invalid signatures)
  - Test timestamp validation (reject old or future)
  - Test message ID uniqueness
  - Test required fields present

- **Friend Request:**
  - Test friend request format
  - Test friend request accept/reject format
  - Test rate limiting logic

**Crypto Module (`src/crypto/`):**

- **Signal Protocol:**
  - Test session establishment (PreKey exchange)
  - Test encryption/decryption round-trip
  - Test multiple messages (key ratcheting)
  - Test session state persistence
  - Mock Signal Protocol for fast tests

- **Session Management:**
  - Test session creation and storage
  - Test session loading from database
  - Test session state updates after each message
  - Test concurrent session access (thread safety)

**Queue Module (`src/net/delivery.rs`):**

- **Queue Operations:**
  - Test enqueue message
  - Test dequeue message (FIFO order)
  - Test queue persistence (survives restart)
  - Test retry count increment

- **Retry Logic:**
  - Mock time progression
  - Test retry after 3 minutes
  - Test max retry limit (50 attempts)
  - Test exponential backoff (optional)

- **Queue Processing:**
  - Test delivery success removes from queue
  - Test delivery failure increments retry
  - Test max retries marks as failed

### Integration Tests

**Two-Instance Local Tests:**

Set up two app instances on same machine with different configs:

**Test Setup:**
```rust
async fn setup_test_instances() -> (App, App) {
    let alice = App::new_with_config(TestConfig {
        data_dir: "/tmp/alice",
        tor_port: 9051,
        listen_port: 8001,
    }).await?;

    let bob = App::new_with_config(TestConfig {
        data_dir: "/tmp/bob",
        tor_port: 9052,
        listen_port: 8002,
    }).await?;

    (alice, bob)
}
```

**Test Scenarios:**

1. **Friend Request Flow:**
   - Alice sends friend request to Bob
   - Bob receives notification
   - Bob accepts request
   - Both sides have friend record
   - Signal sessions established

2. **Message Send/Receive:**
   - Alice sends "Hello" to Bob
   - Bob receives and decrypts message
   - Bob sends reply
   - Alice receives reply
   - Verify message content correct

3. **Offline Queueing:**
   - Alice sends message while Bob offline (stop Bob's instance)
   - Verify message queued in Alice's database
   - Start Bob's instance
   - Verify message delivered after reconnect
   - Verify status updates (Queued → Delivered)

4. **Multiple Messages:**
   - Send 10 messages in rapid succession
   - Verify all delivered in correct order
   - Verify Signal Protocol key ratcheting

5. **Concurrent Conversations:**
   - Alice talks to Bob and Carol simultaneously
   - Verify no message mixups
   - Verify separate Signal sessions

6. **Search Functionality:**
   - Insert test messages into database
   - Search for keyword
   - Verify correct results returned
   - Verify search in all vs current conversation

### Tor Network Tests

**Local Tor Network (Chutney):**

Use `chutney` (Tor testing framework) to spin up local Tor network for testing:

```bash
# Clone chutney
git clone https://git.torproject.org/chutney.git

# Start local Tor network (3 relays, 3 authorities)
cd chutney
./chutney configure networks/basic
./chutney start networks/basic
```

**Tests:**
- Test hidden service descriptor publishing
- Test circuit establishment timing (measure latency)
- Test connection through local Tor network
- Test multiple simultaneous connections
- Test circuit failure recovery

### End-to-End Test Scenarios

**Scenario 1: Happy Path**
1. Alice and Bob both start app (fresh installs)
2. Tor bootstraps successfully (~60s)
3. Alice adds Bob via friend code
4. Bob accepts friend request
5. Alice sends "Hello, Bob!"
6. Bob receives instantly (within 1-2 seconds)
7. Bob replies "Hi, Alice!"
8. Alice receives reply
9. Both see "Delivered" status

**Scenario 2: Offline Delivery**
1. Alice sends message to Bob
2. Bob is offline (app not running)
3. Message shows "Queued" in Alice's UI
4. Wait 3 minutes, verify retry attempt
5. Bob starts app and comes online
6. Message delivers successfully
7. Status updates to "Delivered"
8. Bob sees message in conversation

**Scenario 3: Friend Request Rejection**
1. Alice sends friend request to Bob
2. Bob receives notification
3. Bob rejects request
4. Alice receives rejection notification
5. Verify no friend records created
6. Verify connection closed

**Scenario 4: Message Search**
1. Alice and Bob exchange 20 messages over time
2. Alice searches for keyword "hello"
3. Verify search returns matching messages
4. Alice toggles search scope (all vs current)
5. Verify scope filtering works
6. Alice jumps to message from search
7. Verify conversation scrolls to that message

**Scenario 5: Connection Recovery**
1. Alice and Bob chatting actively
2. Simulate network interruption (kill Tor process)
3. Message shows "Queued"
4. Tor reconnects automatically
5. Queued messages deliver
6. Verify conversation continues seamlessly

**Scenario 6: Concurrent Messages**
1. Alice and Bob both send messages simultaneously
2. Verify both messages delivered
3. Verify correct ordering (by timestamp)
4. Verify no message loss or corruption

### Test Utilities

**Mock Tor Client:**
```rust
struct MockTorClient {
    connections: HashMap<String, MockStream>,
    fail_next: bool,  // Simulate failures
}

impl MockTorClient {
    fn connect(&mut self, onion: &str) -> Result<MockStream> {
        if self.fail_next {
            self.fail_next = false;
            return Err(Error::ConnectionFailed);
        }
        Ok(MockStream::new())
    }
}
```

**Test Fixtures:**
- Pre-generated Signal Protocol sessions
- Sample messages (friend requests, text messages)
- Sample friend codes and .onion addresses
- Database seeding scripts (conversation history)

**Helper Functions:**
```rust
async fn send_test_message(app: &App, to: &str, content: &str) -> Result<MessageId>;
async fn wait_for_message(app: &App, from: &str, timeout: Duration) -> Result<Message>;
async fn assert_message_delivered(app: &App, msg_id: MessageId, timeout: Duration);
```

### Manual Testing

**Usability Testing:**
- TUI keyboard navigation (Tab, Enter, Esc)
- Message composition and editing
- Friend request approval flow
- Search modal interaction
- Copy-paste support (friend codes, messages)

**Performance Testing:**
- Message send latency (measure Tor circuit time)
- UI responsiveness during Tor bootstrap
- Memory usage over 1-hour session
- CPU usage (idle vs active messaging)
- Database query performance (1000+ messages)

**Stability Testing:**
- Multi-hour run (check for memory leaks)
- Repeated connect/disconnect cycles
- Rapid message sending (stress test)
- Large message content (test limits)
- Many queued messages (50+ in queue)

**Cross-Platform Testing:**
- Test on Linux (Ubuntu, Arch)
- Test on macOS (Intel and ARM)
- Test on BSD (if possible)
- Verify platform-specific paths work

### Continuous Integration

**GitHub Actions Workflow:**
```yaml
name: Phase 2 Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run unit tests
        run: cargo test --lib
      - name: Run integration tests
        run: cargo test --test '*'
      - name: Check formatting
        run: cargo fmt -- --check
      - name: Run clippy
        run: cargo clippy -- -D warnings
```

**Test Coverage Goals:**
- Unit tests: 80%+ code coverage
- Integration tests: All critical paths
- Manual testing: Before each release

---

## Implementation Plan

### Module Dependencies

**Phase 2 builds on Phase 1:**
- Uses existing: `error`, `db`, `crypto` (identity), `protocol` (friend_code), `config`, `ui`
- Extends: `crypto` (add Signal), `protocol` (add messages), `db` (add queue)
- New: `tor`, `net`

**Implementation Order:**

1. **Tor Integration** (Week 1)
   - `src/tor/client.rs` - Arti wrapper
   - `src/tor/hidden_service.rs` - Hidden service lifecycle
   - `src/tor/connection.rs` - Connection handling
   - `src/tor/address.rs` - Friend code ↔ .onion mapping

2. **Message Protocol** (Week 1-2)
   - `src/protocol/message.rs` - Message types and serialization
   - `src/crypto/signal.rs` - Signal Protocol wrapper
   - `src/crypto/session.rs` - Session persistence

3. **Friend Requests** (Week 2)
   - Friend request message types
   - Request handling logic
   - Database persistence

4. **Message Delivery** (Week 2-3)
   - Send/receive logic
   - Queue management
   - Retry mechanism
   - Delivery receipts

5. **UI Integration** (Week 3-4)
   - Update conversation view
   - Add status indicators
   - Friend request modal
   - Search functionality

6. **Testing & Polish** (Week 4)
   - Write integration tests
   - Manual testing
   - Bug fixes
   - Documentation

### Next Steps

1. **Ready to set up for implementation?**
   - Use `superpowers:using-git-worktrees` to create isolated workspace
   - Use `superpowers:writing-plans` to create detailed implementation plan

2. **Review dependencies:**
   - Add `libsignal-protocol-rust` to Cargo.toml
   - Verify `arti` version supports hidden services
   - Add FTS5 support to SQLite

3. **Create issues/tasks:**
   - Break down implementation into bite-sized tasks
   - Assign priorities
   - Track progress

---

## Appendix: Open Questions

**For Implementation Planning:**

1. **Signal Protocol Library:** Verify `libsignal-protocol-rust` is maintained and compatible with latest Rust
2. **Arti Hidden Service API:** Confirm arti 2.0 supports hidden service creation (not just client)
3. **FTS5 in SQLCipher:** Verify FTS5 extension works with bundled SQLCipher
4. **Connection Pooling:** Should we reuse TCP connections or create new per-message?
5. **Error Recovery:** How to handle corrupted Signal session state?

**For Future Phases:**

1. **Group Chats:** Phase 3 or later?
2. **File Sharing:** Phase 3 or defer?
3. **Read Receipts:** Include in Phase 2 or Phase 3?
4. **Typing Indicators:** Phase 2 or Phase 3?
5. **Broadcast Channels:** Still Phase 3?

---

**Document Status:** Approved and ready for implementation planning
**Next Action:** Create Phase 2 implementation plan with detailed task breakdown
