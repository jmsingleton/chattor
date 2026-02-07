use crate::error::{Result, TorrentChatError};
use crate::protocol::friend_code::validate_friend_code;
use sha2::{Sha256, Digest};

/// Convert .onion address to friend code
/// Format: word-NNNN-word-NNNN
pub fn onion_to_friend_code(onion: &str) -> Result<String> {
    // Validate .onion format (56 chars + .onion)
    if !onion.ends_with(".onion") || onion.len() != 62 {
        return Err(TorrentChatError::Crypto(
            "Invalid .onion address format".to_string()
        ));
    }

    // Hash the .onion to get deterministic 4 bytes
    let mut hasher = Sha256::new();
    hasher.update(onion.as_bytes());
    let hash = hasher.finalize();

    // Take first 4 bytes
    let bytes = &hash[0..4];

    // Convert to friend code format
    // Use existing friend_code generation logic with these bytes as seed
    // For now, simplified version:
    let word1_idx = (bytes[0] as usize) % crate::protocol::friend_code::WORDS.len();
    let num1 = u16::from_be_bytes([bytes[0], bytes[1]]) % 9000 + 1000;
    let word2_idx = (bytes[2] as usize) % crate::protocol::friend_code::WORDS.len();
    let num2 = u16::from_be_bytes([bytes[2], bytes[3]]) % 9000 + 1000;

    let word1 = crate::protocol::friend_code::WORDS[word1_idx];
    let word2 = crate::protocol::friend_code::WORDS[word2_idx];

    Ok(format!("{}-{}-{}-{}", word1, num1, word2, num2))
}

/// Convert friend code to .onion address
/// This requires a lookup table or reverse mapping
/// For Phase 2 MVP, we'll store the mapping in memory
pub fn friend_code_to_onion(friend_code: &str, mapping: &std::collections::HashMap<String, String>) -> Result<String> {
    validate_friend_code(friend_code)?;

    mapping.get(friend_code)
        .cloned()
        .ok_or_else(|| TorrentChatError::Crypto("Friend code not found".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onion_to_friend_code() {
        let onion = "3g2upl4pq6kufc4m2kyd56yz3b4qbeteqbqndzvt3sp6hhfjdkhqiiqd.onion";
        let code = onion_to_friend_code(onion);
        assert!(code.is_ok());

        let code = code.unwrap();
        // Should be in format word-NNNN-word-NNNN
        let parts: Vec<&str> = code.split('-').collect();
        assert_eq!(parts.len(), 4);
    }

    #[test]
    fn test_onion_to_friend_code_deterministic() {
        let onion = "3g2upl4pq6kufc4m2kyd56yz3b4qbeteqbqndzvt3sp6hhfjdkhqiiqd.onion";
        let code1 = onion_to_friend_code(onion).unwrap();
        let code2 = onion_to_friend_code(onion).unwrap();
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_invalid_onion() {
        let result = onion_to_friend_code("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_friend_code_to_onion_lookup() {
        let mut mapping = std::collections::HashMap::new();
        let onion = "3g2upl4pq6kufc4m2kyd56yz3b4qbeteqbqndzvt3sp6hhfjdkhqiiqd.onion";
        let code = onion_to_friend_code(onion).unwrap();

        mapping.insert(code.clone(), onion.to_string());

        let result = friend_code_to_onion(&code, &mapping);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), onion);
    }
}
