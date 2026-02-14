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

**Current Status:** Phase 5 complete (Crypto & Identity Hardening). All phases through 5 implemented.

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
cargo test                            # Run all tests (~213 currently)
cargo test protocol::message          # Run specific module tests
cargo test --test integration         # Integration tests only
cargo test -- --nocapture             # Show test output
cargo test --release                  # Test optimized build

# Two-instance integration tests (requires Tor, takes time)
cargo test --test e2e_messaging -- --ignored --nocapture

# Two-instance manual testing
./scripts/test-two-instances.sh
```

> **⚠️ Warning: Vanity mining tests are CPU-intensive.** `cargo test crypto::vanity` spawns
> rayon worker threads across all CPU cores. If a test hangs or multiple instances run
> concurrently, they will peg the CPU at 100%. Always verify no orphaned vanity test
> processes are running (`ps aux | grep vanity`) if load is unexpectedly high.

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
# SELECT * FROM schema_version;  # Current schema version (should be 7)
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
```

### Key Components

**1. Application State (`src/app.rs`)**
- Central `App` struct holds all runtime state
- Owns: Settings, Database, Identity (Ed25519), Tor client, Hidden service, Message queue
- `App::new()` initializes everything synchronously (Tor init is async via `init_tor()`)

**2. Database Layer (`src/db/`)**
- Schema version 7 (Phase 3) - see `src/db/schema.rs`
- SQLCipher for at-rest encryption (bundled via rusqlite)
- Key tables: `friends`, `conversations`, `messages`, `message_queue`, `signal_sessions`, `blocked_onions`
- Channel tables: `channels`, `channel_posts`, `channel_subscribers`, `channel_subscriptions`, `channel_post_receipts`
- FTS5 virtual table (`messages_fts`) for full-text search with auto-sync triggers
- Automatic migrations from v2 through v7 in `src/db/connection.rs`

**3. Identity & Crypto (`src/crypto/`)**
- Ed25519 keypair for identity and signing (`identity.rs`)
- Signal Protocol with real X3DH key exchange and ChaCha20-Poly1305 encryption (`signal.rs`)
- Vanity .onion mining with rayon parallel workers (`vanity.rs`)
- Identity key derives v3 .onion address; `Option<IdentityKeypair>` defers creation to first-run mining

**4. Tor Integration (`src/tor/`) - STUBS**
- `client.rs`: TorClient wrapper (arti integration planned)
- `hidden_service.rs`: Hidden service hosting
- `connection.rs`: Peer-to-peer connections over Tor
- `address.rs`: Friend code ↔ .onion mapping (SHA-256 based, deterministic)
- **All currently return success without real Tor connections**

**5. Protocol Layer (`src/protocol/`)**
- Friend codes: `word-NNNN-word-NNNN` format with checksum validation
- Message types (12 total): `FriendRequest`, `FriendRequestAccept`, `FriendRequestReject`, `TextMessage`, `DeliveryReceipt`, `ReadReceipt`, `ChannelSubscribe`, `ChannelUnsubscribe`, `ChannelPost`, `ChannelSyncRequest`, `ChannelSyncResponse`, `ChannelPostReceipt`
- `ChannelType` enum: `Public`, `FriendsOnly`
- JSON serialization with serde for wire protocol
- Signal Protocol envelope: `signal_ciphertext`, `signal_type` (PreKeyMessage or Message)

**6. Message Queue (`src/net/queue.rs`)**
- FIFO queue for offline message delivery
- Persisted in `message_queue` table
- Retry logic: 50 attempts (configurable), 3-minute intervals
- Filters by destination .onion address

**7. UI Layer (`src/ui/`)**
- Full TUI with ratatui: friends sidebar, conversation view, channel feed
- Bootstrap animation screen during Tor connection
- Header displays: version, Tor connection status, onion address
- Modals: add friend, friend requests, identity, ephemeral settings, channel subscribe
- Channel feed view with post composition (own channels) and read-only view (subscriptions)
- Sidebar shows friends list + channels section (own channels + subscriptions)
- Theme struct provides consistent colors across all UI components

**8. Broadcast Channels (`src/ui/channel_feed.rs`, `src/db/queries.rs`)**
- Two auto-created channels per user: Public and Friends Only
- Posts are Ed25519-signed, stored with 100-post retention limit
- Pull-based sync: subscribers request missed posts via `ChannelSyncRequest/Response`
- Read receipts: publisher sees "seen by N" count per post
- Auto-subscribe: friends automatically subscribe to each other's channels on friend accept
- Periodic sync task runs every 5 minutes

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
- Integration tests (52 total, all passing)

