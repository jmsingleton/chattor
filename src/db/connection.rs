use rusqlite::{Connection, OpenFlags};
use std::path::Path;
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
            Ok(v) if v == SCHEMA_VERSION => Ok(()),
            Ok(v) => Err(TorrentChatError::Database(
                format!("Schema version mismatch: expected {}, found {}", SCHEMA_VERSION, v)
            )),
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
}
