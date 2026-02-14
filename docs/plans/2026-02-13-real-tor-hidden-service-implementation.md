# Real Tor Hidden Service — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the stub hidden service with a real arti-hosted onion service so chattor peers can discover and connect to each other over Tor.

**Architecture:** Enable arti-client's `onion-service-service` feature flag, rewrite `HiddenService` to call `TorClient::launch_onion_service()`, add a Tor rendezvous stream listener alongside the existing TCP listener, remove the vanity mining system entirely (arti manages .onion keys), and persist the arti-assigned .onion address in the database.

**Tech Stack:** arti-client 0.39 (onion-service-service feature), tor-hsservice 0.39 (handle_rend_requests, StreamRequest), tor-cell 0.39 (Connected message), Rust/tokio async

---

## Task 1: Add arti onion service dependencies

**Files:**
- Modify: `Cargo.toml:17-20`

This task adds the feature flags and crates needed for onion service hosting.

**Step 1: Update Cargo.toml**

Change the arti dependency block from:

```toml
arti = "2.0"
arti-client = "0.39"
tor-rtcompat = "0.39"
```

To:

```toml
arti = "2.0"
arti-client = { version = "0.39", features = ["onion-service-service", "onion-service-client"] }
tor-rtcompat = "0.39"
tor-hsservice = "0.39"
tor-cell = "0.39"
```

**Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors (many new crates will download)

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: add arti onion service dependencies (tor-hsservice, tor-cell)"
```

---

## Task 2: Remove vanity mining system

**Files:**
- Delete: `src/crypto/vanity.rs` (entire file)
- Delete: `src/ui/mining.rs` (entire file)
- Modify: `src/crypto/mod.rs:4` (remove `pub mod vanity;`)
- Modify: `src/ui/mod.rs:5` (remove `pub mod mining;`)
- Modify: `src/ui/state.rs:52-56` (remove `MiningPrefixInput` variant)
- Modify: `src/ui/state.rs:85-86` (remove `StartMining`, `CancelMining` actions)
- Modify: `src/ui/state.rs:464-500` (remove `MiningPrefixInput` key handler)
- Modify: `src/ui/state.rs` tests (remove 4 mining tests)
- Modify: `src/ui/app_ui.rs:169` (remove `MiningPrefixInput` footer entry)
- Modify: `src/main.rs:55-107` (replace mining flow with immediate identity generation)
- Modify: `src/main.rs:654-656` (remove `StartMining`/`CancelMining` match arm)
- Modify: `src/main.rs:1229-1306` (delete `run_mining_loop()` and `num_cpus()`)
- Modify: `Cargo.toml:31` (remove `rayon = "1.10"`)

This is a large deletion task. The vanity mining system is no longer meaningful because arti manages the .onion address key — you can't mine for a custom prefix.

**Step 1: Delete the vanity mining module**

Delete `src/crypto/vanity.rs` entirely.

Remove line 4 from `src/crypto/mod.rs`:
```rust
pub mod vanity;
```

**Step 2: Delete the mining UI module**

Delete `src/ui/mining.rs` entirely.

Remove line 5 from `src/ui/mod.rs`:
```rust
pub mod mining;
```

**Step 3: Remove MiningPrefixInput state and actions**

In `src/ui/state.rs`, remove the `MiningPrefixInput` variant from `AppState`:
```rust
    MiningPrefixInput {
        prefix: String,
        cursor: usize,
    },
```

Remove these two actions from `AppAction`:
```rust
    StartMining(String),           // prefix to mine
    CancelMining,
```

Remove the entire `AppState::MiningPrefixInput` match arm from `handle_key()` (lines 464-500).

Remove the `MiningPrefixInput` footer entry from `src/ui/app_ui.rs` line 169:
```rust
        AppState::MiningPrefixInput { .. } => vec![("Enter", "Start Mining"), ("Esc", "Skip")],
