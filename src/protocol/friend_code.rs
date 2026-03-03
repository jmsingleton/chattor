use crate::error::{ChattorError, Result};

/// 256-word list for encoding bytes as human-readable words.
/// Each word maps to exactly one byte value (0-255), making encoding/decoding trivial.
/// Words chosen to be short, common, unambiguous, and phonetically distinct.
pub const WORDS: &[&str] = &[
    // 0x00-0x0F
    "ace", "act", "add", "age", "ago", "aid", "aim", "air", "ale", "all", "and", "ant", "any",
    "ape", "arc", "are", // 0x10-0x1F
    "ark", "arm", "art", "ash", "ask", "ate", "awe", "axe", "bad", "bag", "ban", "bar", "bat",
    "bay", "bed", "bee", // 0x20-0x2F
    "bet", "big", "bit", "bow", "box", "bud", "bug", "bus", "but", "buy", "cab", "cam", "can",
    "cap", "car", "cat", // 0x30-0x3F
    "cob", "cod", "cog", "cop", "cow", "cry", "cub", "cup", "cur", "cut", "dab", "dam", "day",
    "den", "dew", "did", // 0x40-0x4F
    "dig", "dim", "dip", "doe", "dog", "don", "dot", "dry", "dub", "dud", "due", "dug", "dun",
    "duo", "dye", "ear", // 0x50-0x5F
    "eat", "eel", "egg", "ego", "elk", "elm", "emu", "end", "era", "eve", "ewe", "eye", "fan",
    "far", "fat", "fax", // 0x60-0x6F
    "fed", "few", "fig", "fin", "fir", "fit", "fix", "fly", "foe", "fog", "for", "fox", "fry",
    "fun", "fur", "gag", // 0x70-0x7F
    "gal", "gap", "gas", "gem", "get", "gin", "gnu", "god", "got", "gum", "gun", "gut", "guy",
    "gym", "had", "ham", // 0x80-0x8F
    "has", "hat", "hay", "hen", "her", "hew", "hex", "hid", "him", "hip", "his", "hit", "hob",
    "hog", "hop", "hot", // 0x90-0x9F
    "how", "hub", "hue", "hug", "hum", "hut", "ice", "icy", "ilk", "ill", "imp", "ink", "inn",
    "ion", "ire", "irk", // 0xA0-0xAF
    "ivy", "jab", "jag", "jam", "jar", "jaw", "jay", "jet", "jig", "job", "jog", "jot", "joy",
    "jug", "jut", "keg", // 0xB0-0xBF
    "ken", "key", "kid", "kin", "kit", "lab", "lad", "lag", "lap", "law", "lay", "lea", "led",
    "leg", "let", "lid", // 0xC0-0xCF
    "lie", "lip", "lit", "log", "lot", "low", "lug", "lye", "mad", "man", "map", "mar", "mat",
    "maw", "may", "men", // 0xD0-0xDF
    "met", "mid", "mix", "mob", "mod", "mop", "mow", "mud", "mug", "nab", "nag", "nap", "net",
    "new", "nil", "nip", // 0xE0-0xEF
    "nit", "nod", "nor", "not", "now", "nun", "nut", "oak", "oar", "oat", "odd", "ode", "off",
    "oft", "ohm", "oil", // 0xF0-0xFF
    "old", "one", "opt", "orb", "ore", "our", "out", "owe", "owl", "own", "pad", "pal", "pan",
    "par", "paw", "pea",
];

/// Encode a .onion address as a human-readable friend code.
///
/// Extracts the 32-byte Ed25519 public key from the v3 .onion address and
/// encodes each byte as a word from the 256-word list.
///
/// Format: 8 groups of 4 words, groups separated by spaces, words by dashes.
/// Example: `ace-tiger-river-cloud flame-gem-shadow-lotus ...`
pub fn onion_to_friend_code(onion: &str) -> Result<String> {
    let pubkey = onion_to_pubkey(onion)?;
    let words: Vec<&str> = pubkey.iter().map(|b| WORDS[*b as usize]).collect();

    // Group into 8 blocks of 4 words
    let groups: Vec<String> = words.chunks(4).map(|chunk| chunk.join("-")).collect();

    Ok(groups.join(" "))
}

