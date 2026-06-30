# Data Rain Connection Animation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 6-frame flipbook Tor bootstrap animation with a procedural, theme-driven "data rain" screen where an onion sprite and the `chattor` wordmark resolve out of the noise as real Tor progress climbs.

**Architecture:** A new pure, deterministic rain engine (`src/ui/rain.rs`) produces a character grid with per-cell brightness levels from a `tick`. `bootstrap.rs` holds the rain state inside `BootstrapPhase::Connecting`, eases the real Tor progress, overlays a logo that locks in cell-by-cell, and renders the grid. `main.rs` drives it at ~20fps and adds brief connect-flash / failure-glitch beats.

**Tech Stack:** Rust, ratatui, crossterm. No new crates (randomness is a local pure hash, not the `rand` crate).

## Global Constraints

- **No new dependencies.** Randomness comes from a pure hash function `rng()` — no `rand`, no clock reads inside render. Frames must be reproducible for a given `(tick, progress_shown, area)`.
- **Theme-driven palette only.** Use existing `theme.accent` (rain head + resolved logo), `theme.fg` (rain body), `theme.fg_dim` (rain trail + cheeky status). No hardcoded colors.
- **Charset is Tor-authentic:** base32 `.onion` alphabet (`a`–`z`, `2`–`7`); ~1-in-7 columns spell `.onion` vertically.
- **Scope is the connecting screen only.** `handle_bootstrap_key`, the `BootstrapAction`/`BootstrapUpdate` enums, and the existing `render_failure` screen keep working unchanged.
- **Eased progress never claims completion early:** `progress_shown` stays `< 1.0` until the real `Connected` event fires.
- Run `cargo fmt` and `cargo clippy -- -D warnings` clean before each commit.

---

## File Structure

- **Create `src/ui/rain.rs`** — pure rain engine: `rng`, `glyph_at`, `RainField`, `Level`, `render_grid`. No ratatui, no theme — just data. Fully unit-testable.
- **Modify `src/ui/bootstrap.rs`** — `BootstrapPhase::Connecting` gains `rain` + `progress_shown`; `advance_tick` steps rain + eases; `render_connecting` paints the grid + logo overlay; add `ease_progress`, logo helpers, `render_failure_glitch`. Remove dead `connecting_frames()`.
- **Modify `src/ui/mod.rs`** — add `pub mod rain;`, export `render_failure_glitch` and the rain types needed by `main.rs`.
- **Modify `src/main.rs`** — bootstrap loop: resize rain to terminal each frame, render with new signature, ~50ms tick, connect-flash + failure-glitch beats.

---

### Task 1: Pure rain primitives — `rng`, `glyph_at`

**Files:**
- Create: `src/ui/rain.rs`
- Modify: `src/ui/mod.rs:2` (add `pub mod rain;`)
- Test: in `src/ui/rain.rs` `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `pub fn rng(a: u32, b: u32, salt: u32) -> u32`
  - `pub fn rng_unit(a: u32, b: u32, salt: u32) -> f32` (range `0.0..1.0`)
  - `pub fn is_address_column(col: u32) -> bool`
  - `pub fn glyph_at(col: u32, row: u32, tick: u64) -> char`
  - `pub const RAIN_CHARS: &[u8]`, `pub const ONION_TOKEN: &[u8]`

- [ ] **Step 1: Write the failing test**

Create `src/ui/rain.rs` with only this content:

```rust
//! Pure, deterministic data-rain engine for the Tor bootstrap screen.
//! No ratatui, no theme, no clock — every output is a function of its inputs,
//! so the whole engine is reproducible and unit-testable.

/// Base32 `.onion` alphabet — the authentic Tor address charset.
pub const RAIN_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
/// Token woven vertically into ~1-in-7 columns.
pub const ONION_TOKEN: &[u8] = b".onion";

const ADDR_SALT: u32 = 0x0505_0505;
const ADDR_COLUMN_EVERY: u32 = 7;
const GLYPH_MUTATE_TICKS: u64 = 3;

