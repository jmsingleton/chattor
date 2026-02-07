use rand::{Rng, thread_rng};
use crate::error::{Result, TorrentChatError};

// Word list for pronounceable codes (subset for demo)
pub const WORDS: &[&str] = &[
    "happy", "tiger", "river", "cloud", "flame", "crystal", "shadow", "lotus",
    "storm", "ocean", "forest", "mountain", "solar", "lunar", "cosmic", "stellar",
];

/// Generate a friend code in format: word-NNNN-word-NNNN
pub fn generate_friend_code() -> String {
    let mut rng = thread_rng();

    let word1 = WORDS[rng.gen_range(0..WORDS.len())];
    let num1 = rng.gen_range(1000..10000);
    let word2 = WORDS[rng.gen_range(0..WORDS.len())];
    let num2 = rng.gen_range(1000..10000);

    format!("{}-{}-{}-{}", word1, num1, word2, num2)
}

/// Validate friend code format
pub fn validate_friend_code(code: &str) -> Result<()> {
    let parts: Vec<&str> = code.split('-').collect();

    if parts.len() != 4 {
        return Err(TorrentChatError::Crypto(
            "Invalid friend code format: expected word-NNNN-word-NNNN".to_string()
        ));
    }

    // Check word1
    if !WORDS.contains(&parts[0].to_lowercase().as_str()) {
        return Err(TorrentChatError::Crypto(
            format!("Invalid word in friend code: {}", parts[0])
        ));
    }

    // Check num1
    if parts[1].parse::<u32>().is_err() || parts[1].len() != 4 {
        return Err(TorrentChatError::Crypto(
            format!("Invalid number in friend code: {}", parts[1])
        ));
    }

    // Check word2
    if !WORDS.contains(&parts[2].to_lowercase().as_str()) {
        return Err(TorrentChatError::Crypto(
            format!("Invalid word in friend code: {}", parts[2])
        ));
    }

    // Check num2
    if parts[3].parse::<u32>().is_err() || parts[3].len() != 4 {
        return Err(TorrentChatError::Crypto(
            format!("Invalid number in friend code: {}", parts[3])
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_friend_code() {
        let code = generate_friend_code();
        let parts: Vec<&str> = code.split('-').collect();

        assert_eq!(parts.len(), 4);
        assert!(WORDS.contains(&parts[0]));
        assert!(parts[1].len() == 4);
        assert!(WORDS.contains(&parts[2]));
        assert!(parts[3].len() == 4);
    }

    #[test]
    fn test_validate_valid_friend_code() {
        let code = "happy-1234-tiger-5678";
        let result = validate_friend_code(code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_invalid_format() {
        let code = "happy-1234-tiger";
        let result = validate_friend_code(code);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_word() {
        let code = "invalid-1234-tiger-5678";
        let result = validate_friend_code(code);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_number() {
        let code = "happy-12-tiger-5678";
        let result = validate_friend_code(code);
        assert!(result.is_err());
    }
}
