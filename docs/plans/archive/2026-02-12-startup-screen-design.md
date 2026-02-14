# Startup Screen Design

## Problem

The app freezes on startup while waiting for Tor to bootstrap. The user sees nothing responsive — no indication that the app is working. When Tor fails to connect, there's no guidance on what went wrong or how to fix it.

## Solution

A dedicated bootstrap screen with animated Unicode pixel art that plays immediately on launch, communicating connection progress. On failure, a styled error screen with troubleshooting tips and clear action keys.

## Architecture: Dedicated Bootstrap Loop (Approach A)

### State

New `BootstrapPhase` enum in `src/ui/bootstrap.rs`:

```rust
enum BootstrapPhase {
    Connecting { progress: u8, frame: usize, tick: u64 },
    Failed { error: String, frame: usize, tick: u64 },
    Done,
}
```

Communication via `tokio::watch` channel:

```rust
enum BootstrapUpdate {
    Progress(u8),       // 0-100
    Connected,
    Failed(String),     // error message
}
```

### Flow

1. App launches → immediately renders first animation frame (zero delay)
2. Tor init spawns in background via `tokio::spawn`, sends updates through `tokio::watch`
3. Bootstrap mini-loop ticks at 100ms, advancing animation every 300ms (~3rd tick)
4. On `Connected` → brief "all lit" flash (~500ms) → transition to main UI event loop
5. On `Failed` → fizzle animation (3 frames) → failure screen
6. On 60s timeout with no response → treat as failure
7. Failure screen waits for user input:
   - `[R]` Retry — re-spawn Tor init, return to connecting animation
   - `[C]` Continue — enter main UI with `tor_client: None` (offline mode)
   - `[Q]` Quit — clean terminal restore and exit

### Key Fix

The current freeze is caused by lock contention: `main.rs` spawns Tor init holding `Arc<Mutex<App>>`, which blocks the main event loop from rendering. The new design eliminates this — Tor init communicates only via a `tokio::watch` channel, and the bootstrap loop never contends for the App lock.

## Visual Design

### Style

Retro Unicode block art using half-block characters (`▀▄█▌▐`) and shading (`░▒▓`). Fits naturally in a terminal while having more visual fidelity than classic ASCII.

### Connecting Animation: Relay Path

Three onion sprites as relay nodes with an animated signal pulse traveling between them:

```
       ▄▌                      ▄▌                      ▄▌
      ▄██                     ▄██                     ▄██
    ▄█▓▓▓█▄                 ▄█▓▓▓█▄                 ▄█▓▓▓█▄
    █▒░░░▒█══░░▒▒▓▓██══════ █▒░░░▒█══════░░▒▒▓▓██══ █▒░░░▒█
     ▀█▓▓█▀                  ▀█▓▓█▀                  ▀█▓▓█▀
       ▀▀                      ▀▀                      ▀▀
      you                    relay                    exit
```

Animation behavior:
- Signal pulse (`░▒▓█`) slides left-to-right along the path between onions
- Each onion "lights up" (shading shifts brighter) as the pulse reaches it
- Pulse loops continuously while connecting
- Rotating cheeky status messages below:
  - "Peeling onion layers..."
  - "Negotiating with relays..."
  - "Building circuits in the dark..."
  - "Routing through the underground..."
  - "Almost there, patience is a virtue..."

### Success Transition

All three onions fully lit, pulse completes, brief flash, then transition to main UI.

### Failure Animation

Pulse stops mid-path and fizzles out (`█▓▒░···`), onions dim over ~3 frames, then static failure screen appears.

### Failure Screen

Single dim onion (lightest shading — "powered down"), inline troubleshooting, and action keys:

```
                        ▄▌
                       ██
                     ▄█░░░█▄
                     █░░░░░█
                      ▀█░░█▀
                        ▀▀

              connection failed :(

  the tor network couldn't be reached.

  * check your internet connection
  * your firewall may be blocking outbound traffic
  * tor network may be temporarily unreachable
  * try a different network — some block tor

  docs: https://github.com/chattor/chattor/wiki/tor

            [R] Retry   [C] Continue   [Q] Quit
```

### Terminal Size

Art designed for 60+ columns wide. On narrower terminals, fall back to a compact single-onion version with text-only status.

## Files Changed

| File | Change |
|------|--------|
| `src/ui/bootstrap.rs` | Replace current unused `render_bootstrap()` with full animation system (frame data, connecting renderer, failure renderer, `BootstrapPhase`, `BootstrapUpdate`) |
| `src/main.rs` | Add bootstrap mini-loop before main event loop. Create `tokio::watch` channel, spawn Tor init, run bootstrap until resolved, then enter existing main loop |
| `src/ui/mod.rs` | Export new bootstrap types |

## Files NOT Changed

- `src/app.rs` — `App::new()` and `init_tor()` unchanged
- `src/ui/state.rs` — `AppState` enum untouched (bootstrapping is a pre-app concern)
- All other UI/rendering code — no changes