/// Deterministic integer hash (splitmix-style). Pure.
pub fn rng(a: u32, b: u32, salt: u32) -> u32 {
    let mut x = a
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add(b.wrapping_mul(0x85EB_CA77))
        .wrapping_add(salt.wrapping_mul(0xC2B2_AE35));
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// Deterministic float in `0.0..1.0`.
pub fn rng_unit(a: u32, b: u32, salt: u32) -> f32 {
    (rng(a, b, salt) % 1_000_000) as f32 / 1_000_000.0
}

/// True for the ~1-in-7 columns that spell `.onion` vertically.
pub fn is_address_column(col: u32) -> bool {
    rng(col, 0, ADDR_SALT) % ADDR_COLUMN_EVERY == 0
}

/// The glyph shown at a grid cell. Address columns spell `.onion`; other
/// columns show base32 chars that slowly mutate over time.
pub fn glyph_at(col: u32, row: u32, tick: u64) -> char {
    if is_address_column(col) {
        let phase = rng(col, 1, ADDR_SALT) % ONION_TOKEN.len() as u32;
        let idx = (row + phase) % ONION_TOKEN.len() as u32;
        return ONION_TOKEN[idx as usize] as char;
    }
    let bucket = (tick / GLYPH_MUTATE_TICKS) as u32;
    let r = rng(col, row, bucket.wrapping_add(1));
    RAIN_CHARS[(r as usize) % RAIN_CHARS.len()] as char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_varied() {
        assert_eq!(rng(1, 2, 3), rng(1, 2, 3));
        assert_ne!(rng(1, 2, 3), rng(1, 2, 4));
    }

    #[test]
    fn rng_unit_in_range() {
        for i in 0..1000 {
            let v = rng_unit(i, i * 7, 0x99);
            assert!((0.0..1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn address_columns_spell_onion_vertically() {
        // Find an address column, then read 6 consecutive rows: they must be a
        // rotation of ".onion".
        let col = (0..1000).find(|c| is_address_column(*c)).expect("an address column exists");
        let read: String = (0..ONION_TOKEN.len() as u32)
            .map(|row| glyph_at(col, row, 0))
            .collect();
        let doubled = "\u{2e}onion.onion"; // ".onion.onion"
        assert!(doubled.contains(&read), "{read:?} not a rotation of .onion");
    }

    #[test]
    fn non_address_glyphs_are_base32() {
        let col = (0..1000).find(|c| !is_address_column(*c)).expect("a normal column exists");
        for row in 0..50 {
            let ch = glyph_at(col, row, 0);
            assert!(RAIN_CHARS.contains(&(ch as u8)), "{ch:?} not in base32 charset");
        }
    }
}
```

Add to `src/ui/mod.rs` after line 1 (`pub mod app_ui;` … keep alphabetical-ish, place after `pub mod modals;`):

```rust
pub mod rain;
```

- [ ] **Step 2: Run test to verify it fails (then passes — these are pure functions defined alongside)**

Run: `cargo test ui::rain -- --nocapture`
Expected: COMPILES and PASSES (the functions are defined in Step 1; this task has no red phase because the engine is self-contained). If it does not compile, fix before continuing.

- [ ] **Step 3: Commit**

```bash
cargo fmt && cargo clippy -- -D warnings
git add src/ui/rain.rs src/ui/mod.rs
git commit -m "feat(ui): pure deterministic rain primitives (rng, glyph_at)"
```

---

### Task 2: `RainField` drops — variable density, step, respawn

**Files:**
- Modify: `src/ui/rain.rs`
- Test: `src/ui/rain.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `rng`, `rng_unit` (Task 1).
- Produces:
  - `pub struct RainField { pub cols: u16, pub rows: u16, /* private drops */ }`
  - `pub fn RainField::new(cols: u16, rows: u16) -> Self`
  - `pub fn RainField::resize(&mut self, cols: u16, rows: u16)`
  - `pub fn RainField::step(&mut self)`
  - `pub fn RainField::drop_count(&self) -> usize` (test accessor)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/ui/rain.rs`:

```rust
    #[test]
    fn density_is_variable_zero_to_two_per_column() {
        let field = RainField::new(200, 24);
        // Count drops per column; every column must have 0, 1, or 2.
        let mut per_col = vec![0u8; 200];
        for col in 0..200u16 {
            per_col[col as usize] = field.drops_in_col(col);
        }
        assert!(per_col.iter().all(|&n| n <= 2), "some column has >2 drops");
        assert!(per_col.iter().any(|&n| n == 0), "no empty (half) columns");
        assert!(per_col.iter().any(|&n| n == 2), "no double-drop columns");
    }

    #[test]
    fn step_advances_heads_downward() {
        let mut field = RainField::new(10, 24);
        let before: Vec<f32> = field.head_positions();
        field.step();
        let after: Vec<f32> = field.head_positions();
        for (b, a) in before.iter().zip(after.iter()) {
            assert!(a > b, "head did not advance: {b} -> {a}");
        }
    }

    #[test]
    fn drops_respawn_above_top_after_falling_off() {
        let mut field = RainField::new(4, 8);
        // Step far more than enough to push every drop off the bottom.
        for _ in 0..500 {
            field.step();
        }
        // After many steps no head has run away to +infinity; all are within
        // a sane band (respawn kept them recycling).
        for h in field.head_positions() {
            assert!(h < 8.0 + 64.0, "head ran away (no respawn): {h}");
        }
    }

    #[test]
    fn resize_rebuilds_only_on_change() {
        let mut field = RainField::new(10, 24);
        field.step();
        let snapshot = field.head_positions();
        field.resize(10, 24); // same size: no rebuild
        assert_eq!(field.head_positions(), snapshot);
        field.resize(20, 24); // changed: rebuild
        assert_eq!(field.cols, 20);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ui::rain::tests::density_is_variable_zero_to_two_per_column -- --nocapture`
Expected: FAIL to compile — `RainField` not defined.

- [ ] **Step 3: Write minimal implementation**

Add above the `tests` module in `src/ui/rain.rs`:

```rust
const COUNT_SALT: u32 = 0x0101_0101;
const HEAD_SALT: u32 = 0x0202_0202;
const SPEED_SALT: u32 = 0x0303_0303;
const LEN_SALT: u32 = 0x0404_0404;

#[derive(Debug, Clone, PartialEq)]
struct DropState {
    col: u16,
    head: f32,
    speed: f32,
    length: u16,
    respawns: u32,
}

/// Procedural rain state: 0–2 falling drops per column.
#[derive(Debug, Clone, PartialEq)]
pub struct RainField {
    pub cols: u16,
    pub rows: u16,
    drops: Vec<DropState>,
}

fn init_drop(col: u16, idx: u32, rows: u16, respawns: u32) -> DropState {
    let seed = idx ^ respawns.wrapping_mul(0x9);
    let r_head = rng_unit(col as u32, seed, HEAD_SALT);
    let r_speed = rng_unit(col as u32, seed, SPEED_SALT);
    let r_len = rng(col as u32, seed, LEN_SALT);
    let rows_f = rows.max(1) as f32;
    DropState {
        col,
        head: -(r_head * rows_f), // start above the top, staggered
        speed: 0.25 + r_speed * 0.75, // 0.25..1.0 rows/tick
        length: 4 + (r_len % (rows.max(8) as u32 / 2)) as u16, // 4..~rows/2
        respawns,
    }
}

impl RainField {
    pub fn new(cols: u16, rows: u16) -> Self {
        let mut drops = Vec::new();
        for col in 0..cols {
            let count = rng(col as u32, 0, COUNT_SALT) % 3; // 0, 1, or 2
            for idx in 0..count {
                drops.push(init_drop(col, idx, rows, 0));
            }
        }
        RainField { cols, rows, drops }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols != self.cols || rows != self.rows {
            *self = RainField::new(cols, rows);
        }
    }

    pub fn step(&mut self) {
        let rows = self.rows;
        let rows_f = rows as f32;
        for d in &mut self.drops {
            d.head += d.speed;
            if d.head - d.length as f32 > rows_f {
                let respawns = d.respawns.wrapping_add(1);
                let fresh = init_drop(d.col, d.col as u32, rows, respawns);
                d.head = fresh.head;
                d.speed = fresh.speed;
                d.length = fresh.length;
                d.respawns = respawns;
            }
        }
    }

    // --- test accessors ---
    #[cfg(test)]
    fn drops_in_col(&self, col: u16) -> u8 {
        self.drops.iter().filter(|d| d.col == col).count() as u8
    }

    #[cfg(test)]
    fn head_positions(&self) -> Vec<f32> {
        self.drops.iter().map(|d| d.head).collect()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test ui::rain -- --nocapture`
Expected: PASS (all Task 1 + Task 2 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -- -D warnings
git add src/ui/rain.rs
git commit -m "feat(ui): RainField drops with variable density, step, respawn"
```

---

### Task 3: `render_grid` — brightness levels

**Files:**
- Modify: `src/ui/rain.rs`
- Test: `src/ui/rain.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `RainField`, `glyph_at` (Tasks 1–2).
- Produces:
  - `pub enum Level { Empty, Trail, Body, Head }` (derives `Debug, Clone, Copy, PartialEq, Eq`)
  - `pub fn RainField::render_grid(&self, tick: u64) -> Vec<Vec<(char, Level)>>` — `rows` × `cols`, brighter drop wins per cell.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module:

```rust
    #[test]
    fn render_grid_has_area_dimensions() {
        let field = RainField::new(12, 6);
        let grid = field.render_grid(0);
        assert_eq!(grid.len(), 6, "rows");
        assert_eq!(grid[0].len(), 12, "cols");
    }

    #[test]
    fn drop_head_is_brightest_cell_in_its_trail() {
        // A single hand-built column: head at row 4, length 5.
        let mut field = RainField::new(1, 8);
        field.force_single_drop(0, 4.0, 1.0, 5);
        let grid = field.render_grid(0);
        // Row 4 = Head, rows 3..=2 = Body, rows below = Trail, others Empty.
        assert_eq!(grid[4][0].1, Level::Head);
        assert_eq!(grid[3][0].1, Level::Body);
        assert_eq!(grid[0][0].1, Level::Trail);
    }

    #[test]
    fn empty_cells_render_space() {
        let mut field = RainField::new(1, 8);
        field.force_single_drop(0, 1.0, 1.0, 2); // only rows 0,1 lit
        let grid = field.render_grid(0);
        assert_eq!(grid[7][0], (' ', Level::Empty));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ui::rain::tests::render_grid_has_area_dimensions -- --nocapture`
Expected: FAIL to compile — `render_grid` / `Level` / `force_single_drop` not defined.

- [ ] **Step 3: Write minimal implementation**

Add to `src/ui/rain.rs` (above `tests`):

```rust
/// Per-cell brightness produced by `render_grid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Empty,
    Trail,
    Body,
    Head,
}

fn level_rank(l: Level) -> u8 {
    match l {
        Level::Empty => 0,
        Level::Trail => 1,
        Level::Body => 2,
        Level::Head => 3,
    }
}

impl RainField {
    /// Compose all drops into a `rows × cols` grid of `(glyph, level)`.
    /// Where drops overlap, the brighter level wins.
    pub fn render_grid(&self, tick: u64) -> Vec<Vec<(char, Level)>> {
        let mut grid = vec![vec![(' ', Level::Empty); self.cols as usize]; self.rows as usize];
        for d in &self.drops {
            for k in 0..d.length {
                let row_f = d.head - k as f32;
                if row_f < 0.0 {
                    continue;
                }
                let row = row_f as usize;
                if row >= self.rows as usize {
                    continue;
                }
                let level = if k == 0 {
                    Level::Head
                } else if k <= 2 {
                    Level::Body
                } else {
                    Level::Trail
                };
                let cell = &mut grid[row][d.col as usize];
                if level_rank(level) >= level_rank(cell.1) {
                    let ch = glyph_at(d.col as u32, row as u32, tick);
                    *cell = (ch, level);
                }
            }
        }
        grid
    }

    #[cfg(test)]
    fn force_single_drop(&mut self, col: u16, head: f32, speed: f32, length: u16) {
        self.drops = vec![DropState { col, head, speed, length, respawns: 0 }];
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test ui::rain -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -- -D warnings
git add src/ui/rain.rs
git commit -m "feat(ui): render rain field to a leveled character grid"
```

---

### Task 4: Eased progress

**Files:**
- Modify: `src/ui/bootstrap.rs`
- Test: `src/ui/bootstrap.rs` `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `pub const PROGRESS_LEAD: f32 = 0.08;`
  - `pub fn ease_progress(shown: f32, target: f32) -> f32` — monotonic non-decreasing; result `< 1.0` unless caller forces it; may lead `target` by at most `PROGRESS_LEAD`, hard-capped at `0.99`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/ui/bootstrap.rs`:

```rust
    #[test]
    fn ease_never_exceeds_cap_below_one() {
        // Even when target is 1.0, easing alone never reaches 1.0.
        let mut shown = 0.0;
        for _ in 0..10_000 {
            shown = ease_progress(shown, 1.0);
        }
        assert!(shown <= 0.99, "easing reached completion on its own: {shown}");
        assert!(shown > 0.95, "easing stalled short: {shown}");
    }

    #[test]
    fn ease_creeps_forward_when_target_is_static() {
        // Tor stalled at 38%. Shown should still drift up (then plateau near lead cap).
        let target = 0.38;
        let a = ease_progress(0.38, target);
        let b = ease_progress(a, target);
        assert!(a > 0.38, "did not creep past a stalled target");
        assert!(b >= a, "not monotonic");
        // ...but capped so it never overruns the real value by much.
        let mut shown = 0.38;
        for _ in 0..1000 {
            shown = ease_progress(shown, target);
        }
        assert!(shown <= 0.38 + PROGRESS_LEAD + 1e-3, "led target too far: {shown}");
    }

    #[test]
    fn ease_is_monotonic_nondecreasing() {
        let mut shown = 0.0;
        for step in 0..100 {
            let target = (step as f32) / 100.0;
            let next = ease_progress(shown, target);
            assert!(next >= shown, "decreased: {shown} -> {next}");
            shown = next;
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap::tests::ease_never_exceeds_cap_below_one -- --nocapture`
Expected: FAIL to compile — `ease_progress` not defined.

- [ ] **Step 3: Write minimal implementation**

Add near the top of `src/ui/bootstrap.rs` (after the imports):

```rust
/// How far the displayed progress may lead the real Tor value during stalls.
pub const PROGRESS_LEAD: f32 = 0.08;

/// Ease `shown` toward `target` (both in `0.0..=1.0`).
///
/// Exponential approach plus a tiny minimum creep so the bar keeps drifting
/// even when Tor stalls at a fixed percentage. Hard-capped at `target +
/// PROGRESS_LEAD` and at `0.99`, so easing alone never claims completion —
/// only the real `Connected` event drives it to 1.0 (see `force_shown_full`).
pub fn ease_progress(shown: f32, target: f32) -> f32 {
    let step = ((target - shown) * 0.08).max(0.0008);
    let next = shown + step;
    let cap = (target + PROGRESS_LEAD).min(0.99);
    next.min(cap).max(shown)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test ui::bootstrap::tests::ease -- --nocapture`
Expected: PASS (3 ease tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -- -D warnings
git add src/ui/bootstrap.rs
git commit -m "feat(ui): eased bootstrap progress that stays alive during stalls"
```

---

### Task 5: Logo resolve — onion before wordmark

**Files:**
- Modify: `src/ui/bootstrap.rs`
- Test: `src/ui/bootstrap.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `rng_unit` (Task 1, via `crate::ui::rain`).
- Produces:
  - `pub fn logo_threshold(x: u32, y: u32, is_wordmark: bool) -> f32` — onion band `0.0..0.45`, wordmark band `0.55..0.90`.
  - `pub const ONION_ART: [&str; 5]`, `pub const WORDMARK: &str = "chattor"`.
  - A cell is *locked* (shows its true glyph) when `progress_shown > logo_threshold(..)`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/ui/bootstrap.rs`:

```rust
    #[test]
    fn onion_locks_before_wordmark() {
        // Every onion threshold is strictly below every wordmark threshold,
        // so the onion always resolves first.
        let mut max_onion = 0.0f32;
        let mut min_word = 1.0f32;
        for y in 0..8 {
            for x in 0..16 {
                max_onion = max_onion.max(logo_threshold(x, y, false));
                min_word = min_word.min(logo_threshold(x, y, true));
            }
        }
        assert!(max_onion < min_word, "onion {max_onion} not below wordmark {min_word}");
    }

    #[test]
    fn at_half_progress_onion_locked_wordmark_not() {
        let shown = 0.5f32;
        // All onion cells locked (max onion threshold < 0.45 < 0.5).
        for y in 0..8 {
            for x in 0..16 {
                assert!(shown > logo_threshold(x, y, false), "onion cell not locked at 0.5");
            }
        }
        // No wordmark cell locked (min wordmark threshold >= 0.55 > 0.5).
        for x in 0..16 {
            assert!(shown <= logo_threshold(x, 0, true), "wordmark cell locked too early");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap::tests::onion_locks_before_wordmark -- --nocapture`
Expected: FAIL to compile — `logo_threshold` not defined.

- [ ] **Step 3: Write minimal implementation**

Add to `src/ui/bootstrap.rs` (after `ease_progress`):

```rust
/// Small onion sprite that resolves above the wordmark.
pub const ONION_ART: [&str; 5] = [
    "  ▄██▄  ",
    "▄██████▄",
    "████████",
    " ▀████▀ ",
    "  ▀██▀  ",
];

/// Wordmark that resolves below the onion.
pub const WORDMARK: &str = "chattor";

/// The `progress_shown` value at which a logo cell locks to its true glyph.
/// Onion cells occupy a lower band (0.00..0.45) than wordmark cells
/// (0.55..0.90), so the onion always resolves before the wordmark.
pub fn logo_threshold(x: u32, y: u32, is_wordmark: bool) -> f32 {
    let salt: u32 = if is_wordmark { 0x0042_0000 } else { 0x0000_0411 };
    let r = crate::ui::rain::rng_unit(x, y, salt);
    if is_wordmark {
        0.55 + r * 0.35 // 0.55..0.90
    } else {
        r * 0.45 // 0.00..0.45
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test ui::bootstrap::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -- -D warnings
git add src/ui/bootstrap.rs
git commit -m "feat(ui): logo resolve thresholds (onion before wordmark)"
```

---

### Task 6: Rewire `BootstrapPhase::Connecting` to hold rain + eased progress

**Files:**
- Modify: `src/ui/bootstrap.rs:24-39` (enum), `:68-92` (`advance_tick`/`set_progress`), `:52-66` (`new`), and the `#[cfg(test)]` tests that assert on `frame`.
- Test: `src/ui/bootstrap.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `RainField` (Task 2), `ease_progress` (Task 4).
- Produces:
  - `BootstrapPhase::Connecting { progress: u8, progress_shown: f32, tick: u64, rain: RainField }`
  - `pub fn BootstrapPhase::resize_rain(&mut self, cols: u16, rows: u16)`
  - `pub fn BootstrapPhase::force_shown_full(&mut self)` — sets `progress_shown = 1.0`, `progress = 100` (the connect flash).
  - `pub fn BootstrapPhase::rain(&self) -> Option<&RainField>`
  - `advance_tick` now steps the rain and eases `progress_shown`.
  - `BootstrapPhase` derives `Debug, Clone, PartialEq` (drop `Eq` — `f32`/`RainField` are not `Eq`).

- [ ] **Step 1: Write the failing test**

Replace the existing `bootstrap_phase_starts_connecting`, `bootstrap_phase_advance_tick`, and `bootstrap_phase_frame_advances_every_3_ticks`, `fail_transitions_and_resets`, `set_progress_updates_connecting` tests (they assert on the removed `frame` / whole-struct equality). Add these instead:

```rust
    #[test]
    fn new_starts_connecting_at_zero() {
        let phase = BootstrapPhase::new();
        match phase {
            BootstrapPhase::Connecting { progress, progress_shown, tick, .. } => {
                assert_eq!(progress, 0);
                assert_eq!(tick, 0);
                assert_eq!(progress_shown, 0.0);
            }
            _ => panic!("expected Connecting"),
        }
    }

    #[test]
    fn advance_tick_increments_and_eases() {
        let mut phase = BootstrapPhase::new();
        phase.resize_rain(20, 10);
        phase.set_progress(50); // target 0.5
        phase.advance_tick();
        match phase {
            BootstrapPhase::Connecting { tick, progress_shown, .. } => {
                assert_eq!(tick, 1);
                assert!(progress_shown > 0.0, "did not ease toward target");
            }
            _ => panic!("expected Connecting"),
        }
    }

    #[test]
    fn force_shown_full_completes() {
        let mut phase = BootstrapPhase::new();
        phase.force_shown_full();
        match phase {
            BootstrapPhase::Connecting { progress, progress_shown, .. } => {
                assert_eq!(progress, 100);
                assert_eq!(progress_shown, 1.0);
            }
            _ => panic!("expected Connecting"),
        }
    }

    #[test]
    fn set_progress_ignored_in_failed_still_holds() {
        let mut phase = BootstrapPhase::new();
        phase.fail("error".to_string());
        phase.set_progress(50);
        assert!(matches!(phase, BootstrapPhase::Failed { .. }));
    }
```

Keep `done_transitions`, `advance_tick_on_done_is_noop`, and all `handle_bootstrap_key` / failure tests as-is. Delete `connecting_frames_exist_and_are_nonempty` (function is removed in this task) and `advance_tick_on_failed_state` may stay (Failed still has `tick`/`frame`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap -- --nocapture`
Expected: FAIL to compile — `progress_shown`, `resize_rain`, `force_shown_full` not defined; `frame` still referenced.

- [ ] **Step 3: Write minimal implementation**

In `src/ui/bootstrap.rs`:

Add import near the top:

```rust
use crate::ui::rain::RainField;
```

Change the enum derive and `Connecting` variant:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BootstrapPhase {
    Connecting {
        progress: u8,
        progress_shown: f32,
        tick: u64,
        rain: RainField,
    },
    Failed {
        error: String,
        frame: usize,
        tick: u64,
    },
    Done,
}
```

Replace `new`:

```rust
    pub fn new() -> Self {
        BootstrapPhase::Connecting {
            progress: 0,
            progress_shown: 0.0,
            tick: 0,
            rain: RainField::new(0, 0),
        }
    }
```

Replace `advance_tick` (Connecting arm) and add new methods:

```rust
    pub fn advance_tick(&mut self) {
        match self {
            BootstrapPhase::Connecting { tick, progress, progress_shown, rain } => {
                *tick += 1;
                rain.step();
                let target = *progress as f32 / 100.0;
                *progress_shown = ease_progress(*progress_shown, target);
            }
            BootstrapPhase::Failed { tick, frame, .. } => {
                *tick += 1;
                if *tick % 3 == 0 {
                    *frame += 1;
                }
            }
            BootstrapPhase::Done => {}
        }
    }

    /// Resize the rain field to the current terminal (rebuilds only on change).
    pub fn resize_rain(&mut self, cols: u16, rows: u16) {
        if let BootstrapPhase::Connecting { rain, .. } = self {
            rain.resize(cols, rows);
        }
    }

    /// Force full resolution for the connect-success flash beat.
    pub fn force_shown_full(&mut self) {
        if let BootstrapPhase::Connecting { progress, progress_shown, .. } = self {
            *progress = 100;
            *progress_shown = 1.0;
        }
    }

    /// Borrow the rain field, if currently connecting.
    pub fn rain(&self) -> Option<&RainField> {
        match self {
            BootstrapPhase::Connecting { rain, .. } => Some(rain),
            _ => None,
        }
    }
```

`set_progress` stays as-is (only sets `progress`). `fail`/`done` stay as-is.

Delete the `connecting_frames()` function (lines ~110-182) — it is now dead.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test ui::bootstrap -- --nocapture`
Expected: COMPILE FAILS in `render_connecting` (still uses removed `frame` and old signature) — that is fixed in Task 7. To verify *this* task in isolation, temporarily confirm the non-render tests: `cargo test ui::bootstrap::tests::new_starts_connecting_at_zero`. Proceed to Task 7 to restore a clean build before committing.

- [ ] **Step 5: Commit (combined with Task 7)**

Do not commit a non-building tree. Commit at the end of Task 7.

---

### Task 7: Rewrite `render_connecting` + add `render_failure_glitch`

**Files:**
- Modify: `src/ui/bootstrap.rs` (`render_connecting`, add `render_failure_glitch`, status cadence), `src/ui/mod.rs:14-17` (exports).
- Test: `src/ui/bootstrap.rs` `#[cfg(test)]` (a smoke render into a `TestBackend`).

**Interfaces:**
- Consumes: `RainField::render_grid` + `Level` (Task 3), `logo_threshold`/`ONION_ART`/`WORDMARK` (Task 5), `glyph_at` (Task 1).
- Produces:
  - `pub fn render_connecting(f: &mut Frame, rain: &RainField, tick: u64, progress_shown: f32, theme: &Theme)`
  - `pub fn render_failure_glitch(f: &mut Frame, rain: &RainField, theme: &Theme)` — frozen rain, all desaturated to `theme.fg_dim`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/ui/bootstrap.rs`:

```rust
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn render_connecting_smoke() {
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let theme = Theme::preset("dark");
        let mut rain = RainField::new(80, 24);
        for _ in 0..10 {
            rain.step();
        }
        // Must not panic and must fill the buffer.
        term.draw(|f| render_connecting(f, &rain, 30, 0.5, &theme)).unwrap();
    }

    #[test]
    fn render_failure_glitch_smoke() {
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let theme = Theme::preset("dark");
        let rain = RainField::new(80, 24);
        term.draw(|f| render_failure_glitch(f, &rain, &theme)).unwrap();
    }
```

Add at the top of the `tests` module if not already imported: `use crate::ui::theme::Theme;`

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap::tests::render_connecting_smoke -- --nocapture`
Expected: FAIL to compile — new `render_connecting` signature / `render_failure_glitch` not defined.

- [ ] **Step 3: Write minimal implementation**

Replace the entire `render_connecting` function in `src/ui/bootstrap.rs` with:

```rust
/// Map a rain level to a style for the given theme.
fn level_style(level: crate::ui::rain::Level, theme: &Theme) -> Style {
    use crate::ui::rain::Level;
    match level {
        Level::Head => Style::default().fg(theme.accent),
        Level::Body => Style::default().fg(theme.fg),
        Level::Trail => Style::default().fg(theme.fg_dim),
        Level::Empty => Style::default(),
    }
}

/// Build the logo overlay: for each terminal cell, `Some((char, locked))` if a
/// logo glyph belongs there. Onion sits above the wordmark, both centered.
/// `locked == true` → show the true glyph; `false` → tease with a rain glyph.
fn logo_overlay(
    cols: u16,
    rows: u16,
    progress_shown: f32,
    show_onion: bool,
) -> Vec<Vec<Option<(char, bool)>>> {
    let mut overlay = vec![vec![None; cols as usize]; rows as usize];
    let onion_h = if show_onion { ONION_ART.len() as u16 } else { 0 };
    let block_h = onion_h + if show_onion { 1 } else { 0 } + 1; // onion + gap + wordmark
    if rows < block_h + 2 || cols < WORDMARK.len() as u16 + 2 {
        return overlay; // too small; caller falls back / skips
    }
    let top = (rows - block_h) / 2;

    // Onion
    if show_onion {
        for (dy, line) in ONION_ART.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let w = chars.len() as u16;
            let left = (cols.saturating_sub(w)) / 2;
            for (dx, ch) in chars.iter().enumerate() {
                if *ch == ' ' {
                    continue;
                }
                let y = top + dy as u16;
                let x = left + dx as u16;
                let locked = progress_shown > logo_threshold(x as u32, y as u32, false);
                overlay[y as usize][x as usize] = Some((*ch, locked));
            }
        }
    }

    // Wordmark
    let word: Vec<char> = WORDMARK.chars().collect();
    let w = word.len() as u16;
    let left = (cols.saturating_sub(w)) / 2;
    let y = top + onion_h + if show_onion { 1 } else { 0 };
    for (dx, ch) in word.iter().enumerate() {
        let x = left + dx as u16;
        let locked = progress_shown > logo_threshold(x as u32, y as u32, true);
        overlay[y as usize][x as usize] = Some((*ch, locked));
    }
    overlay
}

/// Render the data-rain connecting screen.
pub fn render_connecting(
    f: &mut Frame,
    rain: &RainField,
    tick: u64,
    progress_shown: f32,
    theme: &Theme,
) {
    use crate::ui::rain::Level;
    let area = f.size();
    f.render_widget(Clear, area);

    let cols = area.width;
    let rows = area.height;
    let grid = rain.render_grid(tick);

    // Drop the onion on short terminals; resolve wordmark only.
    let show_onion = rows >= ONION_ART.len() as u16 + 4;
    let overlay = logo_overlay(cols, rows, progress_shown, show_onion);

    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for y in 0..rows as usize {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for x in 0..cols as usize {
            if let Some(Some((ch, locked))) = overlay.get(y).map(|r| r[x]) {
                if locked {
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    // Tease: show a bright rain glyph in the logo's footprint.
                    let g = crate::ui::rain::glyph_at(x as u32, y as u32, tick);
                    spans.push(Span::styled(g.to_string(), Style::default().fg(theme.accent)));
                }
            } else {
                let (gch, level) = grid
                    .get(y)
                    .and_then(|r| r.get(x))
                    .copied()
                    .unwrap_or((' ', Level::Empty));
                spans.push(Span::styled(gch.to_string(), level_style(level, theme)));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);

    // Cheeky rotating status, bottom-centered (slowed to ~2s at 20fps).
    let msgs = status_messages();
    let msg_idx = (tick / 40) as usize % msgs.len();
    let status_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + rows.saturating_sub(2),
        width: cols,
        height: 1,
    };
    let status = Paragraph::new(Line::from(Span::styled(
        msgs[msg_idx],
        Style::default().fg(theme.fg_dim),
    )))
    .alignment(Alignment::Center);
    f.render_widget(status, status_area);
}

/// Render frozen, desaturated rain for the ~0.3s "signal lost" beat before the
/// failure screen.
pub fn render_failure_glitch(f: &mut Frame, rain: &RainField, theme: &Theme) {
    let area = f.size();
    f.render_widget(Clear, area);
    let grid = rain.render_grid(0);
    let mut lines: Vec<Line> = Vec::with_capacity(area.height as usize);
    for y in 0..area.height as usize {
        let mut spans: Vec<Span> = Vec::with_capacity(area.width as usize);
        for x in 0..area.width as usize {
            let (ch, _lvl) = grid
                .get(y)
                .and_then(|r| r.get(x))
                .copied()
                .unwrap_or((' ', crate::ui::rain::Level::Empty));
            spans.push(Span::styled(ch.to_string(), Style::default().fg(theme.fg_dim)));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}
```

Update `src/ui/mod.rs:14-17` export block to add the glitch renderer and rain re-export:

```rust
pub use bootstrap::{
    handle_bootstrap_key, render_connecting, render_failure, render_failure_glitch,
    BootstrapAction, BootstrapPhase, BootstrapUpdate,
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test ui:: -- --nocapture`
Expected: PASS (rain + bootstrap render smoke tests). `render_connecting` old-signature callers in `main.rs` still break the *binary* build — fixed in Task 8. Library tests for `ui::` compile because `main.rs` is a separate target; if the workspace builds `main` for tests, proceed to Task 8 before the full `cargo test`.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy --lib -- -D warnings
git add src/ui/bootstrap.rs src/ui/mod.rs
git commit -m "feat(ui): data-rain connecting render + failure glitch beat"
```

---

### Task 8: Wire `main.rs` bootstrap loop

**Files:**
- Modify: `src/main.rs:306-414`
- Test: manual (`cargo run`) + full `cargo test`.

**Interfaces:**
- Consumes: `resize_rain`, `rain()`, `force_shown_full`, `render_connecting` (new signature), `render_failure_glitch` (Tasks 6–7).

- [ ] **Step 1: Replace the render arm and add flash/glitch state**

In `src/main.rs`, just before the `loop {` at line 312, add loop-local state:

```rust
    let mut connect_flash: Option<u8> = None; // countdown of bright "connected" frames
    let mut glitch_rain: Option<crate::ui::rain::RainField> = None;
    let mut glitch_ticks: u8 = 0;
    const FLASH_FRAMES: u8 = 8; // ~0.4s at 50ms
    const GLITCH_FRAMES: u8 = 6; // ~0.3s at 50ms
```

Replace the render `match &phase { ... }` block (lines 314-336) with:

```rust
        // Size the rain field to the terminal before drawing.
        let size = terminal.size()?;
        phase.resize_rain(size.width, size.height);

        match &phase {
            ui::BootstrapPhase::Connecting { tick, progress_shown, rain, .. } => {
                let t = *tick;
                let ps = *progress_shown;
                let rain = rain.clone();
                terminal.draw(|fr| {
                    ui::render_connecting(fr, &rain, t, ps, &theme);
                })?;
            }
            ui::BootstrapPhase::Failed { ref error, .. } => {
                if glitch_ticks > 0 {
                    if let Some(ref rain) = glitch_rain {
                        let rain = rain.clone();
                        terminal.draw(|fr| {
                            ui::render_failure_glitch(fr, &rain, &theme);
                        })?;
                        glitch_ticks -= 1;
                    }
                } else {
                    let err = error.clone();
                    terminal.draw(|fr| {
                        ui::render_failure(fr, &err, &theme);
                    })?;
                }
            }
            ui::BootstrapPhase::Done => {
                break;
            }
        }
```

- [ ] **Step 2: Handle the connect flash on `Connected`**

Replace the `Connected` arm (lines 353-356) with:

```rust
                ui::BootstrapUpdate::Connected => {
                    phase.force_shown_full();
                    connect_flash = Some(FLASH_FRAMES);
                }
```

And add, immediately after the `if bootstrap_rx.has_changed() { ... }` block (after line 361), a flash countdown:

```rust
        // Connect-success flash: hold the fully-resolved bright frame briefly.
        if let Some(n) = connect_flash {
            if n == 0 {
                phase.done();
                continue;
            }
            connect_flash = Some(n - 1);
        }
```

- [ ] **Step 3: Snapshot rain into the glitch beat on failure**

The two places that transition to `Failed` are the timeout (line 342) and the `Failed` update (line 358). Before each `phase.fail(...)`, capture the rain. Replace the timeout block (lines 339-344):

```rust
        if matches!(phase, ui::BootstrapPhase::Connecting { .. })
            && bootstrap_start.elapsed() > bootstrap_timeout
        {
            glitch_rain = phase.rain().cloned();
            glitch_ticks = GLITCH_FRAMES;
            phase.fail("connection timed out after 60 seconds".to_string());
            continue;
        }
```

Replace the `Failed(e)` arm (lines 357-359):

```rust
                ui::BootstrapUpdate::Failed(e) => {
                    glitch_rain = phase.rain().cloned();
                    glitch_ticks = GLITCH_FRAMES;
                    phase.fail(e);
                }
```

On `Retry` (line 384) the fresh `BootstrapPhase::new()` resets everything; also clear the beats. Replace line 384 (`phase = ui::BootstrapPhase::new();`) with:

```rust
                            phase = ui::BootstrapPhase::new();
                            connect_flash = None;
                            glitch_rain = None;
                            glitch_ticks = 0;
```

- [ ] **Step 4: Speed up the loop to ~20fps**

Change the key-poll interval at line 364 from `100` to `50`:

```rust
        if event::poll(Duration::from_millis(50))? {
```

- [ ] **Step 5: Build, test, and run**

Run: `cargo build`
Expected: clean build.

Run: `cargo test`
Expected: PASS (all ~406 existing + the new rain/bootstrap tests; no test references removed `frame`/`connecting_frames`).

Run: `cargo run`
Expected: full-screen theme-tinted rain, onion then `chattor` resolving as Tor connects; on success a brief bright hold then the main UI; if you `cargo run` with no network, after 60s a brief desaturated freeze then the failure screen with `[R]/[C]/[Q]`.

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy -- -D warnings
git add src/main.rs
git commit -m "feat(ui): drive data-rain bootstrap loop with flash + glitch beats"
```

---

## Self-Review

**1. Spec coverage:**
- Procedural render from tick + real progress → Tasks 1–3, 6. ✓
- Real progress used (was ignored) → Task 4 + Task 6 (`advance_tick` eases toward `progress`). ✓
- Onion-then-wordmark resolve → Task 5 (+ Task 7 overlay). ✓
- Eased progress, never completes early, creeps during stalls → Task 4. ✓
- Variable density (0–2 drops, half + double columns) → Task 2. ✓
- Tor-authentic charset + `.onion` columns → Task 1. ✓
- Theme-driven palette (head/body/trail) → Task 7 `level_style`. ✓
- Small-terminal wordmark-only fallback → Task 7 (`show_onion`). ✓
- Failure glitch beat then existing failure screen → Tasks 7–8. ✓
- Connect flash → Tasks 6–8. ✓
- ~20fps → Task 8 (50ms). ✓
- No new deps → all tasks (pure `rng`). ✓
- Tests preserved/updated → Task 6 (removes `frame`-coupled tests, adds field-level ones). ✓

**2. Placeholder scan:** None. Every code step shows the exact content to write.

**3. Type consistency:**
- `RainField` constructed `new(cols,rows)`, mutated via `resize`/`step`, read via `render_grid` and the `#[cfg(test)]` accessors — consistent across Tasks 2/3/6/7/8.
- `Level` enum identical in Tasks 3 and 7.
- `render_connecting(f, &RainField, tick: u64, progress_shown: f32, &Theme)` — defined in Task 7, called identically in Task 8.
- `progress_shown: f32` field name consistent in Tasks 4/6/7/8.
- `force_shown_full` / `resize_rain` / `rain()` defined in Task 6, used in Task 8. ✓
```
