/// SQL schema for chattor database
pub const SCHEMA_VERSION: i32 = 10;

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
    last_read_at INTEGER,
    ephemeral_ttl INTEGER,
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
    expires_at INTEGER,
    ephemeral_ttl INTEGER,
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

-- General-purpose outgoing message queue
CREATE TABLE IF NOT EXISTS message_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    peer_onion TEXT NOT NULL,
    message_json TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'normal',
    retry_count INTEGER NOT NULL DEFAULT 0,
    next_retry_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
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

-- Message queue indices
CREATE INDEX IF NOT EXISTS idx_queue_pending
    ON message_queue(next_retry_at, status)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_queue_peer ON message_queue(peer_onion);

-- Phase 3: Broadcast channels
CREATE TABLE IF NOT EXISTS channels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_type TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Channel posts. A "channel" is identified by (publisher_onion, channel_type),
-- not by an integer FK — this lets us store many publishers' feeds in one
-- table without lumping them into a shared bucket. The `channels` table still
-- exists as a registry of our own channels but is no longer referenced here.
CREATE TABLE IF NOT EXISTS channel_posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    publisher_onion TEXT NOT NULL,
    channel_type TEXT NOT NULL,
    content TEXT NOT NULL,
    post_id TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    signature TEXT NOT NULL
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
);

CREATE INDEX IF NOT EXISTS idx_channel_posts_publisher_type
    ON channel_posts(publisher_onion, channel_type);
CREATE INDEX IF NOT EXISTS idx_channel_posts_post_id ON channel_posts(post_id);
CREATE INDEX IF NOT EXISTS idx_channel_posts_created ON channel_posts(created_at);
CREATE INDEX IF NOT EXISTS idx_channel_subs_onion ON channel_subscribers(subscriber_onion);
CREATE INDEX IF NOT EXISTS idx_channel_subscriptions_publisher ON channel_subscriptions(publisher_onion);
"#;

pub const CREATE_APP_SETTINGS: &str = "
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_schema_version_defined() {
        assert_eq!(SCHEMA_VERSION, 10);
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

    #[test]
    fn test_channels_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();
        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='channels'",
            [], |row| row.get::<_, String>(0)
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "channels");
    }

    #[test]
    fn test_channel_posts_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();
        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='channel_posts'",
            [], |row| row.get::<_, String>(0)
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_channel_subscribers_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();
        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='channel_subscribers'",
            [], |row| row.get::<_, String>(0)
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_channel_subscriptions_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();
        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='channel_subscriptions'",
            [], |row| row.get::<_, String>(0)
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_channel_post_receipts_table_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();
        let result = conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='channel_post_receipts'",
            [], |row| row.get::<_, String>(0)
        );
        assert!(result.is_ok());
    }
}
