# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## ⚠️ Critical: After Context Compaction

**If your conversation context has been compacted**, you MUST re-read the current implementation plan before continuing work:

1. Check for active plans in `docs/plans/` directory
2. Read the most recent `YYYY-MM-DD-*-implementation.md` file
3. Review the corresponding progress document in `docs/`
4. Understand which tasks are complete, in-progress, or pending
5. Do NOT assume you remember the implementation details from before compaction

The plan files contain the source of truth for what needs to be built and how. Always refer back to them.

## Project Overview

**chattor** is a privacy-first TUI (Terminal User Interface) chat application built in Rust. The architecture is pure peer-to-peer over Tor hidden services with no central servers. Each user runs their own hidden service (.onion address) for receiving messages, with end-to-end encryption via Signal Protocol (Double Ratchet).

**Current Status:** All phases complete plus architectural hardening. Real arti Tor transport, Signal Protocol X3DH + Double Ratchet via libsignal-dezire, DashMap connection pooling with rate limiting, presence system (typing indicators + online status), desktop notifications, and comprehensive e2e test coverage. Distribution packaging available (deb, rpm, AUR, Homebrew).

**Core Design Principles:**
- Privacy-first: No central servers, no telemetry, no metadata leakage
- Encrypted at-rest (SQLCipher), in-transit (Tor), and end-to-end (Signal Protocol)
- Pure P2P: Each user = one Tor hidden service
- Unix-focused: Linux, macOS, BSD (no Windows support)

## Development Commands

### Build & Run
```bash
cargo build              # Development build
cargo build --release    # Optimized build
cargo run                # Run the TUI (press 'q' to quit)
cargo run -- --help      # Show CLI options
cargo run -- --debug     # Enable debug logging
cargo run -- --theme cyberpunk  # Run with a specific theme
```

### Testing
```bash
cargo test                            # Run all tests (~252 currently)
cargo test protocol::message          # Run specific module tests
cargo test --test integration         # Integration tests only
cargo test --test e2e_messaging       # E2E crypto/messaging tests (no Tor needed)
cargo test -- --nocapture             # Show test output
cargo test --release                  # Test optimized build

# Two-instance manual testing
./scripts/test-two-instances.sh
```

### Database Management
```bash
# macOS location
rm -rf ~/Library/Application\ Support/chattor/

# Linux location
rm -rf ~/.local/share/chattor/

# Inspect database (macOS)
sqlite3 ~/Library/Application\ Support/chattor/messages.db
# .schema messages       # View table structure
# .tables                # List all tables
# SELECT * FROM schema_version;  # Current schema version (should be 9)
```

### Linting & Formatting
```bash
cargo fmt                # Format code
cargo fmt -- --check     # Check formatting without changing
cargo clippy             # Run linter
cargo clippy -- -D warnings  # Lint with warnings as errors
```

## Architecture Overview

### High-Level Flow
```
User → TUI (ratatui) → App State → Database (SQLCipher)
                    ↓
            Tor Hidden Service ← → Peer's Hidden Service
                    ↓
            Signal Protocol (E2E Encryption)
                    ↓
            Message Queue (offline delivery)
                    ↓
            Presence System (typing/online) ← → Desktop Notifications
```

### Key Components

**1. Application State (`src/app.rs`)**
- Central `App` struct holds all runtime state
- Owns: Settings, Database, Identity (Ed25519), Tor client, Hidden service, Message queue
- `App::new()` initializes everything synchronously (Tor init is async via `init_tor()`)

**2. Database Layer (`src/db/`)**
- Schema version 9 - see `src/db/schema.rs`
- SQLCipher for at-rest encryption (bundled via rusqlite)
- Key tables: `friends`, `conversations`, `messages`, `message_queue`, `signal_sessions`, `blocked_onions`
- Channel tables: `channels`, `channel_posts`, `channel_subscribers`, `channel_subscriptions`, `channel_post_receipts`
- FTS5 virtual table (`messages_fts`) for full-text search with auto-sync triggers
- Automatic migrations from v2 through v9 in `src/db/connection.rs`

**3. Identity & Crypto (`src/crypto/`)**
- Ed25519 keypair for identity and signing (`identity.rs`)
- Signal Protocol with real X3DH key exchange and Double Ratchet via `libsignal-dezire` (`signal.rs`)
- RatchetState wraps libsignal-dezire's `SessionState` for encrypt/decrypt/serialize
- Identity key used for signing; .onion address managed by arti (not derived from identity key)

**4. Tor Integration (`src/tor/`)**
- `client.rs`: TorClient wrapper around arti-client with persistent state directory
- `hidden_service.rs`: Real arti onion service hosting via `launch_onion_service()`
- `connection.rs`: Peer-to-peer connections over Tor
- `address.rs`: Friend code ↔ .onion mapping (SHA-256 based, deterministic)
- Onion service key managed by arti (persistent across restarts)

