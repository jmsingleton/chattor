# Data Rain Resolve — Tor Connection Animation

**Status:** Approved design (2026-06-24)
**Scope:** Replace the connecting/bootstrap animation in `src/ui/bootstrap.rs`. The failure screen, key handling, and bootstrap plumbing are unchanged.

## Problem

The current connecting screen (`src/ui/bootstrap.rs`) is built from **6 hand-drawn frames** (`connecting_frames()`) swapped every 3 ticks (~270ms each) via `BootstrapPhase::advance_tick`. It reads as cheap because:

1. **Discrete, jumpy motion** — the `░▒▓` pulse teleports in large chunks; 6 frames at 270ms looks like a flipbook.
2. **Two-tone nodes** — onions only toggle between `░░` (off) and `▓▓` (on); no glow, fade, or depth.
3. **Ignores reality** — Tor reports a real bootstrap percentage via `BootstrapUpdate::Progress(u8)`, captured into `Connecting { progress }`, then discarded (`_progress: u8` at `render_connecting`). The animation is decoration disconnected from actual state.
4. **Flat single color** — everything is `theme.accent`.
5. **Fixed ~63-char centered sprite** — does not adapt to terminal size.

## Concept

Full-screen **Tor-glyph "data rain"** falls behind a center band. As the **eased** Tor bootstrap progress climbs, a small **onion sprite resolves first** out of the noise, then the **`chattor` wordmark** locks in beneath it. On `Connected`, everything snaps bright for a beat, then transitions to the main UI. On `Failed`, the rain freezes and desaturates, then hands off to the existing failure screen.

The quality lever common to the whole redesign: render **procedurally** from `tick` + the **real progress %**, at ~20fps, using **color depth** (bright head vs. dim trail) instead of frame-swapping.

## Components

### 1. State model

`BootstrapPhase::Connecting` keeps `tick: u64` but replaces the hardcoded `frame: usize` with:

- `rain: RainField` — procedural rain state (per-column drops).
- `progress_shown: f32` — eased progress in `[0.0, 1.0]`, separate from the real target.

`progress` (the real Tor %) stays as the target input via `set_progress`.

`RainField` holds, per column, **0–2 drops** (deterministically assigned), each with head position `y: f32`, `speed: f32`, trail `length: u16`. It is advanced each tick and rendered to a character grid on demand — **no stored frames**.

**Determinism:** all randomness comes from a pure hash function `rng(col, row, salt) -> u32` (xorshift/splitmix style). No `rand` crate, no clock reads inside render. Frames are reproducible for a given `(tick, progress_shown, area)`, preserving unit-testability.

### 2. Rain mechanics

- **Variable density per column:** each column is deterministically assigned 0, 1, or 2 drops, so the field is irregular rather than a uniform wall. Empty ("half") columns leave gaps; double-drop columns (the two drops staggered in `y` and speed) add busier streaks. Each drop's head cell = brightest (`theme.accent`); trailing cells fade to `theme.fg_dim`. The single head glyph mutates as it falls; tail glyphs are stable per cell.
- **Charset (Tor-authentic):** the base32 `.onion` alphabet (`a`–`z`, `2`–`7`) plus hex digits, with rare full `.onion` tokens woven in.
- When a drop's head passes the bottom row, it respawns at the top with fresh deterministic `speed`/`length` derived from `rng(col, respawn_count, salt)`. A column's drop *count* stays fixed for the session (stable texture); only individual drops respawn.

### 3. Onion + wordmark resolve

- A reserved center region holds the onion sprite (above) and the `chattor` wordmark (below).
- Each logo cell has a deterministic `threshold ∈ (0.0, 1.0)` from `rng(cell_x, cell_y, salt)`. A cell **locks** when `progress_shown > threshold`.
- **Onion thresholds are lower than wordmark thresholds**, so the onion resolves first, then the word (the approved order).
- Unlocked logo cells render a bright, flickering rain glyph (teasing the shape). Locked cells render the true glyph in **bold `theme.accent`**.
- On `Connected`: force `progress_shown → 1.0`, lock all cells, brief bright flash, short hold, then `Done`.

