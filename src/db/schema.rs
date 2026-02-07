/// SQL schema for torrent-chat database
pub const SCHEMA_VERSION: i32 = 2;

pub const CREATE_TABLES: &str = r#"
-- Schema version tracking
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);

-- User settings and identity
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Friends list
CREATE TABLE IF NOT EXISTS friends (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    onion_address TEXT NOT NULL UNIQUE,
    display_name TEXT,
    friend_code TEXT,
    added_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    signal_identity_key BLOB,
    signal_prekey_bundle BLOB
);

-- Conversations
CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    friend_id INTEGER NOT NULL,
    is_ephemeral INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (friend_id) REFERENCES friends(id)
);

-- Messages
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT UNIQUE NOT NULL,
    conversation_id INTEGER NOT NULL,
    sender_onion TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'sent',
    FOREIGN KEY (conversation_id) REFERENCES conversations(id)
);

-- Friend requests (pending)
CREATE TABLE IF NOT EXISTS friend_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_onion TEXT NOT NULL,
    friend_code TEXT,
    received_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
);

-- Indices for performance
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_message_id ON messages(message_id);
CREATE INDEX IF NOT EXISTS idx_friends_onion ON friends(onion_address);

-- Phase 2: Message queue for offline delivery
CREATE TABLE IF NOT EXISTS message_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    to_onion TEXT NOT NULL,
    conversation_id INTEGER NOT NULL,
    encrypted_message BLOB NOT NULL,
    message_uuid TEXT,
    created_at INTEGER NOT NULL,
    retry_count INTEGER DEFAULT 0,
    last_retry_at INTEGER,
    max_retries INTEGER DEFAULT 50,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id)
);

-- Phase 2: Signal Protocol sessions
CREATE TABLE IF NOT EXISTS signal_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    remote_onion TEXT NOT NULL UNIQUE,
    session_state BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Phase 2: Blocked .onion addresses
CREATE TABLE IF NOT EXISTS blocked_onions (
    onion_address TEXT PRIMARY KEY,
    blocked_at INTEGER NOT NULL,
    reason TEXT
);

-- Phase 2: Full-text search on messages
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    sender_onion,
    conversation_id,
    content='messages',
    content_rowid='id'
);

-- Phase 2: FTS triggers to keep search index in sync
CREATE TRIGGER IF NOT EXISTS messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content, sender_onion, conversation_id)
    VALUES (new.id, new.content, new.sender_onion, new.conversation_id);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_update AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content, sender_onion, conversation_id)
    VALUES('delete', old.id, old.content, old.sender_onion, old.conversation_id);
    INSERT INTO messages_fts(rowid, content, sender_onion, conversation_id)
    VALUES (new.id, new.content, new.sender_onion, new.conversation_id);
END;

-- Phase 2: Additional indices
CREATE INDEX IF NOT EXISTS idx_queue_to_onion ON message_queue(to_onion);
CREATE INDEX IF NOT EXISTS idx_queue_conversation ON message_queue(conversation_id);
CREATE INDEX IF NOT EXISTS idx_queue_retry ON message_queue(retry_count, last_retry_at);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_schema_version_defined() {
        assert_eq!(SCHEMA_VERSION, 2);
    }

    #[test]
    fn test_create_tables_not_empty() {
        assert!(!CREATE_TABLES.is_empty());
        assert!(CREATE_TABLES.contains("CREATE TABLE"));
    }

    // Phase 2 tests - these will fail until we add the new tables
    #[test]
    fn test_message_queue_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();

        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='message_queue'",
            [],
            |row| row.get::<_, String>(0)
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "message_queue");
    }

    #[test]
    fn test_signal_sessions_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();

        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='signal_sessions'",
            [],
            |row| row.get::<_, String>(0)
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "signal_sessions");
    }

    #[test]
    fn test_blocked_onions_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();

        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='blocked_onions'",
            [],
            |row| row.get::<_, String>(0)
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "blocked_onions");
    }

    #[test]
    fn test_messages_fts_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();

        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='messages_fts'",
            [],
            |row| row.get::<_, String>(0)
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "messages_fts");
    }
}
