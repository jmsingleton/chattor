use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum TorrentChatError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // Signal Protocol errors
    #[error("Signal Protocol error: {0}")]
    SignalProtocol(String),

    #[error("Session not found for {0}")]
    SessionNotFound(String),

    #[error("Invalid PreKey bundle: {0}")]
    InvalidPreKeyBundle(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    // Tor errors
    #[error("Tor connection error: {0}")]
    TorConnection(String),

    #[error("Failed to bootstrap Tor: {0}")]
    TorBootstrap(String),

    #[error("Invalid .onion address: {0}")]
    InvalidOnionAddress(String),

    // Network errors
    #[error("Network error: {0}")]
    Network(String),

    #[error("Connection timeout to {0}")]
    ConnectionTimeout(String),

    // Existing errors
    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Tor error: {0}")]
    Tor(String),
}

#[allow(dead_code)]
pub type ChattorError = TorrentChatError;
pub type Result<T> = std::result::Result<T, TorrentChatError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = TorrentChatError::Tor("connection failed".to_string());
        assert_eq!(err.to_string(), "Tor error: connection failed");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: TorrentChatError = io_err.into();
        assert!(matches!(err, TorrentChatError::Io(_)));
    }
}
