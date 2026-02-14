# Phase 6 Chunk 1: Polish & Fixes — Design

## Overview

Close remaining security gaps, wire unused components, and remove dead code. This is a cleanup chunk — no new features, no new dependencies.

## Scope

Three areas:

1. Friend request signature verification
2. Background queue processor + connection pool wiring
3. Mining stub removal + TODO cleanup

## 1. Friend Request Signature Verification

**Problem:** `validate_request()` in `friend_request.rs` accepts all incoming friend requests without verifying the Ed25519 signature. An attacker can forge requests claiming to be from any .onion address.

**Fix:**
- Extract the sender's Ed25519 public key from their v3 .onion address (the public key is encoded in the address)
- Reconstruct the signed data: `format!("{}{}{}", from_onion, from_friendcode, timestamp)`
- Decode the base64 signature
- Verify with `ed25519_dalek::VerifyingKey::verify()`
- Reject if verification fails

**Files:** `src/protocol/friend_request.rs`, `src/tor/address.rs` (may need a helper to extract pubkey from .onion)

## 2. Background Queue Processor & Connection Pool

**Problem:** The queue processor (`src/net/queue_processor.rs`) has a TODO stub instead of real sending logic. The connection pool (`src/net/pool.rs`) is fully implemented but never instantiated.

**Fix — Connection Pool:**
- Add `ConnectionPool` to `App` struct
- Use it in `try_send_direct()` — check pool first, fall back to new connection
- Connections auto-expire via pool's existing cleanup logic

**Fix — Queue Processor:**
- Replace the TODO in `process_queue()` with real sending:
  1. Load pending messages from DB
  2. Attempt send via TorConnection (using pool)
  3. On success: `mark_delivered()`
  4. On failure: increment retry count, schedule next attempt
- The periodic 30-second task already exists in `app.rs`

**Files:** `src/app.rs`, `src/net/queue_processor.rs`, `src/main.rs`

## 3. Mining Stub Removal & TODO Cleanup

**Problem:** 4 dead `AppAction` variants with stub match arms for runtime mining (first-run mining works, runtime re-mining was cut). Stale TODO comments reference "MVP" stubs that are now real implementations.

**Remove:**
- `AppAction::StartMining`, `AcceptMiningResult`, `CancelMining`, `ToggleMiningView`
- Their match arms in main.rs
- `AppState::MiningActive` variant (dead — first-run uses `MiningPrefixInput` + `run_mining_loop()`)
- Associated `handle_key` arms for dead state

**TODO cleanup:**
- Remove all stale TODO comments after implementing fixes
- Update `create_accept_message()` to use `PreKeyBundle::generate_real()` instead of stub
- Remove misleading "MVP" comments from test-only stub code
- Goal: zero TODOs in `src/`

**Files:** `src/ui/state.rs`, `src/main.rs`, `src/protocol/friend_request.rs`, `src/crypto/signal.rs`, `src/net/queue_processor.rs`

## Out of Scope (Chunk 2 & 3)

- Typing indicators / online status (Chunk 2)
- Desktop notifications (Chunk 2)
- Hidden service hosting (Chunk 3)
- Packaging — deb/rpm/AUR/Homebrew (Chunk 3)
- Backup/restore (Chunk 3)
