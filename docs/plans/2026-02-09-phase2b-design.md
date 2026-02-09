# Phase 2b: Real Tor + Signal Protocol - Design Document

**Date:** 2026-02-09
**Status:** Approved
**Phase:** 2b (Implementation of Phase 2 stubs)

## Overview

Phase 2b replaces the stub implementations from Phase 2 with real working code. The goal is to make torrent-chat actually work - two instances on the same machine can send end-to-end encrypted messages over real Tor hidden services.

**Deliverable:** MVP with functional local testing - real Tor, real encryption, real P2P messaging.

---

## Section 1: Architecture Overview

### Goal

Make torrent-chat actually work - two instances on the same machine can send end-to-end encrypted messages over real Tor hidden services.

### Core Components

**Tor Layer (via arti):**
- `TorClient` bootstraps to Tor network (30-60 second startup)
- `HiddenService` hosts .onion address, accepts incoming connections
- `TorConnection` manages outgoing connections with pooling (5-minute idle timeout)
- Ed25519 identity key persisted in database → stable .onion address

**Encryption Layer (libsignal-dezire):**
- Full Signal Protocol with X3DH key agreement
- PreKey bundles generated and exchanged during friend requests
- Double Ratchet for forward/future secrecy
- Session state persisted in `signal_sessions` table

**Network Layer:**
- Dedicated listener task accepts incoming TCP connections on hidden service
- Length-prefixed framing: `[u32 length][JSON payload]`
- Connection pooling: reuse connections for 5 minutes
- Message queue background task retries failed deliveries every 3 minutes

**UI Layer:**
- Progress bar during Tor bootstrap (fun and vibey!)
- Modal dialogs for friend requests (add/accept/reject)
- Message status icons: 🕐 Queued, ✓ Sent, ✓✓ Delivered, ❌ Failed
- Graceful degradation: errors don't crash app

**Testing:** Two instances via `--config-dir` override, automated integration tests plus manual verification.

---

## Section 2: Tor Integration Details

### Initialization Flow

**1. App Startup:**
- `App::new()` creates app state synchronously
- UI shows immediately

**2. Tor Bootstrap (`App::init_tor()` async):**
- Create arti `TorClient` with default config
- Show progress bar: "🔄 Connecting to Tor... 0%"
- Bootstrap connection (arti emits progress events)
- Update progress with fun messages:
  - "Building circuits... 25%"
  - "Finding relays... 45%"
  - "Establishing connection... 75%"
  - "Almost there... 95%"
- Store `Arc<TorClient>` in app state

**3. Hidden Service Creation:**
- Load Ed25519 identity key from database (or generate on first run)
- Derive .onion address from identity key (v3 format, 56 characters)
- Create `HiddenService` via arti on local port (e.g., 9051)
- Store .onion address in app state
- Spawn listener task for incoming connections
- Update UI: "Tor: Connected" with .onion address

### Connection Management

**Outgoing Connections (TorConnection):**

**Connection Pool:**
```rust
struct ConnectionPool {
    connections: HashMap<String, (TcpStream, Instant)>,
    idle_timeout: Duration, // 5 minutes
}
```

**Send Flow:**
1. Check pool for existing connection to target .onion
2. If found and age < 5 minutes: reuse connection
3. Else: create new via `TorClient::connect(onion_address)`
4. Write length-prefixed JSON message
5. Store connection in pool with current timestamp
6. Lazy cleanup: remove expired connections on access

**Error Handling:**
- Circuit failure → return error, caller queues message
- Connection timeout → return error, retry via background task
- Write failure → close connection, remove from pool
- No panics, all errors propagate gracefully

**Incoming Connections (Listener Task):**

**Listener Loop:**
```rust
async fn listen_for_connections(
    hidden_service: HiddenService,
    app_tx: mpsc::Sender<IncomingMessage>,
) {
    let listener = hidden_service.listen().await?;

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        tokio::spawn(handle_connection(stream, app_tx.clone()));
    }
}
```

