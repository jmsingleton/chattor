# Phase 5: Vanity .Onion Mining & Signal Protocol Wiring

**Design Document**
Date: 2026-02-12
Status: Approved

## Overview

Two tightly related features shipped together as "Phase 5: Crypto & Identity":

1. **Vanity .onion mining** — First-run experience where users can mine a custom .onion prefix before generating their permanent identity. Toggleable full-screen mining UI with background progress indicator.
2. **Signal Protocol wiring** — Remove the plaintext fallback from encrypt/decrypt. Wire real X3DH key exchange into the friend request flow so all conversations use ChaCha20Poly1305 by default.

Also fixes a bug where `App::new()` generates a throwaway identity before `init_tor()` loads/generates the real one.

## Core Principles

- Mining is first-run only, opt-in (user can skip for instant random address)
- Mining runs on all CPU cores via `rayon`, communicates to UI via `tokio::sync::watch`
- Signal encryption is mandatory for all conversations — no plaintext fallback
- Identity lifecycle is consolidated: one path, one keypair, persisted once

---

## Section 1: Identity Lifecycle Fix

### Problem

`App::new()` (line 45) calls `IdentityKeypair::generate()` creating a throwaway keypair. Later, `init_tor()` (line 76) calls `load_or_generate()` which may create a different keypair. Two independent generations on first run.

### Solution

Consolidate identity into a single path:

```
App starts → check DB for identity keypair
  ├─ Found → load it, proceed to Tor bootstrap
  └─ Not found → enter first-run flow
       ├─ Show "Create Your Identity" prefix input screen
       ├─ User types prefix (or skips)
       ├─ Mine or instantly generate
       ├─ Persist winning keypair to DB
       └─ Proceed to Tor bootstrap
```

`App::new()` stores `Option<IdentityKeypair>` — `None` until identity is resolved. `init_tor()` uses the already-resolved identity.

---

## Section 2: Vanity Mining Core

### Algorithm

Each rayon worker runs independently:
```
loop {
    keypair = Ed25519 SigningKey::generate(OsRng)
    onion = derive_onion_address(keypair.verifying_key())
    if onion.starts_with(target_prefix) {
        send winning keypair → done
    }
    increment atomic counter
}
```

### Performance Characteristics

.onion addresses use base32 (`a-z, 2-7` = 32 characters). Expected attempts per prefix length:

| Prefix Length | Expected Attempts | Est. Time (8 cores, ~150k keys/s/core) |
|---------------|-------------------|----------------------------------------|
| 1 char | 32 | Instant |
| 2 chars | 1,024 | Instant |
| 3 chars | 32,768 | < 1 second |
| 4 chars | 1,048,576 | ~1 second |
| 5 chars | 33,554,432 | ~30 seconds |
| 6 chars | 1,073,741,824 | ~15 minutes |
| 7 chars | 34,359,738,368 | ~8 hours |

### Communication

- `rayon` thread pool for CPU-bound mining work
- `crossbeam::channel` to send updates from rayon workers to async runtime
- `tokio::sync::watch` to bridge into the TUI render loop
- Update message: `MiningProgress { attempts: u64, keys_per_sec: f64, best_match: Option<(IdentityKeypair, String)>, found: bool }`

### Prefix Validation

Only allow characters in the base32 alphabet: `a-z` and `2-7`. Reject `0`, `1`, `8`, `9`, and uppercase at input time. Max prefix length: 7 (anything longer would take days+).

### New Dependency

```toml
rayon = "1.10"
```

---

## Section 3: Mining UI

Three screens, all themed via `&Theme`.

### Screen 1: Prefix Input (First Run)

```
╭─────────────────────────────────────────────────╮
│                                                 │
│         Create Your Identity                    │
│                                                 │
│   Your .onion address is your identity on the   │
│   Tor network. You can mine a vanity prefix     │
│   to make it memorable.                         │
│                                                 │
│   Desired prefix: chat█                         │
│   Estimated time: ~1 second (4 chars)           │
│                                                 │
│   Valid characters: a-z, 2-7                    │
│                                                 │
│   [Enter] Start mining                          │
│   [Esc] Skip (use random address)               │
╰─────────────────────────────────────────────────╯
```

- ETA updates live as user types each character
- Invalid characters rejected silently

### Screen 2: Full-Screen Mining View

```
╭─────────────────────────────────────────────────╮
│                                                 │
│          ⛏  MINING VANITY ADDRESS  ⛏           │
│                                                 │
│         (animated ASCII mining art)             │
│                                                 │
│   Target:  chat______________________________   │
│   Best:    chaql7ym4...onion                    │
│                                                 │
│   Hash rate:  142,387 keys/sec                  │
│   Attempts:   8,431,209                         │
│   Elapsed:    00:59                             │
│   Est. left:  ~01:12                            │
│                                                 │
│   [Esc] Browse UI    [Enter] Accept best match  │
│   [q] Cancel mining                             │
╰─────────────────────────────────────────────────╯
```

- Animated pickaxe/block art (similar style to bootstrap mushroom art)
- "Best match" shows longest prefix match found so far
- Three actions: toggle to normal UI, accept current best, cancel

### Screen 3: Header Mining Indicator

