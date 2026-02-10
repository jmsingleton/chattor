# Network Layer MVP - Design Document

**Date:** 2026-02-09
**Status:** Approved
**Phase:** Completing Phase 1 MVP

## Overview

This completes Phase 1's deliverable: "MVP with functional 1-on-1 encrypted chat over Tor". Implements actual sending/receiving of friend requests over Tor, with simple queue resilience and UI to display user's own friend code.

**Goal:** Two users can exchange friend requests and establish encrypted sessions over real Tor connections.

**Scope:** MVP-focused - keep it simple, save sophisticated features for Phase 2.

---

## Architecture

### 1. Sending Friend Requests (Hybrid Approach)

**Handler Flow:**
```rust
async fn handle_send_friend_request(app: &App, friend_code: &str) -> Result<SendResult> {
    // Validate and create message (already implemented)
    validate_friend_code(friend_code)?;
    let request_msg = FriendRequestHandler::create_request(...)?;

    // Convert friend code to .onion address
    let peer_onion = friend_code_to_onion(friend_code)?;

    // Try direct send with timeout
    match try_send_direct(&app, &peer_onion, &request_msg).await {
        Ok(_) => return Ok(SendResult::SentImmediately),
        Err(_) => {
            // Queue for background delivery
            app.message_queue.enqueue(peer_onion, request_msg, priority="high")?;
            return Ok(SendResult::Queued);
        }
    }
}

async fn try_send_direct(app: &App, peer_onion: &str, msg: &Message) -> Result<()> {
    let tor_client = app.tor_client.as_ref().ok_or(...)?;

    // 5-second timeout for connection
    let conn = timeout(
        Duration::from_secs(5),
        TorConnection::connect(tor_client, peer_onion)
    ).await??;

    conn.send(msg).await?;
    Ok(())
}
```

