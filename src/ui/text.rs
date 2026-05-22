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
}