When user presses Esc to browse the UI while mining continues:

```
  chattor  ⛏ Mining "chat..." 142k/s  ◌ Connecting...
```

Press `m` from normal UI to toggle back to full-screen mining view.

### State Machine

```
AppState::MiningPrefixInput { prefix, cursor }
    ├─ Enter → AppState::Mining { ... } (start rayon workers)
    └─ Esc → generate random identity, proceed to bootstrap

AppState::Mining { prefix, progress_rx, show_full_screen }
    ├─ Esc → show_full_screen = false (header indicator mode)
    ├─ m (from Normal) → show_full_screen = true
    ├─ Enter → accept best match, save to DB, proceed to bootstrap
    ├─ q → cancel mining, generate random identity
    └─ (match found) → auto-save, proceed to bootstrap
```

---

## Section 4: Signal Protocol Wiring

### PreKey Bundle Exchange

Extend the friend request flow to carry PreKey bundles:

**FriendRequest message:**
```json
{
  "type": "FriendRequest",
  "from_onion": "alice.onion",
  "public_key": "...",
  "prekey_bundle": {
    "identity_key": "base64...",
    "signed_prekey": { "key_id": 1, "public_key": "base64...", "signature": "base64..." },
    "prekey": { "key_id": 1, "public_key": "base64..." }
  },
  "signature": "..."
}
```

**FriendRequestAccept message:** Same structure — includes responder's PreKeyBundle.

Both sides establish a `SignalSession` from the peer's bundle on accept.

### Session Establishment

On friend request accept:
1. Generate own `PreKeyBundle::generate_real(&identity)` → get bundle + private material
2. Send bundle in accept message
3. Receive peer's bundle
4. `SignalSession::from_prekey_bundle(peer_onion, &peer_bundle)` → session
5. Store session via `SessionStore::store_session()`

### Message Encrypt/Decrypt

**Send path:**
1. Load `SignalSession` from `signal_sessions` table
2. `session.encrypt(plaintext)` → `(ciphertext, is_prekey_message)`
3. Set `signal_ciphertext = base64(ciphertext)`, `signal_type = "PreKeyMessage" | "Message"`
4. Update session in DB (send counter incremented)

**Receive path:**
1. Load `SignalSession` from `signal_sessions` table
2. If `signal_type == "PreKeyMessage"` and no session: `from_prekey_message_real()` to establish
3. `session.decrypt(ciphertext)` → plaintext
4. Update session in DB (recv counter incremented)

### Remove Plaintext Fallback

In `signal.rs`:
- `encrypt()`: if `shared_secret.is_none()`, return `Err(TorrentChatError::SessionNotFound(...))`
- `decrypt()`: if `shared_secret.is_none()`, return `Err(TorrentChatError::DecryptionFailed(...))`

---

## Section 5: Error Handling

### Mining Errors
- Invalid prefix characters → rejected at input (not stored)
- Mining cancelled → fall back to random identity generation
- App killed during mining → no identity persisted, mining screen shown again on next launch
- DB write failure after mining → retry write, hold keypair in memory

### Signal Errors
- No session for peer → `TorrentChatError::SessionNotFound` — UI shows error, suggests re-adding friend
- Decryption failure → `TorrentChatError::DecryptionFailed` — show "[decryption failed]" in conversation
- Counter desync → error for now (future: out-of-order message handling)

---

## Section 6: Testing

### Mining Tests
- Unit: `VanityMiner::check_prefix()` correctness
- Unit: ETA estimation for various prefix lengths
- Unit: prefix validation (reject invalid base32 chars)
- Integration: mine 1-char prefix, verify resulting .onion starts with it

### Signal Tests
- Existing: `test_real_session_encryption_decryption` (already passes)
- New: bidirectional encrypt/decrypt after PreKey bundle exchange
- New: `encrypt()` without shared_secret returns `SessionNotFound` error
- New: `decrypt()` without shared_secret returns `DecryptionFailed` error
- New: integration test — friend request → session establishment → encrypted message roundtrip

---

## Section 7: File Changes

### New Files

| File | Purpose |
|------|---------|
| `src/crypto/vanity.rs` | `VanityMiner` struct, rayon mining, prefix validation, ETA |
| `src/ui/mining.rs` | Three mining UI screens (prefix input, full-screen, header indicator) |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `rayon = "1.10"` |
| `src/app.rs` | Fix identity lifecycle — `Option<IdentityKeypair>`, single load path |
| `src/main.rs` | First-run detection, mining screen loop, `m` toggle keybind |
| `src/crypto/mod.rs` | Export `vanity` module |
| `src/crypto/identity.rs` | Add `from_signing_key()` for mined keypairs |
| `src/crypto/signal.rs` | Remove plaintext fallback, error on missing shared_secret |
| `src/protocol/message.rs` | Add PreKeyBundle to friend request/accept message types |
| `src/protocol/friend_request.rs` | Include PreKeyBundle in request/accept flow |
| `src/ui/mod.rs` | Export `mining` module |
| `src/ui/app_ui.rs` | Mining header indicator rendering |
| `src/ui/state.rs` | New `MiningPrefixInput` and `Mining` AppState variants |
