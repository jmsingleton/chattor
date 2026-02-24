---
title: Architecture Overview
description: How chattor's components fit together
---

## Design Principles

- **Privacy-first**: No central servers, no telemetry, no metadata leakage
- **Encrypted everywhere**: at-rest (SQLCipher), in-transit (Tor), end-to-end (Signal Protocol)
- **Pure P2P**: Each user = one Tor hidden service
- **Unix-focused**: Linux, macOS, BSD

## High-Level Data Flow

```
User → TUI (ratatui) → Signal Protocol (E2E) → Tor Hidden Service → Peer
```

When you send a message:

1. You type in the TUI input bar
2. The message is encrypted with **Signal Protocol** (Double Ratchet via libsignal-dezire)
3. The ciphertext is wrapped in a versioned `MessageEnvelope` with protocol version, ratchet header, and type metadata
4. It's sent via the **connection pool** (reuses cached Tor circuits)
5. If the peer is offline, it's queued in the **message queue** with retries
6. The peer's Tor hidden service receives the envelope
7. The peer decrypts with their Signal Protocol session
8. The plaintext is stored in the encrypted SQLCipher database

## Key Components

### Application State (`src/app.rs`)

The central `App` struct owns all runtime state: settings, database, identity keys, Tor client, hidden service, and message queue. `App::new()` initializes synchronously; Tor bootstrapping happens asynchronously via `init_tor()`.

### Database Layer (`src/db/`)

SQLCipher provides at-rest encryption. The schema (currently v9) includes tables for friends, conversations, messages, signal sessions, blocked users, channels, and a key-value settings store. FTS5 virtual tables enable full-text message search with auto-sync triggers.

Migrations run automatically from v2 through v9 on startup.

### Networking (`src/net/`)

- **Connection Pool** (`pool.rs`): Uses DashMap for lock-free concurrent access. Caches Tor circuits per peer (max 50). Idle connections evicted after 5 minutes. Retry-on-stale: dead connections are evicted and retried with a fresh circuit.
- **Rate Limiter** (`rate_limit.rs`): Per-peer token bucket rate limiter (5 req/s sustained, 20 burst) to prevent abuse.
- **Message Queue** (`queue.rs`): FIFO queue persisted in the database. Exponential backoff retries (30s base, doubles each attempt, capped at 15min, 24h expiry).
- **Framing** (`framing.rs`): Length-prefixed TCP framing for message I/O over Tor streams.

### Protocol (`src/protocol/`)

13 message types: `FriendRequest`, `FriendRequestAccept`, `FriendRequestReject`, `TextMessage`, `DeliveryReceipt`, `ReadReceipt`, `ChannelSubscribe`, `ChannelUnsubscribe`, `ChannelPost`, `ChannelSyncRequest`, `ChannelSyncResponse`, `ChannelPostReceipt`, `Presence`.

All messages are wrapped in a versioned `MessageEnvelope` and JSON-serialized. Encrypted messages include a Double Ratchet header, base64-encoded ciphertext, and a type flag (PreKeyMessage or Message).

### UI (`src/ui/`)

Built with ratatui. Layout: friends sidebar (left) + conversation/channel view (right). Modals for add friend, friend requests, identity, settings, and channel subscribe. Theme engine with 7 presets and TOML config override.

## Project Structure

```
src/
├── app.rs              # Application state
├── cli.rs              # CLI parsing (clap)
├── main.rs             # Entry point, event loop
├── crypto/             # Ed25519 identity, Signal Protocol
├── db/                 # SQLCipher database, queries, schema
├── net/                # Connection pool, message queue, framing
├── protocol/           # Friend codes, messages, friend requests
├── tor/                # Arti client, onion service, connections
└── ui/                 # TUI layout, themes, modals
```
