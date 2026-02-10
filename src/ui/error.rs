use crate::error::TorrentChatError;

pub fn format_error_for_user(err: &TorrentChatError) -> String {
    match err {
        TorrentChatError::SignalProtocol(_) =>
            "Encryption error. Try re-adding this friend.".into(),

        TorrentChatError::SessionNotFound(_) =>
            "No secure session found. Please send a friend request first.".into(),

        TorrentChatError::TorConnection(_) | TorrentChatError::TorBootstrap(_) =>
            "Tor connection failed. Please check your internet connection.".into(),

        TorrentChatError::InvalidOnionAddress(addr) =>
            format!("Invalid friend address: {}", addr),

        TorrentChatError::ConnectionTimeout(addr) =>
            format!("Connection timeout. {} may be offline.", addr),

        TorrentChatError::DecryptionFailed(_) =>
            "Message decryption failed. The sender may need to resend.".into(),

        _ => format!("Error: {}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_crypto_error() {
        let err = TorrentChatError::Crypto("test error".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
    }

    #[test]
    fn test_format_tor_error() {
        let err = TorrentChatError::Tor("Connection failed".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
    }

    #[test]
    fn test_format_signal_protocol_error() {
        let err = TorrentChatError::SignalProtocol("encryption failed".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Encryption error. Try re-adding this friend.");
    }

    #[test]
    fn test_format_session_not_found() {
        let err = TorrentChatError::SessionNotFound("friend123.onion".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "No secure session found. Please send a friend request first.");
    }

    #[test]
    fn test_format_tor_connection_error() {
        let err = TorrentChatError::TorConnection("network unreachable".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Tor connection failed. Please check your internet connection.");
    }

    #[test]
    fn test_format_tor_bootstrap_error() {
        let err = TorrentChatError::TorBootstrap("bootstrap failed".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Tor connection failed. Please check your internet connection.");
    }

    #[test]
    fn test_format_invalid_onion_address() {
        let err = TorrentChatError::InvalidOnionAddress("invalid.onion".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Invalid friend address: invalid.onion");
    }

    #[test]
    fn test_format_connection_timeout() {
        let err = TorrentChatError::ConnectionTimeout("friend123.onion".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Connection timeout. friend123.onion may be offline.");
    }

    #[test]
    fn test_format_decryption_failed() {
        let err = TorrentChatError::DecryptionFailed("invalid ciphertext".into());
        let formatted = format_error_for_user(&err);
        assert_eq!(formatted, "Message decryption failed. The sender may need to resend.");
    }

    #[test]
    fn test_format_database_error() {
        let err = TorrentChatError::Database("disk full".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
        assert!(formatted.contains("Database error"));
    }

    #[test]
    fn test_format_network_error() {
        let err = TorrentChatError::Network("connection refused".into());
        let formatted = format_error_for_user(&err);
        assert!(formatted.contains("Error:"));
        assert!(formatted.contains("Network error"));
    }
}
