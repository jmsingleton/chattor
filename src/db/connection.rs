use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use tracing::{info, warn};
use crate::error::{Result, TorrentChatError};
use crate::db::schema::{CREATE_TABLES, SCHEMA_VERSION};

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create database at path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        ).map_err(|e| TorrentChatError::Database(format!("Failed to open database: {}", e)))?;

        let mut db = Database { conn };
        db.initialize()?;
        Ok(db)
    }

    /// Initialize database schema
    fn initialize(&mut self) -> Result<()> {
        // Execute schema creation
        self.conn.execute_batch(CREATE_TABLES)
            .map_err(|e| TorrentChatError::Database(format!("Failed to create tables: {}", e)))?;

        // Check/set schema version
        let version: rusqlite::Result<i32> = self.conn.query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0)
        );

        match version {
            Ok(_v) => {
                // Version exists, run migrations
                self.migrate_to_v3()?;
                Ok(())
            },
            Err(_) => {
                // No version set, insert current
                self.conn.execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    [SCHEMA_VERSION]
                ).map_err(|e| TorrentChatError::Database(format!("Failed to set schema version: {}", e)))?;
                Ok(())
            }
        }
    }

    /// Get a reference to the connection
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Create a Database instance from an existing connection (for testing)
    pub fn from_connection(conn: Connection) -> Self {
        Database { conn }
    }

    /// Get the current schema version
    fn get_schema_version(&self) -> Result<i64> {
        let version: i64 = self.conn.query_row(
            "SELECT version FROM schema_version",
            [],
            |row| row.get(0)
        ).map_err(|e| TorrentChatError::Database(format!("Failed to get schema version: {}", e)))?;
        Ok(version)
    }

    /// Migrate database from v2 to v3 (production Signal Protocol)
    fn migrate_to_v3(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 3 {
            info!("🔄 Migrating database to schema v3 (production Signal Protocol)");

            let conn = self.connection();

            // Clear old stub sessions (incompatible format)
            let deleted = conn.execute("DELETE FROM signal_sessions", [])
                .map_err(|e| TorrentChatError::Database(format!("Failed to clear sessions: {}", e)))?;
            info!("   Cleared {} old Signal sessions", deleted);

            // Update version
            conn.execute("UPDATE schema_version SET version = 3", [])
                .map_err(|e| TorrentChatError::Database(format!("Failed to update version: {}", e)))?;

            warn!("⚠️  Schema upgraded to v3. All Signal sessions cleared.");
            warn!("   You'll need to re-establish sessions by:");
            warn!("   1. Re-sending friend requests to existing contacts");
            warn!("   2. Waiting for them to accept again");
            warn!("   This is a one-time migration for production crypto.");

            info!("✅ Migration to schema v3 complete");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_database_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open(temp_file.path());
        assert!(db.is_ok());
    }

    #[test]
    fn test_schema_initialization() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open(temp_file.path()).unwrap();

        // Verify schema_version table exists
        let version: i32 = db.connection()
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| row.get(0))
            .unwrap();

        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn test_tables_created() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open(temp_file.path()).unwrap();

        // Verify tables exist
        let table_count: i32 = db.connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('friends', 'messages', 'conversations')",
                [],
                |row| row.get(0)
            )
            .unwrap();

        assert_eq!(table_count, 3);
    }

    #[test]
    fn test_migration_v2_to_v3() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();

        // Create v2 database
        {
            let mut db = Database::open(temp_db.path()).unwrap();
            db.initialize().unwrap();
            let conn = db.connection();

            // Set version to 2
            conn.execute("UPDATE schema_version SET version = 2", []).unwrap();

            // Add some stub sessions
            conn.execute(
                "INSERT INTO signal_sessions (remote_onion, session_state, updated_at) VALUES ('test.onion', X'00', 12345)",
                []
            ).unwrap();
        }

        // Reopen and trigger migration
        let mut db = Database::open(temp_db.path()).unwrap();
        db.initialize().unwrap();

        // Should be v3 now
        let version: i64 = db.connection().query_row(
            "SELECT version FROM schema_version",
            [],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(version, 3);

        // Old sessions should be cleared
        let count: i64 = db.connection().query_row(
            "SELECT COUNT(*) FROM signal_sessions",
            [],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 0);
    }
}