**5. Protocol Layer (`src/protocol/`)**
- Friend codes: 32-word mnemonic (256-word dictionary, 8 groups of 4 words)
- Message types (13 total): `FriendRequest`, `FriendRequestAccept`, `FriendRequestReject`, `TextMessage`, `DeliveryReceipt`, `ReadReceipt`, `ChannelSubscribe`, `ChannelUnsubscribe`, `ChannelPost`, `ChannelSyncRequest`, `ChannelSyncResponse`, `ChannelPostReceipt`, `Presence`
- `PresenceType` enum: `Heartbeat`, `TypingStarted`, `TypingStopped`
- `ChannelType` enum: `Public`, `FriendsOnly`
- JSON serialization with serde for wire protocol
- `MessageEnvelope` wrapper with `protocol_version: 1` for forward compatibility
- Signal Protocol envelope: `signal_header` (Double Ratchet), `signal_ciphertext`, `signal_type` (PreKeyMessage or Message)

**6. Message Queue (`src/net/queue.rs`)**
- FIFO queue for offline message delivery
- Persisted in `message_queue` table
- Exponential backoff retries: 30s base, doubles each attempt, capped at 15min, 24h expiry window
- Concurrent per-peer queue processing (10 parallel peers via semaphore)
- Filters by destination .onion address

**7. Connection Pool (`src/net/pool.rs`)**
- Uses `DashMap` for lock-free concurrent access (no async mutex)
- Caches Tor circuits per peer for reuse (avoids 10-30s circuit build per message)
- MAX_POOL_SIZE (50) with oldest-idle eviction when at capacity
- Retry-on-stale: evicts dead connections and retries once with fresh circuit
- Background cleanup: idle connections evicted after 5 minutes
- Timeouts: 30s circuit build, 10s message send

**7b. Rate Limiter (`src/net/rate_limit.rs`)**
- Per-peer token bucket rate limiter for inbound messages
- Default: 5 tokens/sec sustained, 20 burst
- Independent buckets per peer (one peer's abuse doesn't affect others)

**8. UI Layer (`src/ui/`)**
- Full TUI with ratatui: friends sidebar, conversation view, channel feed
- Bootstrap animation screen during Tor connection
- Header displays: version, Tor connection status, onion address
- Modals: add friend, friend requests, identity, ephemeral settings, channel subscribe
- Channel feed view with post composition (own channels) and read-only view (subscriptions)
- Sidebar shows friends list + channels section (own channels + subscriptions)
- Dynamic sidebar status icons: ● online, ✎ typing, ○ offline
- "is typing..." indicator in conversation view
- Theme struct provides consistent colors across all UI components

**9. Broadcast Channels (`src/ui/channel_feed.rs`, `src/db/queries.rs`)**
- Two auto-created channels per user: Public and Friends Only
- Posts are Ed25519-signed, stored with 100-post retention limit
- Pull-based sync: subscribers request missed posts via `ChannelSyncRequest/Response`
- Read receipts: publisher sees "seen by N" count per post
- Auto-subscribe: friends automatically subscribe to each other's channels on friend accept
- Periodic sync task runs every 5 minutes

**10. Presence System (`src/presence.rs`)**
- In-memory `HashMap<String, PeerPresence>` tracks per-peer online/typing state
- Heartbeat background task sends presence to connected peers every 60s
- Peers marked offline after 120s without heartbeat
- Typing indicators with 5s auto-expiry and 4s outgoing debounce
- Presence messages are best-effort (not queued or encrypted — Tor provides transport security)

**11. Desktop Notifications (`src/notifications.rs`)**
- Uses `notify-rust` for native desktop notifications
- Privacy-first: shows sender name only, never message content
- Global toggle via `[n]` keybinding, persisted in `app_settings` table
- 2-second flash feedback in footer when toggled

### Platform-Specific Paths

| Platform | Config Directory | Data Directory |
|----------|------------------|----------------|
| **macOS** | `~/Library/Application Support/chattor/` | `~/Library/Application Support/chattor/` |
| **Linux** | `~/.config/chattor/` | `~/.local/share/chattor/` |

Database path: `{data_dir}/messages.db`

## Phase Implementation Status

### Phase 1: Core Foundation ✅
- Project structure, error handling, identity keys, friend codes, database, settings, basic TUI

### Phase 2: Tor + Messaging Foundation ✅
**What's Complete:**
- Database schema v2 with Phase 2 tables
- Tor module structure with stubs (client, connections, hidden service)
- Signal Protocol stubs (encrypt/decrypt return plaintext)
- Message queue with retry logic
- Protocol message types (5 types with JSON serialization)
- Friend code ↔ .onion address mapping
- Integration tests (226 total, all passing)