**Key Decisions:**
- 5-second timeout (reasonable for Tor circuit building)
- On failure: queue automatically (user doesn't need to know why)
- UI shows: "Sent ✓" or "Queued 🕐"

### 2. Receiving Friend Requests (Listener Task)

**Listener Architecture:**
```
Hidden Service (port 9051)
  ↓
Listener Task (spawned in init_tor)
  ↓
[Accept connection] → [Receive message] → [Send to main thread via channel]
  ↓
Main Loop polls channel
  ↓
Message Router → handle_incoming_friend_request()
```

**Implementation:**
```rust
// In App::init_tor()
let (msg_tx, msg_rx) = mpsc::channel(100);
tokio::spawn(listener_task(hidden_service.clone(), msg_tx));

// Store receiver in App
self.incoming_message_rx = Some(msg_rx);

// Listener task
async fn listener_task(hidden_service: HiddenService, tx: mpsc::Sender<IncomingMessage>) {
    loop {
        match hidden_service.accept().await {
            Ok(stream) => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let msg = receive_message(stream).await?;
                    tx.send(IncomingMessage { message: msg, ... }).await?;
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }
}

// In main loop (after event polling)
if let Ok(incoming) = app.incoming_message_rx.try_recv() {
    handle_incoming_message(&app, incoming)?;
}

fn handle_incoming_message(app: &App, incoming: IncomingMessage) -> Result<()> {
    match incoming.message.payload {
        Payload::FriendRequest(req) => {
            // Insert into database
            app.db.connection().execute(
                "INSERT INTO friend_requests (from_onion, friend_code, timestamp, status)
                 VALUES (?1, ?2, ?3, 'pending')",
                ...
            )?;
        }
        Payload::FriendRequestAccept(accept) => {
            // Initialize Signal session, add friend
            FriendRequestHandler::handle_accept(&app.db, &app.identity, &accept)?;
        }
        Payload::FriendRequestReject(_) => {
            // Remove queued messages for this peer
        }
        _ => {} // Other message types handled later
    }
    Ok(())
}
```

**Key Decisions:**
- Listener sends to main thread via channel (avoids Database Send issue)
- Main loop polls channel every frame (non-blocking try_recv)
- Message routing is simple match statement

### 3. Simple Message Queue

**Queue Schema:**
```sql
CREATE TABLE message_queue (
    id INTEGER PRIMARY KEY,
    peer_onion TEXT NOT NULL,
    message_json TEXT NOT NULL,
    priority TEXT NOT NULL,  -- 'high' or 'normal'
    retry_count INTEGER DEFAULT 0,
    next_retry_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    status TEXT DEFAULT 'pending'
);

CREATE INDEX idx_queue_next_retry ON message_queue(next_retry_at, status);
```

**Simple Retry Logic:**
```rust
// Queue processor task (spawned in init_tor)
async fn queue_processor_task(tx: mpsc::Sender<ProcessQueue>) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let _ = tx.send(ProcessQueue).await;
    }
}

// In main loop
if let ProcessQueue = command {
    let now = SystemTime::now().as_secs();
    let pending = app.message_queue.get_pending_messages(now)?;

    for queued in pending {
        match try_send_direct(&app, &queued.peer_onion, &queued.message).await {
            Ok(_) => {
                app.message_queue.mark_delivered(queued.id)?;
            }
            Err(_) => {
                if queued.retry_count >= 10 {
                    app.message_queue.mark_failed(queued.id)?;
                } else {
                    // Retry in 30 seconds
                    let next_retry = now + 30;
                    app.message_queue.schedule_retry(queued.id, next_retry)?;
                }
            }
        }
    }
}
```

**Key Decisions:**
- Retry every 30 seconds (no exponential backoff for MVP)
- Max 10 retries (5 minutes total)
- High priority messages (friend requests) processed first
- Simple and predictable

### 4. Display Own Friend Code

**UI State:**
```rust
pub enum AppState {
    Normal,
    AddingFriend { ... },
    ViewingFriendRequest { ... },
    ViewingMyIdentity {  // NEW
        friend_code: String,
        onion_address: String,
    },
}
```

**Keyboard Binding:**
- Press `i` → Show identity modal
- Press `Esc` → Back to normal

**Modal Layout:**
```
┌─ My Identity ─────────────────────────────────────────┐
│                                                        │
│  Share this friend code:                              │
│  ┌──────────────────────────────────────────────────┐ │
│  │ happy-7834-tiger-2910                            │ │
│  └──────────────────────────────────────────────────┘ │
│                                                        │
│  Onion Address:                                       │
│  ┌──────────────────────────────────────────────────┐ │
│  │ abc123xyz789def456ghi.onion                      │ │
│  └──────────────────────────────────────────────────┘ │
│                                                        │
│  [i] Close                                            │
└────────────────────────────────────────────────────────┘
```

**Implementation:**
```rust
// In handle_key()
KeyCode::Char('i') if matches!(state, AppState::Normal) => {
    let friend_code = generate_friend_code(&app.onion_address)?;
    return Ok(Some(AppState::ViewingMyIdentity {
        friend_code,
        onion_address: app.onion_address.clone(),
    }));
}
```

---

## Error Handling

**Scenarios:**

1. **Tor Not Ready**
   - Check `app.tor_client.is_some()`
   - Queue message automatically
   - UI shows "Queued (Tor connecting...)"

2. **Peer Offline**
   - Direct send times out (5 sec)
   - Queue automatically
   - UI shows "Queued 🕐"

3. **Max Retries Exceeded**
   - After 10 attempts (5 minutes)
   - Mark as failed
   - UI shows "Failed to deliver ❌"
   - User can retry manually

4. **Database Errors**
   - Log error, show user-friendly message
   - Don't crash app

5. **Invalid Messages**
   - Ignore malformed messages from peers
   - Log for debugging

---

## Testing Strategy

### Unit Tests

1. **Queue Operations**
   - `test_enqueue_message()`
   - `test_get_pending_messages()`
   - `test_mark_delivered()`
   - `test_retry_count_increment()`

2. **Message Routing**
   - `test_route_friend_request()`
   - `test_route_accept_message()`

3. **Friend Code Display**
   - `test_generate_friend_code_from_onion()`
   - `test_identity_modal_key_handling()`

### Integration Tests

**Two-Instance Test:**
```bash
# Terminal 1
cargo run -- --config-dir /tmp/alice

# Terminal 2
cargo run -- --config-dir /tmp/bob

# Test flow:
# 1. Alice presses 'i' → sees friend code
# 2. Bob presses 'a' → enters Alice's code → sends request
# 3. Alice presses 'r' → sees Bob's request → accepts
# 4. Both see confirmation
# 5. Check database: both have friend entry and Signal session
```

### Manual Verification

- [ ] Send friend request while peer online → immediate delivery
- [ ] Send friend request while peer offline → queues and delivers when peer starts
- [ ] Receive friend request → appears in UI
- [ ] Accept friend request → Signal session established
- [ ] Reject friend request → removed from database
- [ ] View own identity → friend code displays correctly
- [ ] Queue retries work → message eventually delivered
- [ ] Max retries → message marked as failed

---

## Implementation Tasks

**Phase 1: Core Sending**
1. Implement `try_send_direct()` helper
2. Update `handle_send_friend_request()` to use hybrid approach
3. Add `SendResult` enum and UI feedback
4. Test with two instances (both online)

**Phase 2: Queue System**
5. Add message_queue table schema
6. Implement `MessageQueue::enqueue()`, `get_pending()`, `mark_delivered()`
7. Spawn queue processor task in `init_tor()`
8. Wire queue processor to main loop
9. Test with offline peer → comes online → message delivers

**Phase 3: Listener**
10. Implement `listener_task()` with DataStream accept
11. Add `incoming_message_rx` to App struct
12. Poll channel in main loop
13. Implement `handle_incoming_message()` router
14. Test receiving friend requests

**Phase 4: Accept/Reject Handling**
15. Update `handle_accept_friend_request()` to actually send
16. Wire up `handle_incoming` for Accept messages
17. Test full flow: send request → receive → accept → both have session

**Phase 5: Identity UI**
18. Add `ViewingMyIdentity` state
19. Handle `i` key press
20. Render identity modal
21. Test displaying friend code

---

## Success Criteria

✅ **Phase 1 MVP Complete When:**
1. User A can send friend request to User B (both running)
2. User B sees friend request in UI
3. User B can accept → both establish Signal session
4. User B can reject → request removed
5. User can view their own friend code
6. If peer offline, request queues and delivers when peer comes online
7. All tests passing
8. Works with two instances on same machine

This completes Phase 1 MVP. Ready to move to Phase 2 (Enhanced Messaging) after this.
