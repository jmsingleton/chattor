# Phase 5 Completion: Mining Integration & Crypto Hardening — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Wire the vanity mining UI into the first-run flow, fix the identity lifecycle bug, remove the Signal plaintext fallback, and clean up compiler warnings.

**Architecture:** `App.identity` becomes `Option<IdentityKeypair>`. On first run (identity is None), the mining prefix input screen appears before bootstrap. After mining completes or is skipped, identity is saved to DB and set on App. The Signal encrypt/decrypt methods error when no shared_secret exists instead of falling back to plaintext.

**Tech Stack:** Rust, ratatui (TUI), rayon (mining), ed25519-dalek, chacha20poly1305, tokio

---

## Task 1: Make `App.identity` optional

**Files:**
- Modify: `src/app.rs`

**Step 1: Change identity field type**

In `src/app.rs`, change line 19 from:

```rust
    pub identity: IdentityKeypair,
```

to:

```rust
    pub identity: Option<IdentityKeypair>,
```

**Step 2: Update `App::new()` to load from DB instead of generating**

Replace lines 43-45:

```rust
        // Generate or load identity
        // TODO: In future, load from database if exists
        let identity = IdentityKeypair::generate()?;
```

with:

```rust
        // Load identity from DB (None on first run — mining screen will create it)
        let identity = IdentityKeypair::load_from_db(&db);
```

**Step 3: Update `App::new_with_settings()` similarly**

Replace line 133:

```rust
        let identity = IdentityKeypair::generate()?;
```

with:

```rust
        let identity = IdentityKeypair::load_from_db(&db);
```

**Step 4: Update `init_tor()` to use existing identity**

Replace line 76:

```rust
        let identity = crate::crypto::IdentityKeypair::load_or_generate(&self.db)?;
```

with:

```rust
        let identity = self.identity.as_ref()
            .ok_or_else(|| crate::error::TorrentChatError::Crypto(
                "No identity keypair — complete mining or skip first".into()
            ))?;
```

