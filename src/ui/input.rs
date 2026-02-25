//! Char-index-based text input helpers.
//!
//! All cursor positions are expressed as **char indices** (not byte offsets),
//! which prevents panics when the input contains multi-byte UTF-8 characters
//! such as emoji or CJK ideographs.

/// Insert `c` at the char-index `cursor` position, then advance the cursor.
pub fn insert_char(input: &mut String, cursor: &mut usize, c: char) {
    let byte_pos = char_to_byte(input, *cursor);
    input.insert(byte_pos, c);
    *cursor += 1;
}

/// Delete the character immediately before the cursor (backspace).
pub fn backspace(input: &mut String, cursor: &mut usize) {
    if *cursor > 0 {
        *cursor -= 1;
        let byte_pos = char_to_byte(input, *cursor);
        input.remove(byte_pos);
    }
}

/// Move the cursor one character to the left.
pub fn move_left(cursor: &mut usize) {
    if *cursor > 0 {
        *cursor -= 1;
    }
}

/// Move the cursor one character to the right (clamped to input length in chars).
pub fn move_right(input: &str, cursor: &mut usize) {
    if *cursor < input.chars().count() {
        *cursor += 1;
    }
}

/// Convert a char index to the corresponding byte offset in `s`.
///
/// If `char_idx` is beyond the end of `s`, returns `s.len()`.
pub fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(s.len())
}

/// Truncate a string to at most `max_chars` characters, appending "…" if truncated.
pub fn truncate_display(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}…", truncated)
    } else {
        s.to_string()
    }
}