**What's Stubbed (returns success without real implementation):**
- `TorClient::bootstrap()` - doesn't connect to actual Tor network
- `TorConnection::send()` / `receive()` - no real network I/O
- `HiddenService::new()` - no real .onion hosting

**What's Real (Phase 2b + Phase 5):**
- `SignalSession::encrypt()` / `decrypt()` - real ChaCha20-Poly1305 encryption (plaintext fallback removed)
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
- Vanity .onion mining with rayon-based parallel workers
- First-run mining screen: prefix input with live ETA estimate
- Full-screen mining UI with animated ASCII art and live stats
- Auto-accept on prefix match, random fallback on skip/cancel
- Identity lifecycle: `Option<IdentityKeypair>` — load from DB, defer to mining on first run
- Signal Protocol: removed plaintext fallback from encrypt/decrypt (hard error without session)
- PreKeyBundle exchange and real X3DH already wired (from Phase 2b)

### Phase 6: Hardening (In Progress)
**Chunk 1 — Polish & Fixes:**
- Friend request Ed25519 signature verification (closed security gap)
- Dead code removal (QueueProcessor, ConnectionPool, MiningActive)
- Real PreKeyBundle generation in friend request accept flow
- Zero TODOs remaining in src/

**Remaining:**
- Typing indicators, online status, desktop notifications
- Hidden service hosting (waiting on arti onion service APIs)
- Backup/restore, packaging for distributions

## Important Technical Details

### Database Schema Migrations
- Automatic migrations from v2 through v7 in `src/db/connection.rs::Database::initialize()`
- Each version has a `migrate_to_vN()` method that checks current version and applies changes
- Migrations run before `CREATE_TABLES` (which uses `IF NOT EXISTS` for idempotency)
- v3: Clear old Signal sessions (production crypto migration)
- v4: Replace old message queue with general-purpose queue
- v5: Add `last_read_at` for unread tracking
- v6: Add ephemeral message columns (`expires_at`, `ephemeral_ttl`)
- v7: Add 5 broadcast channel tables

### Tor Hidden Service Identity
- Each user's .onion address derived from Ed25519 identity keypair (v3 onion format)
- Friend codes map deterministically to .onion via SHA-256 hash
- Reverse lookup requires in-memory mapping table (HashMap<friend_code, onion>)

### Message Flow (When Implemented)
1. User composes message
2. Encrypt with Signal Protocol (per-conversation session)
3. Wrap in JSON envelope with metadata
4. Send via Tor connection to peer's hidden service
5. If offline: enqueue in `message_queue` table
6. Background task retries every 3 minutes (50 max attempts)
7. Peer receives, decrypts, stores in `messages` table
8. FTS triggers auto-update `messages_fts` for search

### Testing Strategy
- **Unit tests:** Per-module in `#[cfg(test)]` blocks
- **Integration tests:** `tests/integration/messaging_test.rs` (cross-module interaction)
- **Database tests:** Use tempfile crate for isolated test databases
- **Stub behavior:** Tor connection stubs return `Ok(())` to enable integration testing; Signal crypto and friend request signatures are real

## Key Files to Understand

- `src/app.rs` - Central application state and initialization
- `src/db/schema.rs` - Complete database schema (version 7)
- `src/db/queries.rs` - All database queries including channel operations
- `src/protocol/message.rs` - All message types and wire format (12 types)
- `src/net/queue.rs` - Offline message delivery queue
- `src/ui/channel_feed.rs` - Channel post feed rendering
- `src/ui/theme.rs` - Theme struct, 7 preset definitions, hex color parsing, TOML config loading
- `src/ui/mining.rs` - Mining prefix input and full-screen mining UI
- `src/crypto/vanity.rs` - Vanity .onion mining with rayon parallel workers
- `docs/plans/2026-02-06-chattor-design.md` - Complete design vision
- `docs/plans/2026-02-12-broadcast-channels-design.md` - Phase 3 broadcast channels design

## Common Patterns

### Error Handling
- Use `crate::error::Result<T>` (alias for `Result<T, TorrentChatError>`)
- Domain errors: `TorrentChatError::{Database, Crypto, Tor, Network, Io}`
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
- Future: Background tasks for message queue processing, Tor event loop

### Testing Utilities
- `tempfile::NamedTempFile` for temporary test databases
- Integration tests verify cross-module interactions (e.g., queue writes to DB)

## Documentation
- Design docs: `docs/plans/`
- Progress tracking: `docs/Phase2b-Progress.md`, `docs/Phase2-Progress.md`, `docs/phase1-progress.md`
- Implementation plan: `docs/plans/2026-02-09-phase2b-implementation.md`
- Testing guide: `docs/Testing-Phase2b.md`
