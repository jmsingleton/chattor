# chattor

**Peer-to-peer encrypted chat over Tor, right in your terminal.**

A TUI chat application where each user runs their own Tor hidden service.
Messages are end-to-end encrypted with Signal Protocol (Double Ratchet),
stored in an encrypted local database, and routed entirely through Tor.
No central servers. No accounts. No metadata leakage.

Built in Rust with [ratatui](https://github.com/ratatui/ratatui).

---

## How It Works

```
You → TUI (ratatui) → Signal Protocol (E2E) → Tor Hidden Service → Peer
```

- **Identity**: Your Ed25519 keypair *is* your identity. The public key derives your v3 `.onion` address directly — no registration, no usernames, no servers to ask permission from.
- **Sending**: Messages are encrypted with Signal Protocol (X3DH key exchange + Double Ratchet), wrapped in a JSON envelope, and sent to the recipient's hidden service over Tor.
- **Receiving**: Your node runs a Tor hidden service that listens for incoming connections. Messages arrive encrypted and are decrypted locally.
- **Offline delivery**: If a peer is offline, messages are queued locally and retried automatically (up to 50 attempts, every 3 minutes) until delivery succeeds.

---

## Features

### Encryption & Identity

- **Signal Protocol** — X3DH key exchange + Double Ratchet, implemented with `x25519-dalek` and `chacha20poly1305` (not a binding to libsignal-c)
- **Vanity .onion mining** — pick a prefix for your onion address, mined in parallel with rayon
- **Ed25519 identity** — one keypair per user, stored encrypted, derives your `.onion` address
- **SQLCipher** — database encrypted at rest with bundled SQLCipher (via `rusqlite`)

### Networking

- **Pure P2P over Tor** — each user hosts a hidden service; no central relay, no NAT traversal headaches
- **Offline message queue** — FIFO queue with configurable retry logic, persisted to database
- **Friend codes** — 32-word mnemonic encoding of your public key (256-word list, 8 groups of 4 words)

### Broadcast Channels

- **Public & Friends-Only** channels — two auto-created per user, with subscriber management
- **Ed25519-signed posts** — every post is cryptographically signed by the publisher
- **Pull-based sync** — subscribers request missed posts; no push spam
- **Read receipts** — publishers see "seen by N" per post
- **100-post retention** — older posts pruned automatically
- **Auto-subscribe** — friends subscribe to each other's channels on friend accept

### Terminal UI

- **7 themes** — dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn
- **TOML config** — override any theme color via `~/.config/chattor/theme.toml`
- **Animated bootstrap** — mushroom ASCII art while connecting to Tor (it takes a moment)
- **Mining UI** — full-screen vanity mining with live ETA, progress stats, and ASCII art
- **Sidebar + conversation + channels** — friends list, chat view, and channel feed all in one layout
- **Clipboard support** — wl-copy, xclip, xsel, pbcopy (whatever your system has)

---

## Technical Highlights

A few pieces that were particularly interesting to build:

**Vanity .onion mining** — Rayon spawns workers across all CPU cores, each generating Ed25519 keypairs and checking for your chosen prefix. The UI shows live hashrate, ETA estimates, and animated ASCII art. Skip at any time for a random address.

**Signal Protocol from scratch** — X3DH key exchange and Double Ratchet built with `x25519-dalek` and `chacha20poly1305`, not a binding to libsignal-c. PreKey bundles are exchanged during friend requests; subsequent messages use ratcheted keys. Plaintext fallback was removed entirely in Phase 5 — no session, no message.

**Encrypted database with full-text search** — SQLCipher provides at-rest encryption. FTS5 virtual tables with auto-sync triggers keep the search index up to date without manual bookkeeping. Schema migrations run automatically from v2 through v7.

**Theme engine** — A `Theme` struct with named color fields, 7 preset palettes, and TOML config file support. Every UI component references the theme, so swapping palettes is a single config change or CLI flag.

---

## Quick Start

```bash
# Build
cargo build --release

# Run
cargo run

# Run with a theme
cargo run -- --theme cyberpunk

# Run tests
cargo test
```

### CLI Options

| Flag | Short | Description |
|------|-------|-------------|
| `--debug` | `-d` | Enable debug logging |
| `--theme <name>` | `-t` | Theme preset: `dark`, `light`, `cyberpunk`, `minimal`, `rose-pine`, `rose-pine-moon`, `rose-pine-dawn` |
| `--config-dir <path>` | `-c` | Custom config directory |

### Two-Instance Testing

```bash
# Terminal 1
cargo run -- --config-dir /tmp/alice

# Terminal 2
cargo run -- --config-dir /tmp/bob
```

---

## Project Status

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Core Foundation | Done |
| 2 | Tor + Messaging Foundation | Done |
| 3 | Broadcast Channels | Done |
| 4 | Polish & Theming | Done |
| 5 | Crypto & Identity Hardening | Done |
| 6 | Hardening & Packaging | In Progress |

**Honest note on Tor**: The crypto, database, UI, protocol layer, and message queue are all real and tested. The Tor transport layer (arti integration) is currently stubbed — `TorClient::bootstrap()`, `TorConnection::send()`/`receive()`, and `HiddenService::new()` return success without actual network I/O. Wiring up real Tor connections is part of Phase 6. Everything else works end-to-end.

---

## Project Structure

```
src/
├── app.rs                  # Application state and initialization
├── cli.rs                  # CLI argument parsing (clap)
├── lib.rs                  # Public module declarations
├── config/
│   └── settings.rs         # Application settings and preferences
├── error.rs                # Error types (thiserror)
├── main.rs                 # Entry point, event loop, key handling
├── crypto/
│   ├── identity.rs         # Ed25519 keypair management
│   ├── signal.rs           # Signal Protocol (X3DH + Double Ratchet)
│   ├── session_store.rs    # Signal session persistence
│   └── vanity.rs           # Vanity .onion mining with rayon
├── db/
│   ├── connection.rs       # SQLCipher database + migrations (v2→v7)
│   ├── queries.rs          # All database queries
│   └── schema.rs           # Schema definitions (v7)
├── net/
│   ├── framing.rs          # TCP length-prefixed message framing
│   ├── listener.rs         # Incoming connection listener
│   ├── pool.rs             # Connection pooling (placeholder)
│   ├── queue.rs            # Offline message queue
│   ├── queue_processor.rs  # Queue processing (logic in main.rs)
│   ├── receiver.rs         # Message receiving
│   └── sender.rs           # Message sending
├── protocol/
│   ├── friend_code.rs      # 32-word mnemonic friend codes
│   ├── friend_request.rs   # Friend request protocol
│   └── message.rs          # Wire protocol (12 message types)
├── tor/
│   ├── address.rs          # .onion address utilities
│   ├── client.rs           # Tor client wrapper (arti)
│   ├── connection.rs       # Peer connections over Tor
│   └── hidden_service.rs   # Hidden service hosting
└── ui/
    ├── app_ui.rs           # Main app layout
    ├── bootstrap.rs        # Bootstrap animation screen
    ├── channel_feed.rs     # Broadcast channel UI
    ├── conversation.rs     # Chat conversation view
    ├── mining.rs           # Vanity mining UI screens
    ├── modals.rs           # Dialog modals
    ├── sidebar.rs          # Friends & channels sidebar
    ├── state.rs            # UI state management
    └── theme.rs            # Theme engine (7 presets + TOML config)
```

---

## Requirements

- **Rust** 1.70+ (edition 2021)
- **Platform**: Linux, macOS, BSD (no Windows support)
- **SQLCipher**: bundled via `rusqlite` feature flag — no system dependency needed

## License

MIT OR Apache-2.0