### 4. Eased progress (the "always alive" rule)

Each tick:

```
target = real_progress as f32 / 100.0
progress_shown += max((target - progress_shown) * 0.08, 0.0008)
progress_shown = min(progress_shown, min(0.99, target + LEAD))
```

- Exponential ease toward the target so it never snaps.
- A small minimum forward creep keeps it drifting even when Tor stalls at a fixed percentage.
- A `LEAD` cap (≈`0.08`, i.e. 8%) lets `progress_shown` lead the real value slightly (keeps motion during stalls) but **never reaches 1.0 until the real `Connected` event fires** — it does not claim completion prematurely.

### 5. Layout & small-terminal fallback

- Rain fills the whole `f.size()`.
- The app already shows "Terminal too small" below 60×16 (`render_app` guard), so within the rendered range:
  - If vertical room is tight, drop the onion and resolve **wordmark-only**.
  - Otherwise render **onion + wordmark**.
- Bottom line keeps the existing cheeky rotating `status_messages()` (selected by `tick`).

### 6. Failure path

- On transition to `BootstrapPhase::Failed`, the rain **freezes and desaturates** to `fg_dim` for ~0.3s (a "signal lost" glitch beat).
- Then hands off to the **existing `render_failure` screen unchanged**. `[R]etry / [C]ontinue / [Q]uit` and `handle_bootstrap_key` are untouched.

### 7. Timing

- Animation steps ~every 50ms (≈20fps), up from ~270ms/frame. The bootstrap event loop step in `main.rs` is adjusted accordingly while keeping key polling responsive.

## Data flow

```
Tor bootstrap task ──BootstrapUpdate::Progress(u8)──> Connecting.progress (target)
                   ──BootstrapUpdate::Connected─────> force progress_shown=1.0 → flash → Done
                   ──BootstrapUpdate::Failed(s)─────> Failed { .. }  (glitch → render_failure)

every ~50ms tick:
  advance_tick → rain.step() + ease(progress_shown toward progress)
  render_connecting(tick, &rain, progress_shown, theme):
    grid = rain.render(area)            // base rain layer
    overlay onion+wordmark cells        // locked = true glyph (bold accent), unlocked = bright rain glyph
    draw bottom cheeky status line
```

## Testing strategy

- **Pure functions, directly tested:** `rng`, `RainField::step`, the easing function, and `threshold(cell)` are pure and deterministic.
- **Grid snapshots:** render to a fixed-size grid at known `(tick, progress_shown)` values and assert stable output — e.g. at `progress_shown = 0.0` no logo cells are locked; at `1.0` all are locked and show true glyphs.
- **Resolve order:** assert onion cells lock at lower `progress_shown` than wordmark cells.
- **Easing invariants:** `progress_shown` is monotonic non-decreasing, always `< 1.0` until `Connected`, and creeps forward when the target is static.
- **Existing tests stay green:** `handle_bootstrap_key`, failure-state transitions, `set_progress`, `done`, and phase-advance tests are preserved (the `frame` field is removed, so the few tests asserting on `frame` are updated to assert on the new rain/progress state instead).

## Scope / non-goals

- Only the **connecting** screen changes.
- The failure screen, key handling, and `BootstrapUpdate`/`BootstrapAction`/`BootstrapPhase` transition plumbing are kept — the change starts *using* the `progress` value that is currently ignored.
- **No new dependencies** (no `rand`); randomness is a local pure hash.
- No change to themes; the animation consumes existing `theme.accent` / `theme.fg` / `theme.fg_dim`.

## Theming

Theme-driven palette (native on all 7 themes):

- Rain trail: `theme.fg_dim`.
- Rain head (brightest cell per column): `theme.accent`.
- Resolved logo glyphs: bold `theme.accent`.
- Cheeky status line: `theme.fg_dim`.

Green only appears on `cyberpunk`; `dark` is cyan-tinted, `rose-pine` is soft mauve, `light` is dim-gray rain with a dark logo — all from existing theme colors.
