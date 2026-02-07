# torrent-chat

Privacy-first TUI chat application over Tor.

## Status

✅ Phase 1 - Core Foundation (Completed)

## Features Implemented

### Phase 1: Core Foundation
- [x] Project structure and build system
- [x] Error handling framework
- [x] Identity key generation (Ed25519)
- [x] Friend code generation and validation
- [x] Database schema and SQLCipher integration
- [x] Settings management
- [x] Basic TUI with ratatui
- [x] Component integration

## Phase 2 (In Progress)
Phase 2 adds Tor integration and basic P2P messaging:
- ✅ Tor hidden service support (stubs)
- ✅ Friend code ↔ .onion address mapping
- ✅ Message queue for offline delivery
- ✅ Signal Protocol integration (stubs)
- ✅ Integration tests for Phase 2 components

See `docs/Phase2-Progress.md` for detailed status.

## Building

```bash
cargo build
```

## Running

```bash
cargo run
```

## Testing

```bash
cargo test
```

## Project Structure

```
src/
├── main.rs           # Entry point
├── app.rs            # Application state
├── cli.rs            # CLI parsing
├── error.rs          # Error types
├── config/           # Settings management
├── crypto/           # Identity and encryption
├── db/               # Database layer
├── protocol/         # Friend codes and protocols
├── tor/              # Tor integration (TODO)
├── net/              # Networking (TODO)
└── ui/               # TUI components
```

## Next Steps

Phase 2 will implement:
- Tor hidden service integration
- P2P messaging protocol
- Friend request flow
- Message encryption (Double Ratchet)
- Message delivery and queueing