```

Remove the 4 mining tests from `src/ui/state.rs`:
- `test_mining_prefix_input_typing`
- `test_mining_prefix_rejects_invalid_chars`
- `test_mining_prefix_enter_starts`
- `test_mining_prefix_esc_cancels`

**Step 4: Replace first-run mining flow in main.rs**

Replace lines 55-107 of `src/main.rs` (the entire mining flow block) with:

```rust
    // --- First-Run Identity Generation ---
    let needs_identity = {
        let app_lock = app.lock().await;
        app_lock.identity.is_none()
    };

    if needs_identity {
        let identity = IdentityKeypair::generate()?;
        let mut app_lock = app.lock().await;
        identity.save_to_db(&app_lock.db)?;
        app_lock.identity = Some(identity);
        drop(app_lock);
    }
```

Remove the `StartMining`/`CancelMining` match arm from the main event loop (line ~654):
```rust
                        Some(AppAction::StartMining(_)) | Some(AppAction::CancelMining) => {
                            // Handled only in first-run mining flow, not during normal operation
                        }
```

Delete the `run_mining_loop()` function and `num_cpus()` helper (lines ~1229-1306).

Remove unused imports from `src/main.rs` that were only needed for mining:
- `use crate::crypto::IdentityKeypair;` — **keep this** (still used for identity generation)

**Step 5: Remove rayon dependency**

In `Cargo.toml`, remove:
```toml
rayon = "1.10"
```

**Step 6: Verify it compiles and tests pass**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass (mining tests are gone, other tests unaffected)

**Step 7: Commit**

```bash
git add -A
git commit -m "feat: remove vanity mining system (arti manages .onion keys)"
```

---

## Task 3: Add app_settings table for .onion address persistence

**Files:**
- Modify: `src/db/schema.rs`
- Modify: `src/db/connection.rs` (add v8 migration)
- Modify: `src/db/queries.rs` (add get/set setting helpers)
- Test: inline in `src/db/queries.rs`

The arti-assigned .onion address needs to persist across restarts. Add a simple key-value `app_settings` table and a schema v8 migration.

**Step 1: Write failing tests**

Add to the bottom of the `#[cfg(test)]` block in `src/db/queries.rs`:

```rust
    #[test]
    fn test_get_set_app_setting() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        // No setting yet
        assert_eq!(get_app_setting(&db, "onion_address").unwrap(), None);

        // Set a value
        set_app_setting(&db, "onion_address", "abc123.onion").unwrap();
        assert_eq!(
            get_app_setting(&db, "onion_address").unwrap(),
            Some("abc123.onion".to_string())
        );

        // Update existing
        set_app_setting(&db, "onion_address", "xyz789.onion").unwrap();
        assert_eq!(
            get_app_setting(&db, "onion_address").unwrap(),
            Some("xyz789.onion".to_string())
        );
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test db::queries::tests::test_get_set_app_setting 2>&1 | tail -5`
Expected: FAIL — function not found

**Step 3: Add schema and migration**

In `src/db/schema.rs`, add this constant (alongside the existing schema constants):

```rust
pub const CREATE_APP_SETTINGS: &str = "
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";
```

In `src/db/connection.rs`, in the `initialize()` method, add a v8 migration after the existing v7 migration call. Find the pattern of existing migrations (they look like `migrate_to_v7(&conn)?;`) and add:

```rust
        migrate_to_v8(&conn)?;
```

Then add the migration function (following the existing pattern):

```rust
fn migrate_to_v8(conn: &rusqlite::Connection) -> Result<()> {
    let version: i64 = conn.query_row(
        "SELECT version FROM schema_version", [], |row| row.get(0)
    ).unwrap_or(0);

    if version < 8 {
        conn.execute_batch(crate::db::schema::CREATE_APP_SETTINGS)?;
        conn.execute("UPDATE schema_version SET version = 8", [])?;
    }
    Ok(())
}
```

Also add the `CREATE_APP_SETTINGS` to the initial `CREATE_TABLES` execution block so fresh databases get it too.

**Step 4: Add query helpers**

In `src/db/queries.rs`, add:

