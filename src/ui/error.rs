use crate::error::ChattorError;

pub fn format_error_for_user(err: &ChattorError) -> String {
    match err {
        ChattorError::SignalProtocol(_) =>
            "Encryption error. Try re-adding this friend.".into(),

        ChattorError::SessionNotFound(_) =>
            "No secure session found. Please send a friend request first.".into(),

        ChattorError::TorConnection(_) | ChattorError::TorBootstrap(_) =>
            "Tor connection failed. Please check your internet connection.".into(),

        ChattorError::InvalidOnionAddress(addr) =>
            format!("Invalid friend address: {}", addr),

        ChattorError::ConnectionTimeout(addr) =>
            format!("Connection timeout. {} may be offline.", addr),

        ChattorError::DecryptionFailed(_) =>
            "Message decryption failed. The sender may need to resend.".into(),

        _ => format!("Error: {}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_crypto_error() {
        let err = ChattorError::Crypto("test error".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
    }

    #[test]
    fn test_format_tor_error() {
        let err = ChattorError::Tor("Connection failed".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
    }

    #[test]
    fn test_format_signal_protocol_error() {
        let err = ChattorError::SignalProtocol("encryption failed".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Encryption error. Try re-adding this friend.");
    }

    #[test]
    fn test_format_session_not_found() {
        let err = ChattorError::SessionNotFound("friend123.onion".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "No secure session found. Please send a friend request first.");
    }

    #[test]
    fn test_format_tor_connection_error() {
        let err = ChattorError::TorConnection("network unreachable".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Tor connection failed. Please check your internet connection.");
    }

    #[test]
    fn test_format_tor_bootstrap_error() {
        let err = ChattorError::TorBootstrap("bootstrap failed".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Tor connection failed. Please check your internet connection.");
    }

    #[test]
    fn test_format_invalid_onion_address() {
        let err = ChattorError::InvalidOnionAddress("invalid.onion".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Invalid friend address: invalid.onion");
    }

    #[test]
    fn test_format_connection_timeout() {
        let err = ChattorError::ConnectionTimeout("friend123.onion".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Connection timeout. friend123.onion may be offline.");
    }

    #[test]
    fn test_format_decryption_failed() {
        let err = ChattorError::DecryptionFailed("invalid ciphertext".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Message decryption failed. The sender may need to resend.");
    }

    #[test]
    fn test_format_database_error() {
        let err = ChattorError::Database("disk full".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
        assert!(formatted.contains("Database error"));
    }

    #[test]
    fn test_format_network_error() {
        let err = ChattorError::Network("connection refused".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
        assert!(formatted.contains("Network error"));
    }
}