**Connection Handler:**
1. Read 4 bytes → message length (u32 big-endian)
2. Read `length` bytes → JSON payload
3. Deserialize to `Message` enum
4. Send to app via channel
5. Close connection

---

## Section 3: Signal Protocol Integration

### PreKey Bundle Generation

**On App First Start (or when needed):**

1. Generate Signal identity key pair
   - Store in database: `settings` table, key="signal_identity"
   - Used for all Signal sessions

2. Generate signed PreKey
   - Rotated periodically (every 30 days)
   - Signed with identity key

3. Generate one-time PreKeys (pool of 10)
   - Consumed on session initialization
   - Regenerated when pool depletes

4. Serialize as `PreKeyBundle`:
```rust
struct PreKeyBundle {
    identity_key: PublicKey,
    signed_prekey: SignedPreKey,
    prekey: Option<PreKey>, // one-time key
}
```

### X3DH Key Agreement (Friend Request Flow)

**Alice initiates friend request to Bob:**

1. **Alice sends `FriendRequest`:**
   - Contains: Alice's .onion, friend code, timestamp, signature
   - Sent over Tor to Bob's hidden service

2. **Bob receives request:**
   - Validates signature using Alice's .onion public key
   - Stores in `friend_requests` table
   - Generates PreKey bundle for Alice
   - Sends `FriendRequestAccept` with PreKey bundle

3. **Alice receives accept with PreKey bundle:**
   - Initializes Signal session using X3DH:
     - Uses Bob's identity key, signed prekey, one-time prekey
     - Performs Diffie-Hellman calculations
     - Derives shared secret and ratchet keys
   - Stores session in `signal_sessions` table
   - Inserts Bob into `friends` table

4. **Alice sends first message (PreKeySignalMessage):**
   - Includes Alice's identity key and ephemeral key
   - Encrypted with initial chain key

5. **Bob receives PreKeySignalMessage:**
   - Initializes his session from PreKey message
   - Derives same shared secret
   - Decrypts message
   - Stores session in `signal_sessions` table

6. **Both now have established sessions:**
   - Can send regular `SignalMessage` (no PreKey needed)
   - Double Ratchet maintains forward/future secrecy

### Encryption/Decryption Flow

**Sending a Message:**

1. Load `SignalSession` from database (by remote_onion)
2. Serialize `PlaintextPayload` to JSON:
   ```rust
   struct PlaintextPayload {
       content: String,
       sent_at: i64,
       message_type: String, // "text"
   }
   ```
3. Encrypt with `session.encrypt(plaintext)` → `(ciphertext, signal_type)`
4. Create `TextMessage` envelope:
   ```rust
   struct TextMessage {
       from_onion: String,
       to_onion: String,
       signal_ciphertext: Vec<u8>,
       signal_type: SignalMessageType, // PreKeyMessage or Message
       timestamp: i64,
       message_id: Uuid,
   }
   ```
5. Send over Tor connection
6. Update session state in database (ratchet advances)

**Receiving a Message:**

1. Receive `TextMessage` from peer
2. Load `SignalSession` from database (by from_onion)
3. Decrypt with `session.decrypt(ciphertext)` → plaintext bytes
4. Deserialize JSON to `PlaintextPayload`
5. Insert into `messages` table
6. Update session state in database
7. Send `DeliveryReceipt` back to sender

**Session State Persistence:**
- After every encrypt/decrypt: serialize session state to BLOB
- Store in `signal_sessions` table keyed by remote_onion
- On load: deserialize from BLOB, reconstruct session

---

## Section 4: Message Flow & Delivery

### Complete Message Journey

**1. User Composes Message:**
- User types in TUI input field
- Presses Enter to send
- App creates `PlaintextPayload` with content and timestamp