/// Truncate a string to at most `max_chars` characters, appending "..." if truncated.
pub fn truncate_display_dots(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

/// Split `s` at the given char index, returning `(before, after)`.
///
/// If `char_idx` is beyond the end of `s`, returns `(s, "")`.
pub fn split_at_char(s: &str, char_idx: usize) -> (&str, &str) {
    let byte_pos = char_to_byte(s, char_idx);
    s.split_at(byte_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── insert_char ──────────────────────────────────────────────

    #[test]
    fn insert_char_ascii() {
        let mut s = String::from("ac");
        let mut c = 1;
        insert_char(&mut s, &mut c, 'b');
        assert_eq!(s, "abc");
        assert_eq!(c, 2);
    }

    #[test]
    fn insert_char_at_start() {
        let mut s = String::from("bc");
        let mut c = 0;
        insert_char(&mut s, &mut c, 'a');
        assert_eq!(s, "abc");
        assert_eq!(c, 1);
    }

    #[test]
    fn insert_char_at_end() {
        let mut s = String::from("ab");
        let mut c = 2;
        insert_char(&mut s, &mut c, 'c');
        assert_eq!(s, "abc");
        assert_eq!(c, 3);
    }

    #[test]
    fn insert_char_empty_string() {
        let mut s = String::new();
        let mut c = 0;
        insert_char(&mut s, &mut c, 'x');
        assert_eq!(s, "x");
        assert_eq!(c, 1);
    }

    #[test]
    fn insert_char_emoji() {
        let mut s = String::new();
        let mut c = 0;
        // Emoji is 4 bytes in UTF-8
        insert_char(&mut s, &mut c, '\u{1F600}'); // U+1F600 = grinning face
        assert_eq!(s, "\u{1F600}");
        assert_eq!(c, 1);
        assert_eq!(s.len(), 4); // 4 bytes
        insert_char(&mut s, &mut c, 'a');
        assert_eq!(s, "\u{1F600}a");
        assert_eq!(c, 2);
    }

    #[test]
    fn insert_char_before_emoji() {
        let mut s = String::from("\u{1F600}");
        let mut c = 0;
        insert_char(&mut s, &mut c, 'a');
        assert_eq!(s, "a\u{1F600}");
        assert_eq!(c, 1);
    }

    #[test]
    fn insert_char_between_emoji() {
        let mut s = String::from("\u{1F600}\u{1F601}");
        let mut c = 1; // between the two emoji
        insert_char(&mut s, &mut c, 'x');
        assert_eq!(s, "\u{1F600}x\u{1F601}");
        assert_eq!(c, 2);
    }

    #[test]
    fn insert_char_cjk() {
        let mut s = String::from("你好");
        let mut c = 1; // between the two CJK chars
        insert_char(&mut s, &mut c, '世');
        assert_eq!(s, "你世好");
        assert_eq!(c, 2);
    }

    #[test]
    fn insert_char_mixed_content() {
        let mut s = String::from("a\u{1F600}b");
        let mut c = 2; // after emoji, before 'b'
        insert_char(&mut s, &mut c, '你');
        assert_eq!(s, "a\u{1F600}你b");
        assert_eq!(c, 3);
    }

    // ── backspace ────────────────────────────────────────────────

    #[test]
    fn backspace_ascii() {
        let mut s = String::from("abc");
        let mut c = 3;
        backspace(&mut s, &mut c);
        assert_eq!(s, "ab");
        assert_eq!(c, 2);
    }

    #[test]
    fn backspace_at_zero() {
        let mut s = String::from("abc");
        let mut c = 0;
        backspace(&mut s, &mut c);
        assert_eq!(s, "abc");
        assert_eq!(c, 0);
    }

    #[test]
    fn backspace_empty_string() {
        let mut s = String::new();
        let mut c = 0;
        backspace(&mut s, &mut c);
        assert_eq!(s, "");
        assert_eq!(c, 0);
    }

    #[test]
    fn backspace_emoji() {
        let mut s = String::from("\u{1F600}a");
        let mut c = 2; // after 'a'
        backspace(&mut s, &mut c);
        assert_eq!(s, "\u{1F600}");
        assert_eq!(c, 1);
        backspace(&mut s, &mut c);
        assert_eq!(s, "");
        assert_eq!(c, 0);
    }

    #[test]
    fn backspace_cjk() {
        let mut s = String::from("你好世界");
        let mut c = 4;
        backspace(&mut s, &mut c);
        assert_eq!(s, "你好世");
        assert_eq!(c, 3);
    }

    #[test]
    fn backspace_middle_of_emoji_sequence() {
        let mut s = String::from("a\u{1F600}b");
        let mut c = 2; // cursor after the emoji
        backspace(&mut s, &mut c);
        assert_eq!(s, "ab");
        assert_eq!(c, 1);
    }

    // ── move_left ────────────────────────────────────────────────

    #[test]
    fn move_left_normal() {
        let mut c = 5;
        move_left(&mut c);
        assert_eq!(c, 4);
    }

    #[test]
    fn move_left_at_zero() {
        let mut c = 0;
        move_left(&mut c);
        assert_eq!(c, 0);
    }

    // ── move_right ───────────────────────────────────────────────

    #[test]
    fn move_right_normal() {
        let mut c = 0;
        move_right("hello", &mut c);
        assert_eq!(c, 1);
    }

    #[test]
    fn move_right_at_end() {
        let mut c = 5;
        move_right("hello", &mut c);
        assert_eq!(c, 5); // stays
    }

    #[test]
    fn move_right_emoji() {
        let mut c = 0;
        let s = "\u{1F600}a";
        move_right(s, &mut c);
        assert_eq!(c, 1); // past the emoji (1 char, not 4 bytes)
        move_right(s, &mut c);
        assert_eq!(c, 2); // past 'a'
        move_right(s, &mut c);
        assert_eq!(c, 2); // clamped
    }

    #[test]
    fn move_right_cjk() {
        let mut c = 0;
        let s = "你好";
        move_right(s, &mut c);
        assert_eq!(c, 1);
        move_right(s, &mut c);
        assert_eq!(c, 2);
        move_right(s, &mut c);
        assert_eq!(c, 2); // clamped
    }

    #[test]
    fn move_right_empty() {
        let mut c = 0;
        move_right("", &mut c);
        assert_eq!(c, 0);
    }

    // ── char_to_byte ─────────────────────────────────────────────

    #[test]
    fn char_to_byte_ascii() {
        assert_eq!(char_to_byte("hello", 0), 0);
        assert_eq!(char_to_byte("hello", 3), 3);
        assert_eq!(char_to_byte("hello", 5), 5);
    }

    #[test]
    fn char_to_byte_emoji() {
        let s = "a\u{1F600}b";
        assert_eq!(char_to_byte(s, 0), 0); // 'a'
        assert_eq!(char_to_byte(s, 1), 1); // start of emoji
        assert_eq!(char_to_byte(s, 2), 5); // 'b' (1 + 4 bytes for emoji)
        assert_eq!(char_to_byte(s, 3), 6); // end
    }

    #[test]
    fn char_to_byte_cjk() {
        let s = "你好";
        assert_eq!(char_to_byte(s, 0), 0);
        assert_eq!(char_to_byte(s, 1), 3); // each CJK char is 3 bytes
        assert_eq!(char_to_byte(s, 2), 6);
    }

    #[test]
    fn char_to_byte_beyond_end() {
        assert_eq!(char_to_byte("abc", 10), 3);
    }

    #[test]
    fn char_to_byte_empty() {
        assert_eq!(char_to_byte("", 0), 0);
        assert_eq!(char_to_byte("", 5), 0);
    }

    // ── split_at_char ────────────────────────────────────────────

    #[test]
    fn split_at_char_ascii() {
        let (before, after) = split_at_char("hello", 2);
        assert_eq!(before, "he");
        assert_eq!(after, "llo");
    }

    #[test]
    fn split_at_char_start() {
        let (before, after) = split_at_char("hello", 0);
        assert_eq!(before, "");
        assert_eq!(after, "hello");
    }

    #[test]
    fn split_at_char_end() {
        let (before, after) = split_at_char("hello", 5);
        assert_eq!(before, "hello");
        assert_eq!(after, "");
    }

    #[test]
    fn split_at_char_emoji() {
        let s = "a\u{1F600}b";
        let (before, after) = split_at_char(s, 1);
        assert_eq!(before, "a");
        assert_eq!(after, "\u{1F600}b");

        let (before, after) = split_at_char(s, 2);
        assert_eq!(before, "a\u{1F600}");
        assert_eq!(after, "b");
    }

    #[test]
    fn split_at_char_cjk() {
        let s = "你好世界";
        let (before, after) = split_at_char(s, 2);
        assert_eq!(before, "你好");
        assert_eq!(after, "世界");
    }

    #[test]
    fn split_at_char_mixed() {
        let s = "hi\u{1F600}你好";
        let (before, after) = split_at_char(s, 3);
        assert_eq!(before, "hi\u{1F600}");
        assert_eq!(after, "你好");
    }

    #[test]
    fn split_at_char_empty() {
        let (before, after) = split_at_char("", 0);
        assert_eq!(before, "");
        assert_eq!(after, "");
    }

    #[test]
    fn split_at_char_beyond_end() {
        let (before, after) = split_at_char("abc", 10);
        assert_eq!(before, "abc");
        assert_eq!(after, "");
    }

    // ── round-trip scenarios ─────────────────────────────────────

    #[test]
    fn round_trip_type_and_delete_emoji() {
        let mut s = String::new();
        let mut c = 0;

        insert_char(&mut s, &mut c, '\u{1F600}');
        insert_char(&mut s, &mut c, 'a');
        insert_char(&mut s, &mut c, '你');
        assert_eq!(s, "\u{1F600}a你");
        assert_eq!(c, 3);

        backspace(&mut s, &mut c);
        assert_eq!(s, "\u{1F600}a");
        assert_eq!(c, 2);

        backspace(&mut s, &mut c);
        assert_eq!(s, "\u{1F600}");
        assert_eq!(c, 1);

        backspace(&mut s, &mut c);
        assert_eq!(s, "");
        assert_eq!(c, 0);
    }

    #[test]
    fn round_trip_navigate_and_insert() {
        let mut s = String::from("\u{1F600}\u{1F601}");
        let mut c = 2; // at end

        move_left(&mut c);
        assert_eq!(c, 1);

        insert_char(&mut s, &mut c, 'x');
        assert_eq!(s, "\u{1F600}x\u{1F601}");
        assert_eq!(c, 2);

        move_left(&mut c);
        move_left(&mut c);
        assert_eq!(c, 0);

        insert_char(&mut s, &mut c, 'a');
        assert_eq!(s, "a\u{1F600}x\u{1F601}");
        assert_eq!(c, 1);
    }

    #[test]
    fn split_at_char_matches_cursor_after_insert() {
        let mut s = String::new();
        let mut c = 0;

        insert_char(&mut s, &mut c, '你');
        insert_char(&mut s, &mut c, '\u{1F600}');
        insert_char(&mut s, &mut c, 'z');

        let (before, after) = split_at_char(&s, c);
        assert_eq!(before, "你\u{1F600}z");
        assert_eq!(after, "");

        move_left(&mut c); // now at 2
        let (before, after) = split_at_char(&s, c);
        assert_eq!(before, "你\u{1F600}");
        assert_eq!(after, "z");
    }

    // ── truncate_display ──────────────────────────────────────────

    #[test]
    fn truncate_ascii() {
        assert_eq!(truncate_display("abcdefghij", 5), "abcde…");
        assert_eq!(truncate_display("abcde", 5), "abcde");
        assert_eq!(truncate_display("abc", 5), "abc");
    }

    #[test]
    fn truncate_multibyte() {
        assert_eq!(truncate_display("你好世界再见", 4), "你好世界…");
        assert_eq!(truncate_display("😀😀😀", 2), "😀😀…");
    }

    #[test]
    fn truncate_dots_ascii() {
        assert_eq!(truncate_display_dots("abcdefghij", 5), "abcde...");
        assert_eq!(truncate_display_dots("abc", 5), "abc");
    }
}
