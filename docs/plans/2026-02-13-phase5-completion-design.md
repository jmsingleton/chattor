# Phase 5 Completion: Mining Integration & Crypto Hardening

**Date:** 2026-02-13
**Status:** Design approved
**Scope:** Complete remaining Phase 5 tasks — identity lifecycle fix, mining UX wiring, plaintext fallback removal, cleanup

## Context

Phase 5 Tasks 1–5 are complete: rayon dependency, vanity mining core, IdentityKeypair constructors, mining UI screens, and AppState variants with key handling. Tasks 9–10 (Signal Protocol wiring) were found to already be implemented in `main.rs`.

**Remaining work:** Tasks 6, 7, 8, 11 from the original plan, plus compiler warning cleanup.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Identity model | `Option<IdentityKeypair>` | Rust-idiomatic, compiler-enforced, narrow None window |
| Match UX | Auto-accept on prefix match | Fastest flow, no extra confirmation step |
| Plaintext removal | Hard error, no migration | Stub sessions from testing fail; user re-adds friends |
| Task order | Mining UX first, then crypto hardening | Gives users the vanity experience and fixes the identity bug |

## Architecture: First-Run Flow

```
App::new()  →  identity = load_from_db()  →  None?
                                               │
                              ┌─────────── yes ─┘
                              │
                    Show MiningPrefixInput screen
                              │
                     User types prefix + Enter
                              │
                    Start rayon mining workers
                              │
                    Show MiningActive fullscreen
                              │
                    Match found → auto-accept
                              │
                    identity.save_to_db()
                    app.identity = Some(keypair)
                              │
                              └────────────────────┐
                                                   │
                                       identity is Some(_)
                                                   │
                                        Bootstrap (Tor connect)
                                                   │
                                          Main UI loop
```

**Skip path:** Esc during prefix input → generate random identity, save, proceed.
**Cancel path:** q during active mining → cancel workers, generate random identity, save, proceed.

## Changes by File

### src/app.rs — Identity lifecycle fix
- Change `identity: IdentityKeypair` to `identity: Option<IdentityKeypair>`
- `App::new()`: call `IdentityKeypair::load_from_db(&db)` instead of `generate()`
- `App::new_with_settings()`: same change
- Remove the `IdentityKeypair::generate()` call in constructors

### src/main.rs — Mining wiring
- After `App::new()`, check `app.identity.is_none()`
- If first run: enter mining prefix input loop
- Mining loop: render MiningPrefixInput, handle keys, transition to MiningActive
- On match found: auto-accept, save to DB, set `app.identity = Some(keypair)`
- On skip/cancel: generate random, save, set identity
- `init_tor()`: use `app.identity.as_ref().expect("set during mining")`
- All main-loop identity accesses: `.as_ref().expect()` (guaranteed by init flow)
- Add `run_mining_loop()` helper function
- Add `num_cpus()` helper function

### src/crypto/signal.rs — Plaintext fallback removal
- `encrypt()`: replace `else` branch (plaintext return) with `Err(TorrentChatError::SessionNotFound(...))`
- `decrypt()`: replace `else` branch with `Err(TorrentChatError::DecryptionFailed(...))`
- Add 2 tests verifying error on missing shared_secret

### Compiler warning cleanup (multiple files)
- `src/tor/hidden_service.rs`: prefix unused `tor_client` with `_`
- `src/ui/state.rs`: prefix unused `publisher_onion` with `_`, remove unused import
- `src/net/listener.rs`: remove unused `std::io` import
- `src/net/sender.rs`: remove unused `SignalSession` import, update deprecated base64 calls
- `src/crypto/signal.rs`: prefix unused `remote_onion` with `_`
- `src/ui/bootstrap.rs`: remove unused `Color` import
- `src/net/connection.rs`: remove unused `TcpStream` import
- `tests/e2e_messaging.rs`: prefix unused variables with `_`, remove unused `Arc` import
- `src/net/sender.rs` and `src/net/receiver.rs`: update deprecated `base64::encode`/`base64::decode` to `Engine::encode`/`Engine::decode`

### CLAUDE.md — Documentation update
- Add Phase 5 completion section
- Update test count
- Note that Signal wiring was already complete before this phase