```rust
/// Get an application setting by key
pub fn get_app_setting(db: &Database, key: &str) -> Result<Option<String>> {
    let conn = db.connection();
    let result = conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    );
    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(TorrentChatError::Database(format!("Failed to get setting: {}", e))),
    }
}

/// Set an application setting (insert or update)
pub fn set_app_setting(db: &Database, key: &str, value: &str) -> Result<()> {
    let conn = db.connection();
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (key, value),
    ).map_err(|e| TorrentChatError::Database(format!("Failed to set setting: {}", e)))?;
    Ok(())
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test db::queries::tests::test_get_set_app_setting 2>&1 | tail -5`
Expected: PASS

Run: `cargo test 2>&1 | tail -5`
Expected: All tests pass

**Step 6: Commit**

```bash
git add src/db/schema.rs src/db/connection.rs src/db/queries.rs
git commit -m "feat: add app_settings table with v8 migration for .onion persistence"
```

---

## Task 4: Rewrite HiddenService with real arti onion service

**Files:**
- Modify: `src/tor/hidden_service.rs` (complete rewrite)
- Modify: `src/tor/client.rs` (add state directory configuration)

This is the core task. The `HiddenService` struct gets rewritten to actually launch an arti onion service using `TorClient::launch_onion_service()`.

**Step 1: Update TorClient to use persistent state directory**

In `src/tor/client.rs`, change `TorClient::new()` to accept a data directory for persistent state:

```rust
use crate::error::{Result, TorrentChatError};
use arti_client::{TorClient as ArtiTorClient, config::TorClientConfigBuilder};
use std::sync::Arc;
use std::path::Path;

/// Tor client for managing connections
pub struct TorClient {
    client: Arc<ArtiTorClient<tor_rtcompat::PreferredRuntime>>,
}

impl TorClient {
    /// Create and bootstrap a new Tor client with persistent state directory
    pub async fn new_with_data_dir(data_dir: &Path) -> Result<Self> {
        let state_dir = data_dir.join("arti");
        let cache_dir = data_dir.join("arti-cache");

        // Ensure directories exist
        std::fs::create_dir_all(&state_dir)
            .map_err(|e| TorrentChatError::Tor(format!("Failed to create arti state dir: {}", e)))?;
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| TorrentChatError::Tor(format!("Failed to create arti cache dir: {}", e)))?;

        let config = TorClientConfigBuilder::from_directories(&state_dir, &cache_dir)
            .build()
            .map_err(|e| TorrentChatError::Tor(format!("Failed to build Tor config: {}", e)))?;

        let client = ArtiTorClient::create_bootstrapped(config)
            .await
            .map_err(|e| TorrentChatError::Tor(format!("Failed to bootstrap Tor: {}", e)))?;

        Ok(TorClient {
            client: Arc::new(client),
        })
    }

    /// Create and bootstrap with default config (for backward compat / tests)
    pub async fn new() -> Result<Self> {
        let config = arti_client::TorClientConfig::default();
        let client = ArtiTorClient::create_bootstrapped(config)
            .await
            .map_err(|e| TorrentChatError::Tor(format!("Failed to bootstrap Tor: {}", e)))?;

        Ok(TorClient {
            client: Arc::new(client),
        })
    }

    /// Check if Tor client is bootstrapped
    pub fn is_bootstrapped(&self) -> bool {
        true
    }

    /// Get reference to inner arti client
    pub fn inner(&self) -> &Arc<ArtiTorClient<tor_rtcompat::PreferredRuntime>> {
        &self.client
    }
}
```

**Step 2: Rewrite HiddenService**

Replace the entire `src/tor/hidden_service.rs` with:

```rust
use crate::error::{Result, TorrentChatError};
use crate::tor::client::TorClient;
use std::sync::Arc;
use tor_hsservice::{RunningOnionService, RendRequest};
use tor_hsservice::config::OnionServiceConfigBuilder;
use tor_hsservice::HsNickname;

/// Tor hidden service for receiving connections
pub struct HiddenService {
    onion_address: String,
    _service: Arc<RunningOnionService>,
}

impl HiddenService {
    /// Launch a real arti onion service.
    ///
    /// Returns the HiddenService handle and a stream of incoming rendezvous
    /// requests that the caller should feed into `listen_for_tor_connections()`.
    pub async fn launch(
        tor_client: &TorClient,
    ) -> Result<(Self, impl futures::Stream<Item = RendRequest>)> {
        let nickname: HsNickname = "chattor".parse()
            .map_err(|e| TorrentChatError::Tor(format!("Invalid service nickname: {}", e)))?;

        let mut builder = OnionServiceConfigBuilder::default();
        builder.nickname(nickname);
        let config = builder.build()
            .map_err(|e| TorrentChatError::Tor(format!("Failed to build onion service config: {}", e)))?;

        let (service, rend_requests) = tor_client.inner()
            .launch_onion_service(config)
            .map_err(|e| TorrentChatError::Tor(format!("Failed to launch onion service: {}", e)))?
            .ok_or_else(|| TorrentChatError::Tor("Onion service is disabled in config".into()))?;

        // Get the .onion address — may need a brief wait for descriptor publication
        let onion_address = service.onion_address()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "pending.onion".to_string());

        Ok((
            HiddenService {
                onion_address,
                _service: service,
            },
            rend_requests,
        ))
    }

    /// Get .onion address
    pub fn address(&self) -> &str {
        &self.onion_address
    }
}

#[cfg(test)]
mod tests {
    // Real onion service tests require a running Tor network connection
    // and take 30+ seconds. Mark them #[ignore].

    #[tokio::test]
    #[ignore]
    async fn test_hidden_service_launch() {
        let tor_client = crate::tor::client::TorClient::new().await.unwrap();
        let result = super::HiddenService::launch(&tor_client).await;
        assert!(result.is_ok());
        let (hs, _stream) = result.unwrap();
        assert!(hs.address().contains(".onion") || hs.address() == "pending.onion");
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo check 2>&1 | tail -10`
Expected: May have warnings about unused imports but no errors.

If there are type errors with `RendRequest`, `RunningOnionService`, `OnionServiceConfigBuilder`, or `HsNickname`, check the exact import paths — they should all be in `tor_hsservice`. If `launch_onion_service` isn't found, verify the `onion-service-service` feature is enabled on `arti-client`.

**Step 4: Run existing tests**

Run: `cargo test 2>&1 | tail -10`
Expected: All non-ignored tests pass. Some tests that used `HiddenService::new()` may fail — fix callers to use `HiddenService::launch()` or mark them `#[ignore]`.

**Step 5: Commit**

```bash
git add src/tor/client.rs src/tor/hidden_service.rs
git commit -m "feat: rewrite HiddenService with real arti onion service hosting"
```

---

## Task 5: Add Tor rendezvous listener

**Files:**
- Modify: `src/net/listener.rs` (add `listen_for_tor_connections`)
- Modify: `src/net/mod.rs` (re-export new function)

This adds a new listener function that accepts incoming connections from the Tor onion service, using the existing `receive_message()` framing.

**Step 1: Add the Tor listener function**

In `src/net/listener.rs`, add these imports at the top:

```rust
use tor_hsservice::{handle_rend_requests, RendRequest};
use tor_cell::relaycell::msg::Connected;
use futures::StreamExt;
```

Then add the new function after the existing `listen_for_connections`:

```rust
/// Listen for incoming connections via Tor onion service rendezvous.
///
/// Takes the RendRequest stream from `HiddenService::launch()` and
/// converts each into a framed message using the same protocol as TCP.
pub async fn listen_for_tor_connections(
    rend_requests: impl futures::Stream<Item = RendRequest> + Send + 'static,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    let stream_requests = handle_rend_requests(rend_requests);
    futures::pin_mut!(stream_requests);

    while let Some(stream_request) = stream_requests.next().await {
        let tx = tx.clone();
        tokio::spawn(async move {
            match stream_request.accept(Connected::new_empty()).await {
                Ok(mut data_stream) => {
                    match crate::net::framing::receive_message(&mut data_stream).await {
                        Ok(message) => {
                            let _ = tx.send(IncomingMessage {
                                message,
                                remote_addr: "tor-rendezvous".to_string(),
                            }).await;
                        }
                        Err(e) => {
                            eprintln!("Tor connection framing error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to accept Tor stream: {}", e);
                }
            }
        });
    }

    Ok(())
}
```