**2. Encryption:**
- Look up conversation → get friend's .onion address
- Load `SignalSession` for that friend (or error if no session)
- Encrypt payload → get ciphertext + signal message type
- Create `TextMessage` envelope with metadata
- Generate UUID for message_id (deduplication)

**3. Sending:**
- Check connection pool for existing connection to peer's .onion
- If found and fresh (< 5 min): reuse connection
- Else: create new via `TorClient::connect(peer_onion)`
- Write length-prefixed JSON:
  ```
  [u32: message_length][JSON: TextMessage]
  ```
- Insert message into `messages` table with status="sent"
- Update UI: show ✓ Sent icon (gray)

**4. If Send Fails:**
- Catch error (circuit failure, timeout, connection refused)
- Insert into `message_queue` table:
  ```sql
  INSERT INTO message_queue (
      to_onion, conversation_id, encrypted_message,
      message_uuid, retry_count, max_retries
  ) VALUES (?, ?, ?, ?, 0, 50);
  ```
- Update UI: show 🕐 Queued icon (amber)
- Background task will retry

**5. Peer Receives:**
- Listener task accepts TCP connection
- Reads length-prefixed JSON
- Deserializes to `TextMessage`
- Loads Signal session for sender
- Decrypts with `session.decrypt()`
- Inserts into local `messages` table
- Sends `DeliveryReceipt` back:
  ```rust
  DeliveryReceiptMessage {
      message_id: received_message.message_id,
      timestamp: now(),
  }
  ```

**6. Receipt Received:**
- Sender receives `DeliveryReceipt`
- Looks up message by message_id
- Updates status from "sent" to "delivered"
- Updates UI: show ✓✓ Delivered icon (cyan)

### Background Queue Processor

**Tokio Task:**
```rust
async fn process_message_queue(
    db: Arc<Database>,
    tor_client: Arc<TorClient>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(180)).await; // 3 minutes

        let queued = db.get_all_queued_messages().await?;

        for msg in queued {
            if msg.retry_count >= msg.max_retries {
                db.mark_message_failed(msg.id).await?;
                continue;
            }

            match try_send_message(&tor_client, &msg).await {
                Ok(_) => {
                    db.remove_from_queue(msg.id).await?;
                    db.update_message_status(msg.message_uuid, "delivered").await?;
                }
                Err(_) => {
                    db.increment_retry_count(msg.id).await?;
                }
            }
        }
    }
}
```

**Behavior:**
- Wakes every 3 minutes
- Queries all messages in `message_queue`
- For each: attempt to send over Tor
- Success: remove from queue, update message status
- Failure: increment retry_count
- Max retries (50) exceeded: mark message as "failed", update UI with ❌ icon

---

## Section 5: Friend Request Flow

### UI Interaction

**Adding a Friend (Alice):**

1. User presses `n` key in TUI
2. "Add Friend" modal appears:
   ```
   ┌─────────────────────────────────────────┐
   │          Add New Friend                 │
   ├─────────────────────────────────────────┤
   │  Enter friend code:                     │
   │  ┌───────────────────────────────────┐  │
   │  │ happy-1234-tiger-5678_           │  │
   │  └───────────────────────────────────┘  │
   │                                         │
   │  Friend codes are 4 words/numbers      │
   │  Format: word-NNNN-word-NNNN           │
   │                                         │
   │  [Enter] Send    [Esc] Cancel          │
   └─────────────────────────────────────────┘
   ```

3. User types friend code, presses Enter
4. App validates format and checksum
5. Maps friend code → .onion address using deterministic hash
6. Modal shows: "Sending request to `bob7x8y...onion`"
7. Sends `FriendRequest` over Tor
8. Modal closes, friend appears in sidebar with status "Pending"

**Receiving Request (Bob):**

1. Listener task receives `FriendRequest` message:
   ```rust
   FriendRequestMessage {
       from_onion: "alice2k3b4d5e6f7...",
       from_friendcode: "happy-1234-tiger-5678",
       timestamp: 1738886400,
       signature: "base64_signature",
   }
   ```

