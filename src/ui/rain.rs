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
}
