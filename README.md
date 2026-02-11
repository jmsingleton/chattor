# chattor

Privacy-first TUI chat application over Tor.

## Status

✅ Phase 1 - Core Foundation (Completed)
✅ Phase 2 - Tor + Messaging Foundation with stubs (Completed)
🚧 Phase 2b - Real Tor + Signal Protocol (In Progress)

## Features Implemented

### Phase 2b: Real Implementation (In Progress)
- [x] Real Tor client with arti bootstrap (30-60 seconds)
- [x] Persistent .onion address from Ed25519 identity
- [x] TCP listener for incoming connections
- [x] Length-prefixed message framing
- [x] Signal Protocol foundation (MVP stubs)
- [x] Encrypted message sending/receiving
- [x] Friend request protocol with PreKey exchange
- [x] Background queue processor for offline delivery
- [x] Bootstrap progress bar UI
- [x] Friend request modals
- [ ] Full libsignal-dezire integration
- [ ] Real Tor SOCKS proxy connections
- [ ] Interactive UI with keyboard handling

See `docs/Phase2b-Progress.md` for detailed status.

## Building

```bash
cargo build --release
```

## Running

```bash
# Single instance
cargo run

# Two instances for testing
./scripts/test-two-instances.sh
```

## Testing

```bash
# Unit tests
cargo test

# Integration tests (requires Tor, takes time)
cargo test --test e2e_messaging -- --ignored
```

## Two-Instance Testing

```bash
# Terminal 1 (Alice)
cargo run -- --config-dir /tmp/alice

# Terminal 2 (Bob)
cargo run -- --config-dir /tmp/bob
```

Both instances will bootstrap to Tor (30-60 seconds), then you can test:
1. Get Bob's friend code from UI
2. Alice sends friend request to Bob
3. Bob accepts request
4. Alice and Bob exchange encrypted messages

## Project Structure

```
src/
├── main.rs           # Entry point
├── app.rs            # Application state with Tor integration
├── crypto/           # Identity, Signal Protocol, session storage
├── db/               # SQLCipher database
├── net/              # Networking (sender, receiver, queue, listener)
├── protocol/         # Friend codes, message types, friend requests
├── tor/              # Tor client, hidden service, connections
└── ui/               # TUI with bootstrap progress and modals
```

## Next Steps

Phase 2b completion requires:
- Replace Signal Protocol MVP stubs with full libsignal implementation
- Use real Tor SOCKS proxy instead of localhost
- Add keyboard event handling for interactive UI
- Comprehensive testing with two instances

Phase 3 (Broadcast Channels) will build on this foundation.