2. Validates:
   - Signature using sender's .onion public key (embedded in v3 onion)
   - Timestamp (reject if > 5 minutes old)
   - Friend code maps to from_onion

3. Stores in `friend_requests` table (status="pending")

4. Notification appears: "📬 Friend request from `alice2k3...`"

5. Press Enter → modal shows:
   ```
   ┌─────────────────────────────────────────┐
   │   Friend Request from alice2k3b4d5...   │
   ├─────────────────────────────────────────┤
   │  Friend code: happy-1234-tiger-5678     │
   │  Received: 2 minutes ago                │
   │                                         │
   │  This person wants to connect with you. │
   │                                         │
   │  [A]ccept    [R]eject    [Esc] Back    │
   └─────────────────────────────────────────┘
   ```

**Accept Flow:**

1. Bob presses `a` to accept
2. App generates PreKey bundle:
   - Load Signal identity key
   - Generate signed prekey
   - Select one-time prekey from pool
   - Serialize to JSON

3. Sends `FriendRequestAccept` with PreKey bundle:
   ```rust
   FriendRequestAcceptMessage {
       from_onion: "bob7x8y...",
       to_onion: "alice2k3...",
       signal_prekey_bundle: {
           identity_key: "base64...",
           signed_prekey: { ... },
           prekey: { ... },
       },
       timestamp: now(),
       signature: "base64...",
   }
   ```

4. Inserts into `friends` table (status="active")
5. Friend appears in sidebar with ⬤ online indicator

**Alice Receives Accept:**

1. Receives `FriendRequestAccept` with Bob's PreKey bundle
2. Initializes Signal session using X3DH:
   - Performs key agreement with PreKey bundle
   - Derives shared secret
   - Initializes Double Ratchet
3. Stores session in `signal_sessions` table
4. Updates friend status from "pending" to "active"
5. Can now send encrypted messages

**Reject Flow:**

1. Bob presses `r` to reject
2. Sends `FriendRequestReject`:
   ```rust
   FriendRequestRejectMessage {
       from_onion: "bob7x8y...",
       to_onion: "alice2k3...",
       timestamp: now(),
   }
   ```
3. No data stored, connection closed
4. Alice receives rejection
5. Notification: "Friend request rejected by bob7x8y..."
6. Friend removed from sidebar

---

## Section 6: Testing Strategy & Implementation Order

### Testing Setup

**Two-Instance Script (`scripts/test-two-instances.sh`):**
```bash
#!/bin/bash
set -e

# Clean up old test data
rm -rf /tmp/alice /tmp/bob

# Terminal 1: Alice
echo "Starting Alice..."
cargo run -- --config-dir /tmp/alice --debug &
ALICE_PID=$!

# Terminal 2: Bob
echo "Starting Bob..."
cargo run -- --config-dir /tmp/bob --debug &
BOB_PID=$!

# Wait for user to test
echo "Both instances running. Press Ctrl+C to stop."
wait

# Cleanup
kill $ALICE_PID $BOB_PID 2>/dev/null || true
```

**Integration Tests (`tests/integration/e2e_messaging.rs`):**

