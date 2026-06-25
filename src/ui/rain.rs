//! Pure, deterministic data-rain engine for the Tor bootstrap screen.
//! No ratatui, no theme, no clock — every output is a function of its inputs,
//! so the whole engine is reproducible and unit-testable.

#![allow(dead_code)]

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
        head: -(r_head * rows_f),     // start above the top, staggered
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

    pub fn drop_count(&self) -> usize {
        self.drops.len()
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
        let col = (0..1000)
            .find(|c| is_address_column(*c))
            .expect("an address column exists");
        let read: String = (0..ONION_TOKEN.len() as u32)
            .map(|row| glyph_at(col, row, 0))
            .collect();
        let doubled = "\u{2e}onion.onion"; // ".onion.onion"
        assert!(doubled.contains(&read), "{read:?} not a rotation of .onion");
    }

    #[test]
    fn non_address_glyphs_are_base32() {
        let col = (0..1000)
            .find(|c| !is_address_column(*c))
            .expect("a normal column exists");
        for row in 0..50 {
            let ch = glyph_at(col, row, 0);
            assert!(
                RAIN_CHARS.contains(&(ch as u8)),
                "{ch:?} not in base32 charset"
            );
        }
    }

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
}