**Step 5: Verify it compiles (expect errors in main.rs — that's Task 2)**

Run: `cargo check 2>&1 | head -40`
Expected: Errors in `main.rs` about `identity` being `Option` — these are fixed in Task 2.

**Step 6: Commit (will fix remaining errors in Task 2)**

```bash
git add src/app.rs
git commit -m "refactor: make App.identity optional for deferred mining"
```

---

## Task 2: Update all identity access sites in `main.rs`

**Files:**
- Modify: `src/main.rs`

There are 7 places in `main.rs` that access `app.identity` or `app_lock.identity`. All are in functions that only run after identity is guaranteed to exist (post-bootstrap), so `.as_ref().expect()` is correct.

**Step 1: Update line 517 (channel post signing)**

Change:

```rust
                            let signature = base64::encode(&app_lock.identity.sign(sign_data.as_bytes()).to_bytes());
```

to:

```rust
                            let signature = base64::encode(&app_lock.identity.as_ref().expect("identity set during init").sign(sign_data.as_bytes()).to_bytes());
```

**Step 2: Update line 687 (friend request creation)**

Change:

```rust
        &app.identity,
```

to:

```rust
        app.identity.as_ref().expect("identity set during init"),
```

**Step 3: Update line 724 (friend accept — generate PreKeyBundle)**

Change:

```rust
    let (bundle, private_keys) = PreKeyBundle::generate_real(&app.identity)?;
```

to:

```rust
    let identity = app.identity.as_ref().expect("identity set during init");
    let (bundle, private_keys) = PreKeyBundle::generate_real(identity)?;
```

**Step 4: Update line 734 (friend accept — sign)**

Change:

```rust
    let signature = app.identity.sign(data.as_bytes());
```

to:

```rust
    let signature = identity.sign(data.as_bytes());
```

(This reuses the `identity` binding from Step 3 — they're in the same function.)

**Step 5: Update line 753 (friend accept — create session)**

Change:

```rust
        &app.identity,
```

to:

```rust
        identity,
```

**Step 6: Update line 1100 (incoming accept — generate PreKeyBundle)**

Change:

```rust
    let (_, local_private) = PreKeyBundle::generate_real(&app.identity)?;
```

to:

```rust
    let identity = app.identity.as_ref().expect("identity set during init");
    let (_, local_private) = PreKeyBundle::generate_real(identity)?;
```

**Step 7: Update line 1107 (incoming accept — create session)**

Change:

```rust
        &app.identity,
```

to:

```rust
        identity,
```

**Step 8: Verify it compiles**

Run: `cargo check`
Expected: Compiles (may have warnings, those are fixed later).

**Step 9: Run tests**

Run: `cargo test -- --skip vanity`
Expected: All tests pass. The App tests in `src/app.rs` still work because `identity` is just `None` on fresh DB.

**Step 10: Commit**

```bash
git add src/main.rs
git commit -m "refactor: update all identity access sites for Option<IdentityKeypair>"
```

---

## Task 3: Wire mining into first-run flow

**Files:**
- Modify: `src/main.rs`

**Step 1: Add mining imports at the top of main.rs**

After line 27 (`use std::io;`), add:

```rust
use crate::crypto::IdentityKeypair;
```

**Step 2: Add the `num_cpus()` helper function**

At the bottom of `src/main.rs` (after the `collect_sync_requests` function), add:

```rust
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
```

**Step 3: Add the mining prefix input loop**

In `main()`, after the theme loading block (after line 44, before the terminal setup), add the first-run identity check. Actually — we need the terminal set up first so we can render the mining screen. So insert after the terminal setup (after line 51 `let mut terminal = Terminal::new(backend)?;`), BEFORE the bootstrap phase (before line 53 `// --- Bootstrap Phase ---`):

```rust
    // --- First-Run Identity Mining ---
    let needs_identity = {
        let app_lock = app.lock().await;
        app_lock.identity.is_none()
    };

    if needs_identity {
        let estimated_rate = 150_000.0 * num_cpus() as f64;
        let mut mining_state = AppState::MiningPrefixInput {
            prefix: String::new(),
            cursor: 0,
        };

        let mined_identity = loop {
            // Render prefix input screen
            if let AppState::MiningPrefixInput { ref prefix, cursor, .. } = mining_state {
                let p = prefix.clone();
                let c = cursor;
                terminal.draw(|f| {
                    ui::mining::render_prefix_input(f, &p, c, estimated_rate, &theme);
                })?;
            }

            // Handle input
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match mining_state.handle_key(key)? {
                        Some(AppAction::StartMining(prefix)) => {
                            // Transition to active mining
                            break run_mining_loop(&mut terminal, &prefix, &theme)?;
                        }
                        Some(AppAction::CancelMining) => {
                            // Skip — generate random identity
                            break IdentityKeypair::generate()?;
                        }
                        Some(AppAction::Quit) => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        };

        // Save the mined/generated identity
        let mut app_lock = app.lock().await;
        mined_identity.save_to_db(&app_lock.db)?;
        app_lock.identity = Some(mined_identity);
        drop(app_lock);
    }
```

**Step 4: Add the `run_mining_loop()` function**

At the bottom of `src/main.rs`, add:

```rust
/// Run the mining loop — blocks until a match is found, accepted, or cancelled.
fn run_mining_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    prefix: &str,
    theme: &ui::theme::Theme,
) -> Result<IdentityKeypair> {
    use crate::crypto::vanity::{start_mining, MiningProgress};

    let initial_progress = MiningProgress {
        attempts: 0,
        keys_per_sec: 0.0,
        best_prefix_len: 0,
        best_onion: None,
        found: false,
    };

    let (progress_tx, progress_rx) = tokio::sync::watch::channel(initial_progress);
    let (result_tx, mut result_rx) = tokio::sync::oneshot::channel();

    let _handle = start_mining(prefix, progress_tx, result_tx)?;
    let start_time = std::time::Instant::now();

    let mut found_keypair: Option<IdentityKeypair> = None;

    loop {
        let progress = progress_rx.borrow().clone();
        let elapsed = start_time.elapsed().as_secs_f64();

        // Check if result arrived
        if found_keypair.is_none() {
            if let Ok(keypair) = result_rx.try_recv() {
                found_keypair = Some(keypair);
            }
        }

        // Render fullscreen mining view
        {
            let p = prefix.to_string();
            let prog = progress.clone();
            terminal.draw(|f| {
                ui::mining::render_mining_fullscreen(f, &p, &prog, elapsed, theme);
            })?;
        }

        // Auto-accept on match found
        if progress.found {
            if let Some(kp) = found_keypair.take() {
                return Ok(kp);
            }
        }

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    crossterm::event::KeyCode::Enter => {
                        // Accept best match or generate random
                        if let Some(kp) = found_keypair.take() {
                            return Ok(kp);
                        }
                        return IdentityKeypair::generate();
                    }
                    crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc => {
                        // Cancel — generate random
                        return IdentityKeypair::generate();
                    }
                    _ => {}
                }
            }
        }
    }
}
```

**Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles cleanly.

**Step 6: Run tests**

Run: `cargo test -- --skip vanity`
Expected: All tests pass.

**Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire vanity mining into first-run flow with prefix input and mining loop"
```

---

## Task 4: Remove Signal plaintext fallback

**Files:**
- Modify: `src/crypto/signal.rs`

**Step 1: Write failing test for encrypt without session**

Add to the `#[cfg(test)] mod tests` block in `src/crypto/signal.rs`:

```rust
    #[test]
    fn test_encrypt_without_shared_secret_errors() {
        let bundle = PreKeyBundle::generate().unwrap();
        let mut session = SignalSession::from_prekey_bundle(
            "test.onion".into(),
            &bundle,
        ).unwrap();

        // Session has no real shared_secret — should error, not return plaintext
        let result = session.encrypt(b"hello");
        assert!(result.is_err(), "encrypt() should error without shared_secret");
    }

    #[test]
    fn test_decrypt_without_shared_secret_errors() {
        let bundle = PreKeyBundle::generate().unwrap();
        let mut session = SignalSession::from_prekey_bundle(
            "test.onion".into(),
            &bundle,
        ).unwrap();

        // Session has no real shared_secret — should error, not return plaintext
        let result = session.decrypt(b"some ciphertext");
        assert!(result.is_err(), "decrypt() should error without shared_secret");
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test crypto::signal::tests::test_encrypt_without_shared_secret_errors --lib`
Expected: FAIL — currently returns `Ok` with plaintext.

**Step 3: Replace encrypt() plaintext fallback**

In `src/crypto/signal.rs`, replace lines 324-328:

```rust
        } else {
            // For MVP compatibility, return plaintext with flag indicating if PreKey message
            let is_prekey_message = self.session_data.len() < 100;
            Ok((plaintext.to_vec(), is_prekey_message))
        }
```

with:

```rust
        } else {
            Err(TorrentChatError::Crypto(
                format!("No encryption session established for {}", self.remote_onion)
            ))
        }
```

**Step 4: Replace decrypt() plaintext fallback**

Replace lines 368-371:

```rust
        } else {
            // For MVP compatibility, return ciphertext as plaintext
            Ok(ciphertext.to_vec())
        }
```

with:

```rust
        } else {
            Err(TorrentChatError::Crypto(
                format!("No decryption session established for {}", self.remote_onion)
            ))
        }
```

**Step 5: Run tests to verify they pass**

Run: `cargo test crypto::signal --lib`
Expected: All 6 tests pass (4 existing + 2 new).

**Step 6: Commit**

```bash
git add src/crypto/signal.rs
git commit -m "feat: remove plaintext fallback — encrypt/decrypt require established session"
```

---

## Task 5: Fix compiler warnings

**Files:**
- Modify: `src/tor/hidden_service.rs`
- Modify: `src/ui/state.rs`
- Modify: `src/net/listener.rs`
- Modify: `src/net/sender.rs`
- Modify: `src/crypto/signal.rs`
- Modify: `src/ui/bootstrap.rs`
- Modify: `tests/e2e_messaging.rs`

**Step 1: Fix `hidden_service.rs` — unused `tor_client` parameter**

In `src/tor/hidden_service.rs`, line 15, change:

```rust
        tor_client: &TorClient,
```

to:

```rust
        _tor_client: &TorClient,
```

**Step 2: Fix `state.rs` — unused `publisher_onion` in ViewingChannel match**

In `src/ui/state.rs`, line 397, change:

```rust
            AppState::ViewingChannel { input, cursor, is_own, publisher_onion, channel_type, .. } => {
```

to:

```rust
            AppState::ViewingChannel { input, cursor, is_own, channel_type, .. } => {
```

**Step 3: Fix `state.rs` — unused import of `PendingFriendRequest` in test**

In `src/ui/state.rs`, line 858, change:

```rust
        use crate::db::queries::PendingFriendRequest;
```

to remove the line entirely (it's unused).

**Step 4: Fix `listener.rs` — unused `std::io` import**

In `src/net/listener.rs`, line 5, remove:

```rust
use std::io;
```

**Step 5: Fix `sender.rs` — unused `SignalSession` import**

In `src/net/sender.rs`, line 3, change:

```rust
use crate::crypto::{SessionStore, SignalSession};
```

to:

```rust
use crate::crypto::SessionStore;
```

**Step 6: Fix `sender.rs` — deprecated `base64::encode` (line 58)**

In `src/net/sender.rs`, line 58, change:

```rust
            signal_ciphertext: base64::encode(&ciphertext),
```

to:

```rust
            signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
```

And add at the top of the file (after existing use statements):

```rust
use base64::Engine;
```

**Step 7: Fix `sender.rs` — deprecated `base64::decode` (line 175, in test)**

In `src/net/sender.rs`, line 175, change:

```rust
        let decoded_ciphertext = base64::decode(&message.signal_ciphertext).unwrap();
```

to:

```rust
        let decoded_ciphertext = base64::engine::general_purpose::STANDARD.decode(&message.signal_ciphertext).unwrap();
```

**Step 8: Fix `signal.rs` — unused `remote_onion` in `from_bytes`**

In `src/crypto/signal.rs`, line 387, change:

```rust
    pub fn from_bytes(remote_onion: String, bytes: Vec<u8>) -> Result<Self> {
```

to:

```rust
    pub fn from_bytes(_remote_onion: String, bytes: Vec<u8>) -> Result<Self> {
```

**Step 9: Fix `bootstrap.rs` — unused `Color` import**

In `src/ui/bootstrap.rs`, line 4, change:

```rust
    style::{Color, Modifier, Style},
```

to:

```rust
    style::{Modifier, Style},
```

**Step 10: Fix `e2e_messaging.rs` — unused imports and variables**

In `tests/e2e_messaging.rs`:

Line 1 — remove unused `Arc`:
```rust
use std::sync::Arc;
```
→ Delete this line.

Line 10 — prefix unused `name`:
```rust
async fn create_test_instance(name: &str) -> (App, TempDir) {
```
→ Change to:
```rust
async fn create_test_instance(_name: &str) -> (App, TempDir) {
```

Line 41 — prefix unused `bob_onion`:
```rust
    let bob_onion = bob.onion_address.as_ref().unwrap();
```
→ Change to:
```rust
    let _bob_onion = bob.onion_address.as_ref().unwrap();
```

Line 42 — prefix unused `bob_friend_code`:
```rust
    let bob_friend_code = "test-1234-code-5678"; // Simplified for test
```
→ Change to:
```rust
    let _bob_friend_code = "test-1234-code-5678"; // Simplified for test
```

**Step 11: Also fix deprecated `base64::decode` in `receiver.rs`**

In `src/net/receiver.rs`, add at the top:

```rust
use base64::Engine;
```

Then update both `base64::decode(...)` calls to `base64::engine::general_purpose::STANDARD.decode(...)`.

And in `src/main.rs`, find all `base64::encode(...)` and `base64::decode(...)` calls and update them similarly. Add `use base64::Engine;` at the top.

**Step 12: Verify clean build**

Run: `cargo build 2>&1 | grep -c warning`
Expected: 0 warnings (or only warnings from dependencies).

**Step 13: Run all tests**

Run: `cargo test -- --skip vanity`
Expected: All tests pass.

**Step 14: Commit**

```bash
git add -A
git commit -m "fix: resolve all compiler warnings — unused imports, variables, deprecated base64 API"
```

---

## Task 6: Update CLAUDE.md documentation

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add Phase 5 section to Implementation Status**

After the Phase 4 section, add:

```markdown
### Phase 5: Crypto & Identity Hardening ✅
- Vanity .onion mining with rayon-based parallel workers
- First-run mining screen: prefix input with live ETA estimate
- Full-screen mining UI with animated ASCII art and live stats
- Auto-accept on prefix match, random fallback on skip/cancel
- Identity lifecycle: load from DB, defer to mining on first run
- Signal Protocol: removed plaintext fallback from encrypt/decrypt
- PreKeyBundle exchange and real X3DH already wired (from Phase 2b)
```

**Step 2: Update the test count**

Search for the test count mention and update it to reflect current count.

**Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with Phase 5 completion"
```

---

## Summary

| Task | Description | Files | Commits |
|------|-------------|-------|---------|
| 1 | Make identity Optional | `src/app.rs` | 1 |
| 2 | Update identity access sites | `src/main.rs` | 1 |
| 3 | Wire mining into first-run flow | `src/main.rs` | 1 |
| 4 | Remove Signal plaintext fallback | `src/crypto/signal.rs` | 1 |
| 5 | Fix compiler warnings | 7+ files | 1 |
| 6 | Update CLAUDE.md | `CLAUDE.md` | 1 |

**Total: 6 tasks, 6 commits**