```rust
#[tokio::test]
async fn test_full_message_flow() {
    // Setup two instances
    let alice = spawn_instance("/tmp/test-alice").await?;
    let bob = spawn_instance("/tmp/test-bob").await?;

    // Wait for Tor bootstrap
    alice.wait_for_tor().await?;
    bob.wait_for_tor().await?;

    // Alice gets friend code from Bob
    let bob_friend_code = bob.get_friend_code()?;

    // Alice sends friend request
    alice.send_friend_request(&bob_friend_code).await?;

    // Bob receives and accepts
    let request = bob.get_pending_requests().await?;
    assert_eq!(request.len(), 1);
    bob.accept_friend_request(request[0].id).await?;

    // Alice receives accept and initializes session
    alice.wait_for_friend_active("bob").await?;

    // Alice sends message
    let msg_id = alice.send_message("bob", "Hello, Bob!").await?;

    // Bob receives message
    let received = bob.wait_for_message(Duration::from_secs(10)).await?;
    assert_eq!(received.content, "Hello, Bob!");

    // Alice receives delivery receipt
    let status = alice.get_message_status(msg_id).await?;
    assert_eq!(status, "delivered");

    // Cleanup
    alice.stop().await?;
    bob.stop().await?;
}

#[tokio::test]
async fn test_offline_queueing() {
    let alice = spawn_instance("/tmp/test-alice").await?;
    let bob = spawn_instance("/tmp/test-bob").await?;

    // Setup friendship
    setup_friendship(&alice, &bob).await?;

    // Stop Bob
    bob.stop().await?;

    // Alice sends message (should queue)
    let msg_id = alice.send_message("bob", "Offline message").await?;
    let status = alice.get_message_status(msg_id).await?;
    assert_eq!(status, "queued");

    // Start Bob
    let bob = spawn_instance("/tmp/test-bob").await?;
    bob.wait_for_tor().await?;

    // Wait for delivery (background task retries)
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify delivery
    let received = bob.get_messages().await?;
    assert!(received.iter().any(|m| m.content == "Offline message"));

    let status = alice.get_message_status(msg_id).await?;
    assert_eq!(status, "delivered");
}
```

### Manual Testing Checklist

- [ ] Both instances bootstrap to Tor successfully (progress bar shows)
- [ ] Friend codes generated and display correctly
- [ ] Friend request sent from Alice to Bob
- [ ] Bob receives notification and can view request
- [ ] Bob accepts request, PreKey bundle exchanged
- [ ] Alice receives accept, Signal session initialized
- [ ] Alice sends message "Hello, Bob!"
- [ ] Bob receives and decrypts message correctly
- [ ] Bob's UI shows message in conversation
- [ ] Alice receives delivery receipt
- [ ] Alice's UI updates to ✓✓ Delivered
- [ ] Stop Bob, send message from Alice, verify 🕐 Queued
- [ ] Start Bob, verify message delivers automatically
- [ ] Failed messages show ❌ after 50 retries (simulate by invalid .onion)
- [ ] Connection pooling reuses connections (check logs)
- [ ] Modal dialogs work correctly (add friend, accept/reject)

### Implementation Order

**Week 1: Tor Foundation**

1. **Replace `TorClient` stub with arti:**
   - Initialize `arti_client::TorClient` with default config
   - Implement bootstrap with progress events
   - Handle errors gracefully

2. **Progress bar UI:**
   - Show during bootstrap
   - Update with percentage and fun messages
   - Use ratatui progress widget

3. **Persistent identity:**
   - Load/generate Ed25519 keypair
   - Store in database
   - Derive .onion address

4. **Create `HiddenService`:**
   - Use arti to create hidden service
   - Listen on local port (9051)
   - Spawn listener task

5. **Listener task:**
   - Accept incoming TCP connections
   - Read length-prefixed JSON
   - Send to app via channel

6. **Basic connection sending:**
   - Use `TorClient::connect()` to peer .onion
   - Write length-prefixed JSON
   - Handle errors

**Week 2: Signal Protocol & Message Flow**

7. **PreKey bundle generation:**
   - Generate identity, signed prekey, one-time prekeys
   - Store in database
   - Serialize/deserialize

8. **X3DH key agreement:**
   - Implement using libsignal-dezire
   - Session initialization from PreKey bundle
   - Session initialization from PreKey message

9. **Encrypt/decrypt:**
   - Implement using Double Ratchet
   - Persist session state after each operation
   - Handle both PreKeyMessage and regular Message

10. **Connection pooling:**
    - HashMap of active connections
    - 5-minute idle timeout
    - Lazy cleanup

11. **Message sending flow:**
    - Load session, encrypt, send
    - Insert into messages table
    - Update UI with status