**What's Real (Phase 2b + Phase 5 + Phase 6):**
- `TorClient::new_with_data_dir()` - real arti bootstrap with persistent state
- `HiddenService::launch()` - real arti onion service hosting
- `SignalSession::encrypt()` / `decrypt()` - real Double Ratchet via libsignal-dezire (plaintext fallback removed)
- X3DH key exchange via `from_prekey_bundle_real()` / `from_prekey_message_real()`
- TCP framing layer for message I/O

### Phase 3: Broadcast Channels ✅
**What's Complete:**
- Database schema v7 with 5 channel tables and indices
- 6 new protocol message types for channel operations
- 15 channel query functions with full test coverage
- Auto-channel initialization (Public + Friends Only per user)
- Incoming channel message handling (subscribe, unsubscribe, post, sync, receipts)
- Auto-subscribe to friend channels on friend accept
- Channel UI: sidebar channels section, channel feed view, subscribe modal
- Channel action handlers: publish, subscribe, select channel
- Periodic channel sync task (every 5 minutes)
- Integration tests for full channel flow

### Phase 4: Polish & Theming ✅
- Full theming system with Theme struct and 7 preset themes (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)
- TOML config overrides via ~/.config/chattor/theme.toml
- --theme CLI flag for quick preset switching
- All UI components themed (header, sidebar, conversation, modals, bootstrap, channels)
- Rounded borders throughout, wider sidebar (24 chars)
- Connection status indicator in header (◉ Connected / ◌ Connecting)
- Themed keybinding hints in footer
- Polished identity modal with copy feedback ([o] copy onion, [c] copy code)
- Setup wizard removed, replaced with empty state welcome hint
- Clipboard fix with fallback to wl-copy/xclip/xsel/pbcopy

### Phase 5: Crypto & Identity Hardening ✅
- Signal Protocol: removed plaintext fallback from encrypt/decrypt (hard error without session)
- PreKeyBundle exchange and real X3DH already wired (from Phase 2b)

### Phase 6: Hardening ✅
**Chunk 1 — Polish & Fixes: ✅**
- Friend request Ed25519 signature verification (closed security gap)
- Dead code removal (QueueProcessor, ConnectionPool, MiningActive)
- Real PreKeyBundle generation in friend request accept flow
- Zero TODOs remaining in src/

**Chunk 2 — Real Tor Hidden Service: ✅**
- Real arti onion service hosting (replaces stubs)
- Tor rendezvous stream listener for incoming connections
- .onion address persistence across restarts (app_settings table, schema v8)
- Vanity mining system removed (arti manages .onion keys)

**Chunk 3 — P2P Messaging Hardening: ✅**
- ConnectionPool with per-peer caching, idle eviction (5min), retry-on-stale
- Exponential backoff retries (30s→15min cap, 24h expiry window)
- Concurrent per-peer queue processing (10 parallel peers via semaphore)
- CHATTOR_PORT constant, TorConnection dead code cleanup

**Chunk 4 — X3DH Session Fixes + E2E Tests: ✅**
- Fixed session asymmetry: acceptor defers session creation until first PreKey message
- Fixed acceptor-can't-send-first: initiator queues handshake PreKey message on accept
- Fixed PreKey flag: encrypt() uses `.take()` on ephemeral, decrypt() takes explicit `is_prekey_message`
- Friend request accept queued (avoids 30s UI block from direct Tor send)
- 6 comprehensive e2e tests covering full friend-request → X3DH → messaging pipeline

### UX Polish ✅
- Presence system: typing indicators, online status with heartbeat, dynamic sidebar icons
- Desktop notifications via notify-rust with global toggle (`[n]` keybinding)
- Distribution packaging: deb, rpm, AUR (chattor-bin + chattor-git), Homebrew formula
- CI release workflow for automated package builds

### Architectural Hardening ✅
- Signal Protocol replaced with libsignal-dezire (real X3DH + Double Ratchet)
- MessageEnvelope with protocol versioning for wire format evolution
- Schema v9 migration wipes stale sessions
- DashMap connection pool (lock-free, MAX_POOL_SIZE 50)
- Per-peer rate limiting (token bucket, 5 req/s sustained, 20 burst)
- CHATTOR_PORT changed to 9735
- TorrentChatError renamed to ChattorError
- Removed unused dependencies (anyhow, chacha20poly1305, hkdf, sha2)

### Future Work
- Backup/restore functionality
- File transfer support

## Important Technical Details