/// Decode a friend code back to a .onion address.
///
/// Looks up each word in the word list to recover the 32-byte public key,
/// then reconstructs the v3 .onion address (with checksum and version byte).
pub fn friend_code_to_onion(code: &str) -> Result<String> {
    let normalized = code.trim().to_lowercase();

    // Split on spaces and dashes to extract individual words
    let words: Vec<&str> = normalized
        .split([' ', '-'])
        .filter(|w| !w.is_empty())
        .collect();

    if words.len() != 32 {
        return Err(ChattorError::Crypto(format!(
            "Friend code must be 32 words, got {}",
            words.len()
        )));
    }

    // Convert words back to bytes
    let mut pubkey = [0u8; 32];
    for (i, word) in words.iter().enumerate() {
        let idx = WORDS.iter().position(|w| w == word).ok_or_else(|| {
            ChattorError::Crypto(format!("Unknown word in friend code: '{}'", word))
        })?;
        pubkey[i] = idx as u8;
    }

    pubkey_to_onion(&pubkey)
}

/// Validate that a string looks like a friend code (32 words from our word list).
#[allow(dead_code)]
pub fn validate_friend_code(code: &str) -> Result<()> {
    let normalized = code.trim().to_lowercase();
    let words: Vec<&str> = normalized
        .split([' ', '-'])
        .filter(|w| !w.is_empty())
        .collect();

    if words.len() != 32 {
        return Err(ChattorError::Crypto(format!(
            "Friend code must be 32 words, got {}",
            words.len()
        )));
    }

    for word in &words {
        if !WORDS.contains(word) {
            return Err(ChattorError::Crypto(format!(
                "Unknown word in friend code: '{}'",
                word
            )));
        }
    }

    Ok(())
}

/// Extract the 32-byte Ed25519 public key from a v3 .onion address.
pub(crate) fn onion_to_pubkey(onion: &str) -> Result<[u8; 32]> {
    let addr = onion
        .strip_suffix(".onion")
        .ok_or_else(|| ChattorError::Crypto("Missing .onion suffix".into()))?;

    if addr.len() != 56 {
        return Err(ChattorError::Crypto(format!(
            "Invalid onion address length: expected 56 chars, got {}",
            addr.len()
        )));
    }

    let decoded = base32::decode(
        base32::Alphabet::RFC4648 { padding: false },
        &addr.to_uppercase(),
    )
    .ok_or_else(|| ChattorError::Crypto("Invalid base32 in onion address".into()))?;

    if decoded.len() != 35 {
        return Err(ChattorError::Crypto(format!(
            "Invalid decoded length: expected 35 bytes, got {}",
            decoded.len()
        )));
    }

    // Verify version byte
    if decoded[34] != 0x03 {
        return Err(ChattorError::Crypto("Not a v3 onion address".into()));
    }

    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&decoded[..32]);
    Ok(pubkey)
}

