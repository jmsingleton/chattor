use crate::db::schema::{CREATE_APP_SETTINGS, CREATE_TABLES, SCHEMA_VERSION};
use crate::error::{ChattorError, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use tracing::{info, warn};

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create database at path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(|e| ChattorError::Database(format!("Failed to open database: {}", e)))?;

        // Optimize SQLite for concurrent read/write workload
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=-8000;
             PRAGMA busy_timeout=5000;",
        )
        .map_err(|e| ChattorError::Database(format!("Failed to set pragmas: {}", e)))?;

        let mut db = Database { conn };
        db.initialize()?;
        Ok(db)
    }

    /// Initialize database schema
    fn initialize(&mut self) -> Result<()> {
        // Ensure schema_version table exists first (needed for migrations)
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);",
            )
            .map_err(|e| {
                ChattorError::Database(format!("Failed to create schema_version: {}", e))
            })?;

        // Check if this is an existing database that needs migrations
        let version: rusqlite::Result<i32> =
            self.conn
                .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                    row.get(0)
                });

        match version {
            Ok(_v) => {
                // Existing database - run migrations BEFORE CREATE_TABLES
                // so old tables get replaced before new indices are created
                self.migrate_to_v3()?;
                self.migrate_to_v4()?;
                self.migrate_to_v5()?;
                self.migrate_to_v6()?;
                self.migrate_to_v7()?;
                self.migrate_to_v8()?;
                self.migrate_to_v9()?;
                self.migrate_to_v10()?;
            }
            Err(_) => {
                // Fresh database - will set version after creating tables
            }
        }

        // Execute schema creation (IF NOT EXISTS handles already-migrated tables)
        self.conn
            .execute_batch(CREATE_TABLES)
            .map_err(|e| ChattorError::Database(format!("Failed to create tables: {}", e)))?;

        // Create app_settings table (added in v8)
        self.conn
            .execute_batch(CREATE_APP_SETTINGS)
            .map_err(|e| ChattorError::Database(format!("Failed to create app_settings: {}", e)))?;

        // Set version if not yet set (fresh database)
        if version.is_err() {
            self.conn
                .execute(
                    "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                    [SCHEMA_VERSION],
                )
                .map_err(|e| {
                    ChattorError::Database(format!("Failed to set schema version: {}", e))
                })?;
        }

        Ok(())
    }

    /// Open database with SQLCipher encryption key.
    pub fn open_encrypted<P: AsRef<Path>>(path: P, key: &[u8; 32]) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(|e| ChattorError::Database(format!("Failed to open database: {}", e)))?;

        // Set encryption key FIRST, before any other operations
        let key_hex = zeroize::Zeroizing::new(hex::encode(key));
        conn.execute_batch(&format!(
            "PRAGMA key = \"x'{}'\";\
             PRAGMA cipher_page_size = 4096;",
            *key_hex
        ))
        .map_err(|e| ChattorError::Database(format!("Failed to set encryption key: {}", e)))?;

        // Verify key works by reading from the database
        conn.execute_batch("SELECT count(*) FROM sqlite_master;")
            .map_err(|_| ChattorError::Database("Wrong passphrase or corrupted database".into()))?;

        // Optimize SQLite
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=-8000;
             PRAGMA busy_timeout=5000;",
        )
        .map_err(|e| ChattorError::Database(format!("Failed to set pragmas: {}", e)))?;

        let mut db = Database { conn };
        db.initialize()?;
        Ok(db)
    }

    /// Migrate an unencrypted database to encrypted format.
    pub fn migrate_to_encrypted<P: AsRef<Path>>(
        old_path: P,
        new_path: P,
        key: &[u8; 32],
    ) -> Result<()> {
        let old_conn = Connection::open_with_flags(&old_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| ChattorError::Database(format!("Failed to open old database: {}", e)))?;

        let key_hex = zeroize::Zeroizing::new(hex::encode(key));
        let new_path_str = new_path.as_ref().to_string_lossy().to_string();

        old_conn
            .execute_batch(&format!(
                "ATTACH DATABASE '{}' AS encrypted KEY \"x'{}'\";\
                 SELECT sqlcipher_export('encrypted');\
                 DETACH DATABASE encrypted;",
                new_path_str.replace('\'', "''"),
                *key_hex
            ))
            .map_err(|e| ChattorError::Database(format!("Failed to migrate database: {}", e)))?;

        Ok(())
    }

    /// Get a reference to the connection
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Create a Database instance from an existing connection (for testing)
    #[allow(dead_code)]
    pub fn from_connection(conn: Connection) -> Self {
        Database { conn }
    }

    /// Get the current schema version
    fn get_schema_version(&self) -> Result<i64> {
        let version: i64 = self
            .conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .map_err(|e| ChattorError::Database(format!("Failed to get schema version: {}", e)))?;
        Ok(version)
    }

    /// Migrate database from v3 to v4 (general-purpose message queue)
    fn migrate_to_v4(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 4 {
            info!("Migrating database to schema v4 (general-purpose message queue)");

            let conn = self.connection();

            // Drop old message_queue table and its indices
            conn.execute_batch(
                "DROP INDEX IF EXISTS idx_queue_to_onion;
                 DROP INDEX IF EXISTS idx_queue_conversation;
                 DROP INDEX IF EXISTS idx_queue_retry;
                 DROP TABLE IF EXISTS message_queue;",
            )
            .map_err(|e| ChattorError::Database(format!("Failed to drop old queue: {}", e)))?;

            // Update version
            conn.execute("UPDATE schema_version SET version = 4", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v4 complete (old message queue replaced)");
        }

        Ok(())
    }

    /// Migrate database from v4 to v5 (add last_read_at for unread tracking)
    fn migrate_to_v5(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 5 {
            info!("Migrating database to schema v5 (unread tracking)");

            let conn = self.connection();

            // Check if column already exists (e.g. fresh DB created with latest schema)
            let has_column: bool = conn
                .prepare("SELECT last_read_at FROM conversations LIMIT 0")
                .is_ok();

            if !has_column {
                conn.execute_batch("ALTER TABLE conversations ADD COLUMN last_read_at INTEGER;")
                    .map_err(|e| {
                        ChattorError::Database(format!("Failed to add last_read_at: {}", e))
                    })?;
            }

            conn.execute("UPDATE schema_version SET version = 5", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v5 complete");
        }

        Ok(())
    }

    /// Migrate database from v5 to v6 (ephemeral messages)
    fn migrate_to_v6(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 6 {
            info!("Migrating database to schema v6 (ephemeral messages)");

            let conn = self.connection();

            // Add ephemeral_ttl to conversations
            let has_conv_col: bool = conn
                .prepare("SELECT ephemeral_ttl FROM conversations LIMIT 0")
                .is_ok();
            if !has_conv_col {
                conn.execute_batch("ALTER TABLE conversations ADD COLUMN ephemeral_ttl INTEGER;")
                    .map_err(|e| {
                        ChattorError::Database(format!(
                            "Failed to add ephemeral_ttl to conversations: {}",
                            e
                        ))
                    })?;
            }

            // Add expires_at and ephemeral_ttl to messages
            let has_expires: bool = conn
                .prepare("SELECT expires_at FROM messages LIMIT 0")
                .is_ok();
            if !has_expires {
                conn.execute_batch(
                    "ALTER TABLE messages ADD COLUMN expires_at INTEGER;
                     ALTER TABLE messages ADD COLUMN ephemeral_ttl INTEGER;",
                )
                .map_err(|e| {
                    ChattorError::Database(format!(
                        "Failed to add ephemeral columns to messages: {}",
                        e
                    ))
                })?;
            }

            conn.execute("UPDATE schema_version SET version = 6", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v6 complete");
        }

        Ok(())
    }

    /// Migrate database from v6 to v7 (broadcast channels)
    fn migrate_to_v7(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 7 {
            info!("Migrating database to schema v7 (broadcast channels)");

            let conn = self.connection();

            // Create channel tables (IF NOT EXISTS handles fresh databases)
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS channels (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    channel_type TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS channel_posts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    channel_id INTEGER NOT NULL,
                    content TEXT NOT NULL,
                    post_id TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL,
                    signature TEXT NOT NULL,
                    FOREIGN KEY (channel_id) REFERENCES channels(id)
                );
                CREATE TABLE IF NOT EXISTS channel_subscribers (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    subscriber_onion TEXT NOT NULL,
                    channel_type TEXT NOT NULL,
                    subscribed_at INTEGER NOT NULL,
                    UNIQUE(subscriber_onion, channel_type)
                );
                CREATE TABLE IF NOT EXISTS channel_subscriptions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    publisher_onion TEXT NOT NULL,
                    channel_type TEXT NOT NULL,
                    subscribed_at INTEGER NOT NULL,
                    last_sync_at INTEGER,
                    UNIQUE(publisher_onion, channel_type)
                );
                CREATE TABLE IF NOT EXISTS channel_post_receipts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    post_id TEXT NOT NULL,
                    reader_onion TEXT NOT NULL,
                    read_at INTEGER NOT NULL,
                    UNIQUE(post_id, reader_onion)
                );",
            )
            .map_err(|e| {
                ChattorError::Database(format!("Failed to create channel tables: {}", e))
            })?;

            conn.execute("UPDATE schema_version SET version = 7", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v7 complete");
        }

        Ok(())
    }

    /// Migrate database from v7 to v8 (app_settings for .onion persistence)
    fn migrate_to_v8(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 8 {
            info!("Migrating database to schema v8 (app_settings table)");

            let conn = self.connection();

            conn.execute_batch(crate::db::schema::CREATE_APP_SETTINGS)
                .map_err(|e| {
                    ChattorError::Database(format!("Failed to create app_settings: {}", e))
                })?;

            conn.execute("UPDATE schema_version SET version = 8", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v8 complete");
        }

        Ok(())
    }

    /// Migrate database from v8 to v9 (wipe Signal sessions for crypto upgrade)
    ///
    /// The Signal Protocol implementation was rewritten to use libsignal-dezire's
    /// Double Ratchet. Old session state is incompatible, so we wipe all sessions
    /// and stored PreKey private material to force re-establishment.
    fn migrate_to_v9(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 9 {
            info!("Migrating database to schema v9 (wipe Signal sessions for crypto upgrade)");

            let conn = self.connection();

            // Delete all Signal sessions (incompatible with new Double Ratchet format)
            let deleted_sessions =
                conn.execute("DELETE FROM signal_sessions", [])
                    .map_err(|e| {
                        ChattorError::Database(format!("Failed to clear signal sessions: {}", e))
                    })?;
            info!("  Cleared {} old Signal sessions", deleted_sessions);

            // Delete stored PreKey private material from app_settings
            let deleted_prekeys = conn
                .execute("DELETE FROM app_settings WHERE key LIKE 'prekey_%'", [])
                .map_err(|e| {
                    ChattorError::Database(format!("Failed to clear prekey material: {}", e))
                })?;
            info!("  Cleared {} stored PreKey entries", deleted_prekeys);

            // Update version
            conn.execute("UPDATE schema_version SET version = 9", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            warn!("Schema upgraded to v9. All Signal sessions and PreKey material cleared.");
            warn!("  Sessions will be re-established automatically on next message exchange.");

            info!("Migration to schema v9 complete");
        }

        Ok(())
    }

    /// Migrate database from v9 to v10 (TOFU Ed25519 pubkey on friends)
    fn migrate_to_v10(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 10 {
            info!("Migrating database to schema v10 (TOFU Ed25519 pubkey)");

            let conn = self.connection();

            let has_column: bool = conn
                .prepare("SELECT ed25519_pubkey FROM friends LIMIT 0")
                .is_ok();

            if !has_column {
                conn.execute_batch("ALTER TABLE friends ADD COLUMN ed25519_pubkey BLOB;")
                    .map_err(|e| {
                        ChattorError::Database(format!("Failed to add ed25519_pubkey: {}", e))
                    })?;
            }

            conn.execute("UPDATE schema_version SET version = 10", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

            info!("Migration to schema v10 complete");
        }

        Ok(())
    }

    /// Migrate database from v2 to v3 (production Signal Protocol)
    fn migrate_to_v3(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 3 {
            info!("🔄 Migrating database to schema v3 (production Signal Protocol)");

            let conn = self.connection();

            // Clear old sessions (incompatible with v3 format)
            let deleted = conn
                .execute("DELETE FROM signal_sessions", [])
                .map_err(|e| ChattorError::Database(format!("Failed to clear sessions: {}", e)))?;
            info!("   Cleared {} old Signal sessions", deleted);

            // Update version
            conn.execute("UPDATE schema_version SET version = 3", [])
                .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

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
        let version: i32 = db
            .connection()
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
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
    fn test_migration_v8_to_v9() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();

        // Create a v8 database with sessions and prekey material
        {
            let db = Database::open(temp_db.path()).unwrap();
            let conn = db.connection();

            // Set version back to 8 (simulating pre-migration state)
            conn.execute("UPDATE schema_version SET version = 8", [])
                .unwrap();

            // Insert some old Signal sessions
            conn.execute(
                "INSERT INTO signal_sessions (remote_onion, session_state, updated_at) VALUES ('alice.onion', X'DEADBEEF', 12345)",
                []
            ).unwrap();
            conn.execute(
                "INSERT INTO signal_sessions (remote_onion, session_state, updated_at) VALUES ('bob.onion', X'CAFEBABE', 67890)",
                []
            ).unwrap();

            // Insert some prekey_ entries in app_settings
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES ('prekey_identity_secret', 'secret1')",
                []
            ).unwrap();
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES ('prekey_signed_prekey_secret', 'secret2')",
                []
            ).unwrap();
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES ('prekey_one_time_secret', 'secret3')",
                []
            ).unwrap();

            // Also insert a non-prekey setting that should survive
            conn.execute(
                "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('onion_address', 'keep_me.onion')",
                []
            ).unwrap();

            // Verify data is there
            let session_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM signal_sessions", [], |row| row.get(0))
                .unwrap();
            assert_eq!(session_count, 2);

            let prekey_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM app_settings WHERE key LIKE 'prekey_%'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(prekey_count, 3);
        }

        // Reopen — this triggers migrations including v9
        let db = Database::open(temp_db.path()).unwrap();
        let conn = db.connection();

        // Should be at version 9
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION as i64);

        // All signal sessions should be deleted
        let session_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM signal_sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(session_count, 0);

        // All prekey_ entries should be deleted
        let prekey_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM app_settings WHERE key LIKE 'prekey_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(prekey_count, 0);

        // Non-prekey settings should survive
        let onion: String = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'onion_address'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(onion, "keep_me.onion");
    }

    #[test]
    fn test_key_derivation() {
        let salt = [1u8; 16];
        let key1 = crate::db::encryption::derive_key(b"password123", &salt).unwrap();
        let key2 = crate::db::encryption::derive_key(b"password123", &salt).unwrap();
        assert_eq!(key1, key2); // Deterministic

        let key3 = crate::db::encryption::derive_key(b"different", &salt).unwrap();
        assert_ne!(key1, key3); // Different passphrase = different key
    }

    #[test]
    fn test_encrypted_database_roundtrip() {
        let temp_file = NamedTempFile::new().unwrap();
        let key = [42u8; 32];

        // Create encrypted DB
        {
            let db = Database::open_encrypted(temp_file.path(), &key).unwrap();
            let conn = db.connection();
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES ('test', 'hello')",
                [],
            )
            .unwrap();
        }

        // Reopen with same key
        {
            let db = Database::open_encrypted(temp_file.path(), &key).unwrap();
            let val: String = db
                .connection()
                .query_row(
                    "SELECT value FROM app_settings WHERE key = 'test'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(val, "hello");
        }

        // Wrong key should fail
        {
            let wrong_key = [99u8; 32];
            let result = Database::open_encrypted(temp_file.path(), &wrong_key);
            assert!(result.is_err());
        }
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
            conn.execute("UPDATE schema_version SET version = 2", [])
                .unwrap();

            // Add pre-v3 sessions to test migration clears them
            conn.execute(
                "INSERT INTO signal_sessions (remote_onion, session_state, updated_at) VALUES ('test.onion', X'00', 12345)",
                []
            ).unwrap();
        }

        // Reopen and trigger migration
        let mut db = Database::open(temp_db.path()).unwrap();
        db.initialize().unwrap();

        // Should be at latest schema version (v3 + v4 migrations both run)
        let version: i64 = db
            .connection()
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION as i64);

        // Old sessions should be cleared
        let count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM signal_sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
