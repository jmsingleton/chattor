# Phase 2b Testing Guide

## Automated Tests

### Unit Tests
```bash
# All unit tests
cargo test

# Specific modules
cargo test tor::
cargo test crypto::
cargo test net::
cargo test protocol::
```

### Integration Tests
```bash
# Two-instance tests (requires Tor, ~2 minutes)
cargo test --test e2e_messaging -- --ignored --nocapture

# Individual test
cargo test --test e2e_messaging test_two_instance_friend_request -- --ignored --nocapture
```

## Manual Testing

### Single Instance Smoke Test

```bash
cargo run --release
```

**Expected:**
1. App starts immediately
2. "Bootstrapping Tor connection..." message
3. Progress bar appears (0% → 100%)
4. Status messages: "Building circuits...", "Finding relays...", etc.
5. After 30-60 seconds: "Tor connected!"
6. TUI shows connected status
7. Press 'q' to quit

### Two Instance Testing

#### Setup
```bash
# Terminal 1
cargo run --release -- --config-dir /tmp/alice --debug

# Terminal 2
cargo run --release -- --config-dir /tmp/bob --debug
```

#### Test 1: Friend Request Flow

**Alice:**
1. Wait for Tor to connect
2. Press 'n' to add friend
3. Enter Bob's friend code
4. Press Enter to send request

**Bob:**
1. Wait for Tor to connect
2. See notification: "📬 Friend request from alice..."
3. Press Enter to view request
4. Press 'a' to accept

**Expected Result:**
- Alice sees "Friend request accepted by bob..."
- Both see each other in friends list with ⬤ online indicator
- Signal sessions created in both databases

#### Test 2: Message Sending

**Alice:**
1. Select Bob in friends list
2. Type "Hello, Bob!"
3. Press Enter to send

**Bob:**
1. See message appear in conversation
2. Content matches "Hello, Bob!"

**Alice:**
1. See message status update: ✓ Sent → ✓✓ Delivered

**Expected Result:**
- Message encrypted by Alice
- Message decrypted by Bob
- Delivery receipt sent back
- Status icons update correctly

#### Test 3: Offline Queueing

**Setup:**
1. Alice and Bob connected
2. Stop Bob (Ctrl+C or 'q')

**Alice:**
1. Send message to Bob: "Offline message"
2. See status: 🕐 Queued (amber)

**Wait:** 3-5 minutes (for queue processor)

**Bob:**
1. Restart Bob's instance
2. Wait for Tor to connect
3. Check messages

**Expected Result:**
- Message delivers automatically
- Alice's status updates to ✓✓ Delivered
- Bob receives and decrypts message

#### Test 4: Failed Delivery

**Setup:**
1. Modify Bob's .onion to invalid value in Alice's database
2. Alice sends message

**Expected Result:**
- Message queues
- Background task retries every 3 minutes
- After 50 attempts (~2.5 hours): status shows ❌ Failed

## Database Verification

### Check Persistent Identity

```bash
# Alice
sqlite3 /tmp/alice/data/messages.db "SELECT key FROM settings WHERE key = 'identity_keypair'"

# Should return one row (identity exists)
```

### Check Signal Sessions

```bash
# Alice
sqlite3 /tmp/alice/data/messages.db "SELECT remote_onion FROM signal_sessions"

# Should show Bob's .onion if session established
```

### Check Message Queue

```bash
# Alice
sqlite3 /tmp/alice/data/messages.db "SELECT to_onion, retry_count FROM message_queue"

# Should show queued messages and retry counts
```

### Check Friends

```bash
# Alice
sqlite3 /tmp/alice/data/messages.db "SELECT onion_address, status FROM friends"

# Should show Bob with status 'active'
```

## Performance Testing

### Bootstrap Time
```bash
time cargo run --release
# Expected: 30-60 seconds to "Tor connected!"
```

### Memory Usage
```bash
# While app running
ps aux | grep torrent-chat
# Expected: < 200MB
```

### Message Latency (local)
- Send message, measure time to ✓✓ Delivered
- Expected: < 1 second for localhost
- Expected: 300-800ms for real Tor (Phase 3)

## Known Limitations (MVP)

1. **Signal Protocol:** Uses placeholder encryption
   - encrypt() returns plaintext
   - decrypt() returns ciphertext as-is
   - Session state is stored but not used
   - **Fix:** Replace with real libsignal-dezire

2. **Tor Connections:** Uses localhost instead of Tor SOCKS
   - TorConnection connects to 127.0.0.1:9051
   - No actual Tor routing
   - **Fix:** Use tor_client.inner().connect() with SOCKS

3. **Signature Verification:** Placeholder validation
   - Friend request signatures not verified
   - **Fix:** Extract public key from .onion, verify Ed25519 signature

4. **UI Modals:** Not fully interactive
   - Modals render but need keyboard event handling
   - **Fix:** Add event handling in app_ui.rs

## Troubleshooting

### Tor Bootstrap Fails
- Check internet connection
- Try: `rm -rf /tmp/alice /tmp/bob` and restart
- Check firewall settings

### Messages Not Delivering
- Check both instances connected to Tor
- Check Bob's listener is running: `netstat -an | grep 9051`
- Check message queue: see Database Verification above

### Database Locked
- Only one instance per config directory
- Close previous instance before starting new one
- Or use different --config-dir

## Success Criteria

Phase 2b is complete when:
- [x] Tor bootstraps successfully (both instances)
- [x] Friend requests send and receive
- [x] Friend requests accepted with PreKey exchange
- [x] Messages send with encryption (MVP)
- [x] Messages receive with decryption (MVP)
- [x] Delivery receipts update status
- [x] Offline messages queue and retry
- [ ] Full libsignal integration (real encryption)
- [ ] Real Tor SOCKS connections
- [ ] UI modals fully interactive

Current Status: **MVP Complete**, ready for libsignal and Tor integration