### Database Schema Migrations
- Automatic migrations from v2 through v9 in `src/db/connection.rs::Database::initialize()`
- Each version has a `migrate_to_vN()` method that checks current version and applies changes
- Migrations run before `CREATE_TABLES` (which uses `IF NOT EXISTS` for idempotency)
- v3: Clear old Signal sessions (production crypto migration)
- v4: Replace old message queue with general-purpose queue
- v5: Add `last_read_at` for unread tracking
- v6: Add ephemeral message columns (`expires_at`, `ephemeral_ttl`)
- v7: Add 5 broadcast channel tables
- v8: Add `app_settings` key-value table for .onion address persistence
- v9: Wipe signal_sessions (libsignal-dezire format incompatible with old hand-rolled crypto)

### Tor Hidden Service Identity
- .onion address generated and managed by arti (v3 onion format, persistent via arti state dir)
- .onion address cached in `app_settings` table for display before Tor connects
- Friend codes map deterministically to .onion via SHA-256 hash
- Reverse lookup requires in-memory mapping table (HashMap<friend_code, onion>)

### X3DH Session Establishment Flow
1. Alice sends friend request to Bob (Ed25519-signed)
2. Bob accepts: generates PreKeyBundle, stores PreKeyPrivateMaterial in `app_settings`, queues accept message
3. Alice receives accept: calls `from_prekey_bundle_real(bob_bundle)`, stores session, queues handshake PreKey message
4. Bob receives handshake: loads stored private material, calls `from_prekey_message_real(ciphertext)`, stores session, cleans up private material
5. Both sides now have established sessions; bidirectional messaging works

### Message Flow
1. User composes message
2. Encrypt with Signal Protocol (per-conversation session)
3. Wrap in MessageEnvelope with `signal_header`, `signal_ciphertext` (base64) and `signal_type` (PreKeyMessage or Message)
4. Send via ConnectionPool (reuses cached Tor circuit or creates new one)
5. If offline: enqueue in `message_queue` table
6. Background task retries with exponential backoff (30s→15min, 24h expiry)
7. Peer receives via Tor rendezvous listener, decrypts with `is_prekey_message` from wire format
8. Stores in `messages` table; FTS triggers auto-update `messages_fts` for search

### Testing Strategy
- **Unit tests:** Per-module in `#[cfg(test)]` blocks (~230 tests)
- **Integration tests:** `tests/integration/` (cross-module interaction, 16 tests including presence)
- **E2E tests:** `tests/e2e_messaging.rs` (full Signal Protocol pipeline via libsignal-dezire, 6 tests)
- **Database tests:** Use tempfile crate for isolated test databases
- **No stubs:** Tor client, hidden service, Signal crypto, and friend request signatures are all real. TorConnection sends over real arti DataStreams via ConnectionPool.

## Key Files to Understand

- `src/app.rs` - Central application state and initialization
- `src/db/schema.rs` - Complete database schema (version 9)
- `src/db/queries.rs` - All database queries including channel operations
- `src/protocol/message.rs` - All message types and wire format (13 types)
- `src/net/queue.rs` - Offline message delivery queue with exponential backoff
- `src/net/pool.rs` - Connection pool with per-peer Tor circuit caching
- `src/ui/channel_feed.rs` - Channel post feed rendering
- `src/ui/theme.rs` - Theme struct, 7 preset definitions, hex color parsing, TOML config loading
- `src/tor/hidden_service.rs` - Real arti onion service hosting
- `src/presence.rs` - Peer presence tracker (online/typing state, heartbeat constants)
- `src/notifications.rs` - Desktop notifications with notify-rust and global toggle
- `tests/e2e_messaging.rs` - E2E tests for full X3DH + messaging pipeline
- `tests/integration/presence_test.rs` - Presence system integration tests
- `docs/plans/2026-02-06-torrent-chat-design.md` - Complete design vision

## Common Patterns

### Error Handling
- Use `crate::error::Result<T>` (alias for `Result<T, ChattorError>`)
- Domain errors: `ChattorError::{Database, Crypto, Tor, Network, Io}`
- Propagate with `?` operator, wrap external errors with `.map_err()`

### Database Operations
```rust
let db = Database::open(&path)?;
let conn = db.connection(); // Get rusqlite::Connection reference
conn.execute("INSERT INTO ...", params)?;
```

### Async/Await
- Tokio runtime with "full" features
- `App::init_tor()` is async, but `App::new()` is sync
- Background tasks: message queue processing, channel sync, heartbeat presence updates

### Testing Utilities
- `tempfile::NamedTempFile` for temporary test databases
- Integration tests verify cross-module interactions (e.g., queue writes to DB)

## Documentation
- Design docs: `docs/plans/`
- Progress tracking: `docs/Phase2b-Progress.md`, `docs/Phase2-Progress.md`, `docs/phase1-progress.md`
- Implementation plan: `docs/plans/2026-02-09-phase2b-implementation.md`
- Testing guide: `docs/Testing-Phase2b.md`
