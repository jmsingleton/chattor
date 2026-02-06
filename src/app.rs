use crate::error::Result;
use crate::config::Settings;
use crate::db::Database;
use crate::crypto::IdentityKeypair;
use std::fs;

pub struct App {
    pub settings: Settings,
    pub db: Database,
    pub identity: IdentityKeypair,
}

impl App {
    pub fn new() -> Result<Self> {
        // Load settings
        let settings = Settings::default()?;

        // Ensure directories exist
        fs::create_dir_all(&settings.config_dir)?;
        fs::create_dir_all(&settings.data_dir)?;

        // Open database
        let db = Database::open(&settings.db_path)?;

        // Generate or load identity
        // TODO: In future, load from database if exists
        let identity = IdentityKeypair::generate()?;

        Ok(App {
            settings,
            db,
            identity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_app_creation_with_temp_dirs() {
        let temp_dir = TempDir::new().unwrap();

        // Override HOME for test
        std::env::set_var("HOME", temp_dir.path());

        let app = App::new();
        assert!(app.is_ok());
    }
}
