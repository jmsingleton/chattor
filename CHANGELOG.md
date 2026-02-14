# Changelog

All notable changes to chattor are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Real Tor hidden service integration via arti (in progress)
- App settings table with schema v8 migration for .onion persistence
- Shell completions for bash, zsh, and fish (`completions/`)
- Hand-written man page (`man/chattor.1`)
- Enhanced `--help` with keybindings, first-run guide, and file paths
- MIT and Apache-2.0 license files
- CONTRIBUTING.md and SECURITY.md
- `.gitignore` coverage for macOS, keys, logs, profiling data

### Changed
- Vanity mining system removed — arti now manages .onion keys directly
- Cargo.toml metadata cleaned up (repository, keywords, categories, MSRV)

### Removed
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
- Error handling with `TorrentChatError` domain types
