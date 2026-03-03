use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Config directory path
    pub config_dir: PathBuf,

    /// Data directory path
    pub data_dir: PathBuf,

    /// Database file path
    pub db_path: PathBuf,

    /// Enable debug logging
    pub debug: bool,

    /// Tor SOCKS port
    pub tor_socks_port: u16,
}

impl Settings {
    /// Create settings with defaults
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Result<Self> {
        let config_dir = Self::default_config_dir()?;
        let data_dir = Self::default_data_dir()?;
        let db_path = data_dir.join("messages.db");

        Ok(Settings {
            config_dir,
            data_dir,
            db_path,
            debug: false,
            tor_socks_port: 9050,
        })
    }

    /// Get default config directory based on OS
    fn default_config_dir() -> Result<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME").map_err(|_| {
                crate::error::ChattorError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "HOME not set",
                ))
            })?;
            Ok(PathBuf::from(home).join("Library/Application Support/chattor"))
        }

        #[cfg(not(target_os = "macos"))]
        {
            let home = std::env::var("HOME").map_err(|_| {
                crate::error::ChattorError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "HOME not set",
                ))
            })?;
            Ok(PathBuf::from(home).join(".config/chattor"))
        }
    }

    /// Get default data directory based on OS
    fn default_data_dir() -> Result<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME").map_err(|_| {
                crate::error::ChattorError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "HOME not set",
                ))
            })?;
            Ok(PathBuf::from(home).join("Library/Application Support/chattor"))
        }

        #[cfg(not(target_os = "macos"))]
        {
            let home = std::env::var("HOME").map_err(|_| {
                crate::error::ChattorError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "HOME not set",
                ))
            })?;
            Ok(PathBuf::from(home).join(".local/share/chattor"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings_creation() {
        let settings = Settings::default();
        assert!(settings.is_ok());
    }

    #[test]
    fn test_default_settings_values() {
        let settings = Settings::default().unwrap();
        assert_eq!(settings.debug, false);
        assert_eq!(settings.tor_socks_port, 9050);
        assert!(settings.config_dir.to_string_lossy().contains("chattor"));
    }

    #[test]
    fn test_db_path_in_data_dir() {
        let settings = Settings::default().unwrap();
        assert!(settings.db_path.starts_with(&settings.data_dir));
        assert_eq!(settings.db_path.file_name().unwrap(), "messages.db");
    }
}