**Step 2: Update net/mod.rs**

Add the new function to the re-exports in `src/net/mod.rs`:

```rust
pub use listener::{listen_for_connections, listen_for_tor_connections, IncomingMessage};
```

**Step 3: Add futures dependency if not already present**

Check if `futures` is in Cargo.toml. If not, add:

```toml
futures = "0.3"
```

**Step 4: Verify it compiles**

Run: `cargo check 2>&1 | tail -10`
Expected: No errors. The `handle_rend_requests` function and `Connected::new_empty()` should resolve from the new dependencies.

**Step 5: Commit**

```bash
git add src/net/listener.rs src/net/mod.rs Cargo.toml Cargo.lock
git commit -m "feat: add Tor rendezvous stream listener for incoming onion connections"
```

---

## Task 6: Wire up real onion service in App::init_tor()

**Files:**
- Modify: `src/app.rs:66-125` (rewrite `init_tor()`)
- Modify: `src/main.rs` (update identity/onion address flow)

This wires everything together: `init_tor()` launches the real onion service, spawns the Tor listener, and persists the .onion address.

**Step 1: Rewrite init_tor()**

Replace the `init_tor()` method in `src/app.rs` with:

```rust
    /// Initialize Tor client and hidden service
    pub async fn init_tor(&mut self) -> Result<()> {
        if self.tor_client.is_some() {
            return Ok(()); // Already initialized
        }

        // Bootstrap Tor client with persistent state directory
        let client = crate::tor::client::TorClient::new_with_data_dir(
            &self.settings.data_dir
        ).await?;

        // Launch real onion service
        let (hidden_service, rend_requests) =
            crate::tor::hidden_service::HiddenService::launch(&client).await?;

        let onion_address = hidden_service.address().to_string();

        // Persist the .onion address in database
        crate::db::queries::set_app_setting(&self.db, "onion_address", &onion_address)?;

        // Spawn Tor rendezvous listener for incoming connections
        let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(100);
        tokio::spawn(async move {
            if let Err(e) = crate::net::listener::listen_for_tor_connections(rend_requests, msg_tx).await {
                eprintln!("Tor listener task error: {}", e);
            }
        });
        self.incoming_message_rx = Some(msg_rx);

        // Spawn queue processor task (sends ProcessQueue command every 30 seconds)
        let (queue_cmd_tx, queue_cmd_rx) = tokio::sync::mpsc::channel(10);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                if queue_cmd_tx.send(crate::app::QueueCommand::ProcessQueue).await.is_err() {
                    break;
                }
            }
        });
        self.queue_command_rx = Some(queue_cmd_rx);

        // Store in app state
        self.tor_client = Some(Arc::new(client));
        self.hidden_service = Some(hidden_service);
        self.onion_address = Some(onion_address);

        Ok(())
    }
```

**Step 2: Load persisted .onion address on startup**

In `src/app.rs`, update `App::new()` to try loading the persisted onion address:

After the `let identity = ...` line, add:

```rust
        // Load previously-persisted .onion address (set during Tor init)
        let onion_address = crate::db::queries::get_app_setting(&db, "onion_address")
            .unwrap_or(None);
```

And in the `Ok(App { ... })` block, change `onion_address: None` to:

```rust
            onion_address,
```

Do the same in `new_with_settings()`.

**Step 3: Remove unused `IdentityKeypair` parameter from HiddenService**

