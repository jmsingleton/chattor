//! Tiny string-helpers shared across the UI layer.
//!
//! All truncation is done on character (codepoint) boundaries — never on
//! byte indices — so multi-byte characters (CJK, emoji, accented letters in
//! display names) never panic the renderer or cut a code point in half.
//! Width is approximated as character count, which is good enough for our
//! purposes (we'd need `unicode-width` to handle East-Asian double-width
//! glyphs precisely, but that's another dependency).

/// Truncate `s` to at most `max_chars` characters, appending the ellipsis
/// character `…` when truncation happens. Counting is by characters
/// (codepoints), so byte-level slicing of multi-byte text is impossible.
///
/// `max_chars` is the budget INCLUDING the ellipsis when truncation happens
/// — i.e. a 14-char budget yields up to 13 source chars + '…'. If the input
/// already fits, it's returned as-is.
pub fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    // Reserve one character for the ellipsis.
    let take = max_chars.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

/// Convert a byte-position cursor into a screen column for placing the OS
/// terminal cursor inside a bordered input box. Returns a column relative
/// to the box's outer x — i.e. `1 + chars_before_cursor`, clamped so the
/// cursor never escapes the right border. Counting is by char count of
/// the prefix, so multi-byte input doesn't displace it.
///
/// `box_width` is the outer width of the bordered input rect. The +1
/// accounts for the left border; the clamp accounts for the right.
pub fn input_cursor_column(input: &str, byte_cursor: usize, box_width: u16) -> u16 {
    let chars_before = input
        .get(..byte_cursor)
        .map(|s| s.chars().count())
        .unwrap_or(0) as u16;
    // box_width - 2 leaves one column for the right border.
    let max_col = box_width.saturating_sub(2);
    1 + chars_before.min(max_col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_when_fits() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncates_ascii_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello w…");
        assert_eq!(truncate_with_ellipsis("hello world", 4), "hel…");
    }

    #[test]
    fn handles_multibyte_chars_safely() {
        // 日本語 is 3 chars (9 bytes in UTF-8) — naive &s[..2] would panic.
        assert_eq!(truncate_with_ellipsis("日本語", 3), "日本語");
        assert_eq!(truncate_with_ellipsis("日本語テスト", 4), "日本語…");
    }

    #[test]
    fn handles_emoji() {
        assert_eq!(truncate_with_ellipsis("hi 👋 world", 5), "hi 👋…");
    }

    #[test]
    fn handles_zero_budget() {
        assert_eq!(truncate_with_ellipsis("anything", 0), "");
    }

    #[test]
    fn handles_one_char_budget() {
        // Budget = 1 means just the ellipsis (no source chars left).
        assert_eq!(truncate_with_ellipsis("hello", 1), "…");
    }

    #[test]
    fn input_cursor_column_starts_at_one() {
        // Empty input, the cursor sits just past the left border.
        assert_eq!(input_cursor_column("", 0, 10), 1);
    }

    #[test]
    fn input_cursor_column_advances_by_char_count() {
        // "hi" — cursor at byte 2 (end) → 1 (border) + 2 chars.
        assert_eq!(input_cursor_column("hi", 2, 10), 3);
        // After 日 (3 bytes, 1 char), column is 1 + 1 = 2.
        assert_eq!(input_cursor_column("日", 3, 10), 2);
    }

    #[test]
    fn input_cursor_column_clamps_at_right_border() {
        // 20-char input, box width 10 → max visible chars = 8 (10 - 2 borders).
        // Cursor at end should clamp at column 9 (1 + 8), not 21.
        let s = "abcdefghijklmnopqrst";
        assert_eq!(input_cursor_column(s, s.len(), 10), 9);
    }

    #[test]
    fn input_cursor_column_handles_off_boundary_cursor_gracefully() {
        // If cursor falls between chars (shouldn't happen post-fix, but
        // defensive), the get(..cursor) returns None and we count zero.
        let s = "日"; // 3 bytes
        // Byte cursor=1 is mid-codepoint.
        assert_eq!(input_cursor_column(s, 1, 10), 1);
    }
}
