# Real Tor Hidden Service — Design

**Date:** 2026-02-13
**Phase:** 6, Chunk 2
**Status:** Approved

## Goal

Replace the stub hidden service with a real arti-hosted onion service, making chattor a genuine P2P Tor chat application where peers can receive incoming connections over Tor.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Tor backend | Arti embedded | Already depend on arti 2.0; self-contained, no external daemon |
| Onion key management | Arti-managed | Simpler and more reliable than importing Ed25519 keys |
| Friend codes | Encode actual .onion | Straightforward; arti assigns the address |
| Vanity mining | Remove entirely | .onion prefix no longer derived from Ed25519; mining is meaningless |
| Scope | Tor only | Typing indicators, online status, notifications deferred to Chunk 3 |

## Architecture

### Before (current)

```
TorClient (real arti) → connects to Tor network
HiddenService (stub)  → derives .onion from Ed25519, binds localhost TCP
Listener (TCP)        → accepts TcpStream on localhost
```

### After

```
TorClient (real arti) → connects to Tor network
HiddenService (real)  → launches arti onion service, gets real .onion address
Listener (Tor)        → accepts arti StreamRequests from rendezvous circuit
```

### Identity Model Change

| Component | Before | After |
|-----------|--------|-------|
| Ed25519 keypair | Identity + .onion derivation + signing | Signing + Signal Protocol only |
| .onion address | Derived from Ed25519 key | Assigned by arti (persistent via state dir) |
| Friend codes | Encode Ed25519-derived .onion | Encode arti-assigned .onion |
| Vanity mining | Mine Ed25519 key for .onion prefix | Removed |

## Components

### 1. Cargo.toml Changes

```toml
arti-client = { version = "0.39", features = ["onion-service-service", "onion-service-client"] }
tor-hsservice = "0.39"
```

- `onion-service-service`: Enables `TorClient::launch_onion_service()`
- `onion-service-client`: Enables connecting to `.onion` addresses (already works but explicit)
- `tor-hsservice`: Provides `handle_rend_requests()`, `StreamRequest`, `RendRequest`

### 2. Arti State Directory

Configure arti to use `~/.local/share/chattor/arti/` as its state directory so onion service keys persist across restarts. This means the `.onion` address is stable.

```rust
let config = TorClientConfigBuilder::default()
    .storage()
    .cache_dir(data_dir.join("arti/cache"))
    .state_dir(data_dir.join("arti/state"))
    // ...
    .build()?;
```

### 3. HiddenService Rewrite (`src/tor/hidden_service.rs`)

```rust
pub struct HiddenService {
    onion_address: String,
    service: Arc<RunningOnionService>,
}

impl HiddenService {
    pub async fn new(tor_client: &TorClient) -> Result<(Self, impl Stream<Item = RendRequest>)> {
        let config = OnionServiceConfigBuilder::default()
            .nickname("chattor".try_into()?)
            .build()?;

        let (service, rend_requests) = tor_client.inner()
            .launch_onion_service(config)?
            .expect("service not disabled");

        let onion_address = format!("{}.onion", service.onion_name()?);

        Ok((HiddenService { onion_address, service }, rend_requests))
    }

    pub fn address(&self) -> &str { &self.onion_address }
}
```

No more `local_addr()` — Tor handles routing internally.

### 4. Tor Listener (`src/net/listener.rs`)

New function alongside the existing TCP listener:

```rust
pub async fn listen_for_tor_connections(
    rend_requests: impl Stream<Item = RendRequest>,
    tx: mpsc::Sender<IncomingMessage>,
) -> Result<()> {
    let mut stream_requests = handle_rend_requests(rend_requests);

    while let Some(stream_req) = stream_requests.next().await {
        let stream_req = match stream_req {
            Ok(r) => r,
            Err(e) => { eprintln!("Stream request error: {}", e); continue; }
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            let mut stream = stream_req.accept().await?;
            let message = receive_message(&mut stream).await?;
            tx.send(IncomingMessage { message, remote_addr: "tor".into() }).await?;
            Ok::<_, anyhow::Error>(())
        });
    }
    Ok(())
}
```

The existing `listen_for_connections()` TCP function stays for `--debug` local testing.

### 5. App::init_tor() Changes

Updated flow:
1. Configure arti with persistent state directory
2. Bootstrap TorClient (same as before)
3. Launch onion service → get `(HiddenService, rend_request_stream)`
4. Get `.onion` address from service handle
5. Store `.onion` address in DB for persistence
6. Spawn Tor listener task processing rendezvous streams
7. Spawn queue processor task (same as before)

### 6. Remove Vanity Mining

Delete from `main.rs`:
- `MiningPrefixInput` first-run flow (lines 56-107)
- `run_mining_loop()` function
- `num_cpus()` helper

Replace with immediate identity generation:
```rust
if needs_identity {
    let identity = IdentityKeypair::generate()?;
    let mut app_lock = app.lock().await;
    identity.save_to_db(&app_lock.db)?;
    app_lock.identity = Some(identity);
}
```

Delete `src/ui/mining.rs` and `src/crypto/vanity.rs` entirely.
Remove `rayon` dependency from Cargo.toml.

### 7. Store .onion Address in DB

Add a `settings` table or use the existing `schema_version` pattern to persist the arti-assigned `.onion` address:

```sql
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

On startup after `init_tor()`, store the `.onion` address. On subsequent startups, the address will be the same (arti state dir persists the key).

## Testing Strategy

- **Unit tests**: HiddenService creation with mock/test config
- **Integration test**: Full two-instance test over real Tor (marked `#[ignore]`)
- **TCP fallback**: Keep TCP listener for `cargo test` (Tor not available in CI)
- **Existing tests**: All 213+ tests should still pass (they don't use real Tor)

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Arti onion service API is experimental | Pin to 0.39, test thoroughly. API has been stable across recent releases |
| Build time increase | Feature flags are additive; arti already in dependency tree |
| .onion address changes on fresh install | Expected — arti state dir persists across restarts |
| Compile errors from new arti types | Incremental approach: get it compiling first, then wire in |