The old `HiddenService::new()` took an `IdentityKeypair` to derive the .onion address. Since that function no longer exists, check that `init_tor()` no longer references `self.identity` for the hidden service (it shouldn't after the rewrite above).

Remove the now-unused `identity` variable from the init flow if present.

**Step 4: Fix compilation — update app.rs test**

The `test_init_tor` test in `src/app.rs` uses the old `HiddenService` API. Update it — since it calls `init_tor()` which now requires a real Tor network, mark it `#[ignore]` or adjust expectations:

```rust
    #[tokio::test]
    #[ignore] // Requires real Tor network
    async fn test_init_tor() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let mut app = App::new().unwrap();

        // Identity is None on fresh DB — init_tor now requires identity for signing, not for .onion
        // But bootstrap itself should still work
        let identity = IdentityKeypair::generate().unwrap();
        app.identity = Some(identity);

        let result = app.init_tor().await;
        assert!(result.is_ok());
        assert!(app.tor_client.is_some());
        assert!(app.onion_address.is_some());
    }
```

**Step 5: Verify compilation**

Run: `cargo check 2>&1 | tail -10`
Expected: No errors

**Step 6: Run tests**

Run: `cargo test 2>&1 | tail -10`
Expected: All non-ignored tests pass

**Step 7: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: wire real arti onion service into App::init_tor()"
```

---

## Task 7: Update CLAUDE.md and clean up

**Files:**
- Modify: `CLAUDE.md`
- Modify: `src/tor/hidden_service.rs` (if any remaining fixups)
- Modify: `src/net/pool.rs` (can be deleted if still present — was gutted in Chunk 1)

**Step 1: Update CLAUDE.md**

Update the following sections in `CLAUDE.md`:

- **Tor Integration section**: Remove "STUBS" label. Update to reflect real hidden service hosting via arti.
- **Phase 6 status**: Note Chunk 2 (Real Tor Hidden Service) complete.
- **What's Stubbed**: Remove the `HiddenService::new()` entry since it's now real.
- **Test count**: Update to current count.
- **Key Files**: Note `src/tor/hidden_service.rs` now hosts a real onion service.

**Step 2: Remove dead files if any remain**

If `src/net/pool.rs` still exists and only contains a comment, delete it and remove `pub mod pool;` from `src/net/mod.rs`.

If `src/net/queue_processor.rs` still exists and only contains a comment, delete it and remove `pub mod queue_processor;` from `src/net/mod.rs`.

**Step 3: Final test run**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass

Run: `cargo clippy 2>&1 | tail -10`
Expected: No warnings (or only pre-existing ones)

**Step 4: Commit**

```bash
git add -A
git commit -m "chore: update CLAUDE.md for real Tor hidden service, clean up dead files"
```

---

## Important Notes for the Implementer

### Arti API Gotchas

1. **`onion_address()` may return `None` initially** — the address might not be available until the service publishes its descriptor. If you get `None`, either wait and poll, or use a placeholder and update later.

2. **`handle_rend_requests()` auto-accepts all rendezvous requests.** This is fine for chattor since we handle authentication at the protocol layer (friend requests, Signal sessions).

3. **`Connected::new_empty()`** is the correct constructor for accepting onion service streams. Import from `tor_cell::relaycell::msg::Connected`.

4. **The framing functions are already generic** — `send_message` and `receive_message` work with any `AsyncRead + AsyncWrite + Unpin` stream, so they work with arti's `DataStream` without changes.

5. **Arti state directory** persists onion service keys across restarts. The .onion address will be the same each time the app starts, as long as the state directory exists.

### What NOT to change

- **Signal Protocol** — completely unrelated, don't touch `src/crypto/signal.rs`
- **Friend code encoding** — the algorithm stays the same, just the .onion source changes
- **Message queue** — unchanged, still queues to .onion addresses
- **Database schema** — only the v8 migration and `app_settings` table are new
- **UI** — no visual changes needed (the header already shows the .onion address from `app.onion_address`)

### Testing approach

- Most existing tests don't use real Tor and will continue to pass unchanged
- New Tor tests should be marked `#[ignore]` since they require network access
- The TCP listener tests in `src/net/listener.rs` stay as-is (they test TCP, not Tor)
- Run `cargo test` after every task to catch breakage early
