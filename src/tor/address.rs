use crate::error::Result;

/// Convert .onion address to friend code (reversible word encoding).
///
/// Delegates to the friend_code module which handles the actual
/// byte-to-word encoding of the Ed25519 public key.
pub fn onion_to_friend_code(onion: &str) -> Result<String> {
    crate::protocol::friend_code::onion_to_friend_code(onion)
}

/// Convert friend code back to .onion address.
///
/// Reverses the word encoding to recover the public key,
/// then reconstructs the v3 .onion address with proper checksum.
#[allow(dead_code)]
pub fn friend_code_to_onion(friend_code: &str) -> Result<String> {
    crate::protocol::friend_code::friend_code_to_onion(friend_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        // Build a valid v3 .onion from a known pubkey via the friend_code module
        let _pubkey = [0u8; 32];
        let onion = crate::protocol::friend_code::friend_code_to_onion(
            &std::iter::repeat("ace").take(32).collect::<Vec<_>>().join(" ")
        ).unwrap();

        let code = onion_to_friend_code(&onion).unwrap();
        let recovered = friend_code_to_onion(&code).unwrap();
        assert_eq!(onion, recovered);
    }

    #[test]
    fn test_deterministic() {
        let pubkey_words: String = std::iter::repeat("ace").take(32).collect::<Vec<_>>().join(" ");
        let onion = crate::protocol::friend_code::friend_code_to_onion(&pubkey_words).unwrap();

        let code1 = onion_to_friend_code(&onion).unwrap();
        let code2 = onion_to_friend_code(&onion).unwrap();
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_invalid_onion() {
        let result = onion_to_friend_code("invalid");
        assert!(result.is_err());
    }
}
