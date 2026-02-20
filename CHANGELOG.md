# Changelog

All notable changes to chattor are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased] — Architectural Hardening

### Added
- **Signal Protocol via libsignal-dezire** — replaced hand-rolled crypto with real X3DH + Double Ratchet from `libsignal-dezire` (AGPL-3.0)
- **MessageEnvelope** — versioned wire format wrapper (`protocol_version: 1`) for forward compatibility
- **Per-peer rate limiting** — token bucket rate limiter (5 req/s sustained, 20 burst) in `net/rate_limit.rs`
- **DashMap connection pool** — lock-free concurrent access replaces `Mutex<HashMap>`, with MAX_POOL_SIZE (50) cap and oldest-idle eviction
- **Schema v9 migration** — wipes stale Signal sessions from previous crypto implementation
- Real Tor hidden service integration via arti
- App settings table with schema v8 migration for .onion persistence
- Shell completions for bash, zsh, and fish (`completions/`)
- Hand-written man page (`man/chattor.1`)
- Enhanced `--help` with keybindings, first-run guide, and file paths
- MIT and Apache-2.0 license files
- CONTRIBUTING.md and SECURITY.md
- `.gitignore` coverage for macOS, keys, logs, profiling data

### Changed
- **CHATTOR_PORT** changed from 9051 to 9735 (avoids confusion with Tor control port)
- **ChattorError** — renamed from `TorrentChatError` across all 21 source files
- **Signal wire format** — TextMessage now carries `signal_header` (Double Ratchet header) alongside `signal_ciphertext`
- Vanity mining system removed — arti now manages .onion keys directly
- Cargo.toml metadata cleaned up (repository, keywords, categories, MSRV)

### Removed
- `anyhow` dependency (unused; error handling is thiserror-only)
- `chacha20poly1305`, `hkdf`, `sha2` dependencies (replaced by libsignal-dezire internals)
- Hand-rolled Signal Protocol code (encrypt/decrypt/x3dh) — replaced by libsignal-dezire
- Stale progress docs (Phase 1-2b) — superseded by CLAUDE.md
- Dead code: QueueProcessor, ConnectionPool, MiningActive state
- Stale TODOs and misleading MVP comments

## [0.1.0] - 2026-02-12

Initial development release covering Phases 1-5.

### Phase 5: Crypto & Identity Hardening
- Vanity .onion mining with rayon parallel workers
- First-run mining screen with prefix input, animated ASCII art, live ETA
- Identity lifecycle: `Option<IdentityKeypair>` defers creation to first run
- Removed plaintext fallback from Signal encrypt/decrypt (hard error without session)
- Ed25519 signature verification on incoming friend requests

### Phase 4: Polish & Theming
- 7 color themes: dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn
- TOML config overrides via `~/.config/chattor/theme.toml`
- `--theme` CLI flag for quick preset switching
- Rounded borders, wider sidebar, connection status indicator
- Polished identity modal with clipboard copy feedback
- Clipboard fallback chain: arboard → wl-copy → xclip → xsel → pbcopy

### Phase 3: Broadcast Channels
- Two auto-created channels per user: Public and Friends Only
- 6 channel protocol messages (subscribe, unsubscribe, post, sync request/response, receipt)
- Ed25519-signed posts with 100-post retention limit
- Pull-based sync every 5 minutes via `ChannelSyncRequest/Response`
- Read receipts with "seen by N" count per post
- Auto-subscribe to friend channels on friend accept
- Channel sidebar section, feed view, subscribe modal

### Phase 2/2b: Tor & Messaging Foundation
- Tor module structure (client, connections, hidden service) with arti
- Real Signal Protocol: X3DH key exchange + Double Ratchet (ChaCha20-Poly1305)
- Signal session storage with libsignal
- TCP framing layer for message I/O
- Message queue with retry logic (50 attempts, 3-minute intervals)
- 12 protocol message types with JSON serialization
- Friend request handling with PreKey exchange
- Database schema v2-v7 with automatic migrations
- Reversible 32-word mnemonic friend codes (256-word dictionary)
- Bootstrap animation screen with mushroom ASCII art
- Friend request modals, identity modal, ephemeral message settings

### Phase 1: Core Foundation
- Ed25519 identity keypair generation and signing
- SQLCipher-encrypted database with FTS5 full-text search
- Friend code generation and validation
- Settings management with platform-specific paths
- Basic TUI with ratatui (friends sidebar, conversation view)
- Error handling with `ChattorError` domain types
