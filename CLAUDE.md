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

**Current Status:** Phase 2b in progress (real Tor + Signal Protocol implementation).

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
```

### Testing
```bash
cargo test                            # Run all tests (52 currently)
cargo test protocol::message          # Run specific module tests
cargo test --test integration         # Integration tests only
cargo test -- --nocapture             # Show test output
cargo test --release                  # Test optimized build

# Two-instance integration tests (requires Tor, takes time)
cargo test --test e2e_messaging -- --ignored --nocapture

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
# SELECT * FROM schema_version;  # Current schema version (should be 2)
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
- Schema version 2 (Phase 2) - see `src/db/schema.rs`
- SQLCipher for at-rest encryption (bundled via rusqlite)
- Key tables: `friends`, `conversations`, `messages`, `message_queue`, `signal_sessions`, `blocked_onions`
- FTS5 virtual table (`messages_fts`) for full-text search with auto-sync triggers
- **Important:** `messages.message_id` (TEXT UUID) added in Phase 2 for deduplication

**3. Identity & Crypto (`src/crypto/`)**
- Ed25519 keypair for identity and signing (`identity.rs`)
- Signal Protocol stubs for E2E encryption (`signal.rs`) - **currently returns plaintext**
- Identity key derives .onion address (not yet implemented)

**4. Tor Integration (`src/tor/`) - STUBS**
- `client.rs`: TorClient wrapper (arti integration planned)
- `hidden_service.rs`: Hidden service hosting
- `connection.rs`: Peer-to-peer connections over Tor
- `address.rs`: Friend code ↔ .onion mapping (SHA-256 based, deterministic)
- **All currently return success without real Tor connections**

**5. Protocol Layer (`src/protocol/`)**
- Friend codes: `word-NNNN-word-NNNN` format with checksum validation
- Message types (5 total): `FriendRequest`, `FriendRequestAccept`, `FriendRequestReject`, `TextMessage`, `DeliveryReceipt`
- JSON serialization with serde for wire protocol
- Signal Protocol envelope: `signal_ciphertext`, `signal_type` (PreKeyMessage or Message)

**6. Message Queue (`src/net/queue.rs`)**
- FIFO queue for offline message delivery
- Persisted in `message_queue` table
- Retry logic: 50 attempts (configurable), 3-minute intervals
- Filters by destination .onion address

**7. UI Layer (`src/ui/`)**
- Basic TUI with ratatui showing Phase 2 status
- Header displays: version, Tor connection status
- Footer: Current phase milestone
- Main UI implementation planned for Phase 3+

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
- `SignalSession::encrypt()` / `decrypt()` - no real encryption (passes through plaintext)

**Next Steps for Phase 2b (Optional):**
- Replace Tor stubs with real arti integration
- Replace Signal stubs with libsignal-dezire implementation
- Add TCP framing and network I/O layer
- Background async task for message queue processing

### Phase 3: Broadcast Channels (Next)
- One-to-many communication system
- Public and friends-only channels
- Channel subscriptions and post delivery

### Phase 4: Polish & Theming
- Animations, full theming system, vanity .onion mining UI

### Phase 5: Hardening
- Security audit, backup/restore, packaging for distributions

## Important Technical Details

### Database Schema Migrations
**Current Issue:** No automatic migration between schema versions. Database with old schema (v1 without `message_id` column) will fail on startup.

**Workaround:** Delete database to start fresh with v2 schema.

**Future Enhancement:** Implement ALTER TABLE migrations in `src/db/connection.rs::Database::initialize()`

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
- **Stub behavior:** All Phase 2 stubs return `Ok(())` to enable integration testing

## Key Files to Understand

- `src/app.rs` - Central application state and initialization
- `src/db/schema.rs` - Complete database schema (version 2)
- `src/protocol/message.rs` - All message types and wire format
- `src/net/queue.rs` - Offline message delivery queue
- `docs/plans/2026-02-06-chattor-design.md` - Complete design vision
- `docs/Phase2-Progress.md` - Current implementation status

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