/// Reconstruct a v3 .onion address from a 32-byte Ed25519 public key.
fn pubkey_to_onion(pubkey: &[u8; 32]) -> Result<String> {
    use sha3::{Digest, Sha3_256};

    // Compute checksum: SHA3-256(".onion checksum" || pubkey || version)[:2]
    let mut hasher = Sha3_256::new();
    hasher.update(b".onion checksum");
    hasher.update(pubkey);
    hasher.update([0x03u8]); // version
    let hash = hasher.finalize();
    let checksum = &hash[..2];

    // Build raw: pubkey(32) || checksum(2) || version(1)
    let mut raw = Vec::with_capacity(35);
    raw.extend_from_slice(pubkey);
    raw.extend_from_slice(checksum);
    raw.push(0x03);

    // Base32 encode (lowercase, no padding)
    let encoded = base32::encode(base32::Alphabet::RFC4648 { padding: false }, &raw);

    Ok(format!("{}.onion", encoded.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_list_has_256_entries() {
        assert_eq!(WORDS.len(), 256);
    }

    #[test]
    fn word_list_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for word in WORDS {
            assert!(seen.insert(word), "Duplicate word: {}", word);
        }
    }

    #[test]
    fn roundtrip_onion_to_code_and_back() {
        // Valid v3 .onion address (we need a real one with correct checksum)
        // Generate one from a known public key
        let pubkey = [0u8; 32]; // all-zero key for testing
        let onion = pubkey_to_onion(&pubkey).unwrap();

        let code = onion_to_friend_code(&onion).unwrap();
        let recovered = friend_code_to_onion(&code).unwrap();

        assert_eq!(onion, recovered);
    }

    #[test]
    fn roundtrip_with_nonzero_key() {
        let mut pubkey = [0u8; 32];
        for i in 0..32 {
            pubkey[i] = (i * 7 + 13) as u8;
        }
        let onion = pubkey_to_onion(&pubkey).unwrap();

        let code = onion_to_friend_code(&onion).unwrap();
        let recovered = friend_code_to_onion(&code).unwrap();

        assert_eq!(onion, recovered);
    }

    #[test]
    fn friend_code_format() {
        let pubkey = [0u8; 32];
        let onion = pubkey_to_onion(&pubkey).unwrap();
        let code = onion_to_friend_code(&onion).unwrap();

        // Should be 8 groups separated by spaces
        let groups: Vec<&str> = code.split(' ').collect();
        assert_eq!(groups.len(), 8, "Expected 8 groups, got: {}", code);

        // Each group should have 4 words separated by dashes
        for group in &groups {
            let words: Vec<&str> = group.split('-').collect();
            assert_eq!(
                words.len(),
                4,
                "Expected 4 words in group '{}', got {}",
                group,
                words.len()
            );
        }
    }

    #[test]
    fn friend_code_is_deterministic() {
        let pubkey = [42u8; 32];
        let onion = pubkey_to_onion(&pubkey).unwrap();

        let code1 = onion_to_friend_code(&onion).unwrap();
        let code2 = onion_to_friend_code(&onion).unwrap();
        assert_eq!(code1, code2);
    }

    #[test]
    fn validate_valid_code() {
        let pubkey = [0u8; 32];
        let onion = pubkey_to_onion(&pubkey).unwrap();
        let code = onion_to_friend_code(&onion).unwrap();
        assert!(validate_friend_code(&code).is_ok());
    }

    #[test]
    fn validate_rejects_wrong_word_count() {
        let result = validate_friend_code("ace act add");
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_unknown_words() {
        let code = std::iter::repeat("ace")
            .take(31)
            .chain(std::iter::once("zzz"))
            .collect::<Vec<_>>()
            .join(" ");
        let result = validate_friend_code(&code);
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_invalid_word() {
        let code = std::iter::repeat("ace")
            .take(31)
            .chain(std::iter::once("zzz"))
            .collect::<Vec<_>>()
            .join(" ");
        let result = friend_code_to_onion(&code);
        assert!(result.is_err());
    }

    #[test]
    fn decode_handles_mixed_separators() {
        // Friend codes can use dashes within groups and spaces between groups
        let pubkey = [0u8; 32];
        let onion = pubkey_to_onion(&pubkey).unwrap();
        let code = onion_to_friend_code(&onion).unwrap();

        // Also works with all spaces
        let all_spaces = code.replace('-', " ");
        let recovered = friend_code_to_onion(&all_spaces).unwrap();
        assert_eq!(onion, recovered);

        // Also works with all dashes
        let all_dashes = code.replace(' ', "-");
        let recovered = friend_code_to_onion(&all_dashes).unwrap();
        assert_eq!(onion, recovered);
    }

    #[test]
    fn case_insensitive_decode() {
        let pubkey = [0u8; 32];
        let onion = pubkey_to_onion(&pubkey).unwrap();
        let code = onion_to_friend_code(&onion).unwrap();
        let upper = code.to_uppercase();
        let recovered = friend_code_to_onion(&upper).unwrap();
        assert_eq!(onion, recovered);
    }

    #[test]
    fn all_byte_values_encode() {
        // Ensure every byte value maps to a unique word
        for b in 0u8..=255 {
            let word = WORDS[b as usize];
            assert!(!word.is_empty(), "Empty word for byte {}", b);
            let idx = WORDS.iter().position(|w| *w == word).unwrap();
            assert_eq!(idx, b as usize, "Word '{}' maps to wrong index", word);
        }
    }
}
