# Testing Guide for Phase 2

## Quick Start

```bash
# Run all tests
cargo test

# Build and run the TUI
cargo run

# Or run the release build
cargo build --release
./target/release/torrent-chat
```

## What Works Right Now ✅

### 1. **TUI Interface**
- Basic ratatui interface loads
- Header shows version and "Tor: Not Connected" status
- Footer shows "Phase 2: Core Foundation + Tor Integration"
- Press `q` to quit

### 2. **Database Layer**
- SQLCipher database creation at `~/.config/torrent-chat/chat.db`
- Schema version 2 with all Phase 2 tables:
  - `message_queue` - Offline message delivery
  - `signal_sessions` - Encryption state
  - `blocked_onions` - Spam protection
  - `messages_fts` - Full-text search (FTS5)
- All FTS triggers and indices in place

### 3. **Identity Management**
- Ed25519 keypair generation
- Message signing and verification
- Key persistence in encrypted database

### 4. **Friend Code System**
- Friend code generation (format: `word-NNNN-word-NNNN`)
- Friend code validation with checksum
- Mapping between friend codes and .onion addresses

### 5. **Protocol Layer**
- Message type definitions (5 types):
  - `FriendRequest`
  - `FriendRequestAccept`
  - `FriendRequestReject`
  - `TextMessage`
  - `DeliveryReceipt`
- JSON serialization/deserialization
- All message types tested

### 6. **Message Queue**
- FIFO queue implementation
- Database persistence
- Retry logic with configurable max retries
- Query by .onion address

## What's Stubbed (Returns Success) 🔧

### 1. **Tor Integration**
- `TorClient::new()` - Creates stub, doesn't connect to Tor
- `TorClient::bootstrap()` - Returns Ok immediately
- `TorConnection::new()` - Creates stub connection
- `TorConnection::send()` - Returns Ok without sending
- `TorConnection::receive()` - Returns empty Vec
- `HiddenService::new()` - Creates stub with fake .onion

### 2. **Signal Protocol Encryption**
- `SignalSession::new()` - Creates stub session
- `SignalSession::encrypt()` - Returns input as-is (no encryption)
- `SignalSession::decrypt()` - Returns input as-is (no decryption)

### 3. **App State Tor Initialization**
- `App::init_tor()` - Creates stub TorClient but doesn't bootstrap

## Testing Scenarios

### Scenario 1: Run the TUI

```bash
cargo run
```

**What you'll see:**
- TUI launches with Phase 2 header
- Shows "Tor: Not Connected" (because it's a stub)
- Press `q` to quit

**What's happening:**
- App initializes database
- Generates identity keypair
- Creates app state with Phase 2 components
- Renders TUI with Phase 2 status

### Scenario 2: Test Database Schema

```bash
# Run tests
cargo test db::schema

# Check the actual database (macOS)
sqlite3 ~/Library/Application\ Support/torrent-chat/messages.db

# Or on Linux
sqlite3 ~/.local/share/torrent-chat/messages.db

# In sqlite3:
.tables              # Should show all Phase 2 tables
.schema message_queue
.schema signal_sessions
.schema blocked_onions
.schema messages_fts
```

### Scenario 3: Test Protocol Messages

```bash
# Run protocol tests
cargo test protocol::message

# All message types serialize/deserialize correctly
```

### Scenario 4: Test Message Queue

```bash
# Run queue tests
cargo test net::queue

# Tests verify:
# - FIFO ordering
# - Retry logic
# - Database persistence
# - Filtering by onion address
```

### Scenario 5: Test Address Mapping

```bash
# Run address mapping tests
cargo test tor::address

# Tests verify:
# - .onion → friend code (deterministic)
# - friend code → .onion (lookup)
# - Round-trip consistency
```

### Scenario 6: Test Integration Points

```bash
# Run integration tests
cargo test --test integration

# Tests verify:
# - Cross-module interaction
# - Database schema completeness
# - Message serialization
# - Queue integration
```

## What You CAN'T Test Yet ❌

### 1. **Actual Tor Connectivity**
- No real Tor network connections
- No hidden service hosting
- No .onion reachability

### 2. **Real Encryption**
- Signal Protocol not implemented
- Messages not actually encrypted
- No key exchange

### 3. **Network I/O**
- No TCP connections
- No message sending over network
- No friend requests over Tor

### 4. **End-to-End Flows**
- Can't send real messages between users
- Can't establish friend connections
- Can't test offline delivery (no network to be offline from)

## Performance Testing

```bash
# Run with timing
cargo test --release -- --nocapture

# Check binary size
ls -lh target/release/torrent-chat

# Memory usage (while app running)
ps aux | grep torrent-chat
```

## Next Steps for Full Testing

To enable real end-to-end testing, you need to implement:

1. **Tor Integration** (from stubs)
   - Replace `TorClient` with real arti integration
   - Implement `HiddenService` with actual hidden service
   - Implement `TorConnection` with SOCKS5 proxy

2. **Signal Protocol** (from stubs)
   - Replace `SignalSession` with libsignal-dezire
   - Implement key exchange
   - Implement actual encryption/decryption

3. **Network Layer**
   - TCP connection handling
   - JSON framing over TCP
   - Background retry task for message queue

4. **Integration Tests**
   - Two-instance local tests
   - Friend request flow
   - Message send/receive
   - Offline queueing

## Debugging Tips

### Enable Logging
```bash
RUST_LOG=debug cargo run
```

### Check Database
```bash
# View database contents
sqlite3 ~/.config/torrent-chat/chat.db "SELECT * FROM friends;"

# Check schema version
sqlite3 ~/.config/torrent-chat/chat.db "PRAGMA user_version;"
```

### Run Specific Test
```bash
cargo test test_message_queue_integration -- --nocapture
```

### Clean State
```bash
# Remove database to start fresh (macOS)
rm -rf ~/Library/Application\ Support/torrent-chat/

# Or on Linux
rm -rf ~/.local/share/torrent-chat/
```

## Known Issues

1. **Warnings during build**: Many "unused" warnings because Phase 2 components are stubs
   - These are expected and will disappear when stubs are replaced

2. **TUI shows "Not Connected"**: Correct! Tor stubs don't actually connect
   - This will change when arti integration is added

3. **No actual messaging**: Correct! Network layer is stubbed
   - Phase 2 provides the foundation, not the implementation

## Success Criteria

✅ **Phase 2 is complete when:**
- All 52 tests passing (DONE)
- Database schema v2 in place (DONE)
- Protocol types defined (DONE)
- Tor stubs integrated into app state (DONE)
- TUI shows Phase 2 status (DONE)
- No compile errors (DONE)

❌ **Phase 2 is NOT about:**
- Actually connecting to Tor
- Actually encrypting messages
- Actually sending data over network
- End-to-end testing

Those come in Phase 2b (stub replacement) or Phase 3!