12. **Message receiving flow:**
    - Receive, load session, decrypt
    - Insert into messages table
    - Send delivery receipt

13. **Delivery receipts:**
    - Send after successful receive
    - Update sender's message status
    - Update sender's UI

**Week 3: Friend Requests & Polish**

14. **Friend request modal UI:**
    - Input field for friend code
    - Validation and error display
    - Send request on Enter

15. **Friend request protocol:**
    - Send `FriendRequest` message
    - Validate signature and timestamp
    - Store in database

16. **Receive request modal:**
    - Show pending requests
    - Accept/reject buttons
    - Generate PreKey bundle on accept

17. **Accept flow:**
    - Send `FriendRequestAccept` with PreKey
    - Initialize Signal session on receive
    - Update UI

18. **Background queue processor:**
    - Tokio task, 3-minute interval
    - Query database, try to send
    - Update status, increment retry

19. **Status icons and animations:**
    - 🕐 ✓ ✓✓ ❌ icons
    - Color coding
    - Smooth transitions

20. **Error handling:**
    - Circuit failures
    - Connection timeouts
    - Invalid messages
    - Graceful degradation

21. **Integration tests:**
    - Two-instance test harness
    - Full message flow test
    - Offline queueing test

22. **Documentation:**
    - Update Phase2-Progress.md
    - Document new features
    - Testing guide

---

## Key Technical Details

### Arti Configuration

```rust
let config = arti_client::TorClientConfig::default();
let tor_client = arti_client::TorClient::create_bootstrapped(config).await?;
```

### Hidden Service Creation

```rust
let identity_keypair = load_or_generate_identity(&db)?;
let hidden_service = tor_client
    .launch_onion_service(
        identity_keypair,
        OnionServiceConfig::default(),
    )
    .await?;
let onion_address = hidden_service.onion_name();
```

### Length-Prefixed Framing

```rust
// Send
async fn send_message(stream: &mut TcpStream, msg: &Message) -> Result<()> {
    let json = serde_json::to_vec(msg)?;
    let len = (json.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&json).await?;
    stream.flush().await?;
    Ok(())
}

// Receive
async fn receive_message(stream: &mut TcpStream) -> Result<Message> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    let mut json_bytes = vec![0u8; len];
    stream.read_exact(&mut json_bytes).await?;

    let msg: Message = serde_json::from_slice(&json_bytes)?;
    Ok(msg)
}
```

### Signal Protocol Session

```rust
use libsignal_protocol::{
    SessionBuilder, SessionCipher, PreKeyBundle,
    IdentityKeyPair, SignedPreKeyRecord,
};

// Initialize from PreKey bundle
let session_builder = SessionBuilder::new(
    &store,
    &remote_address,
)?;
session_builder.process_prekey_bundle(&prekey_bundle).await?;

// Encrypt
let session_cipher = SessionCipher::new(&store, &remote_address)?;
let ciphertext = session_cipher.encrypt(&plaintext).await?;

// Decrypt
let plaintext = session_cipher.decrypt(&ciphertext).await?;
```

---

## Success Criteria

Phase 2b is complete when:

- [ ] Two instances can bootstrap to Tor network
- [ ] Each instance has persistent .onion address
- [ ] Friend requests can be sent and received
- [ ] Friend requests can be accepted with PreKey exchange
- [ ] Messages can be sent and encrypted with Signal Protocol
- [ ] Messages can be received and decrypted correctly
- [ ] Delivery receipts update sender's UI
- [ ] Offline messages queue and retry automatically
- [ ] Failed messages show ❌ after max retries
- [ ] All integration tests pass
- [ ] Manual testing checklist complete
- [ ] TUI shows connection status, friend requests, message status
- [ ] No crashes, graceful error handling throughout

---

**Document Status:** Approved and ready for implementation planning
**Next Action:** Create Phase 2b implementation plan with detailed task breakdown
