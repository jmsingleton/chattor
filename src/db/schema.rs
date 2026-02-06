/// SQL schema for torrent-chat database
pub const SCHEMA_VERSION: i32 = 1;

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
    status TEXT NOT NULL DEFAULT 'pending'
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
CREATE INDEX IF NOT EXISTS idx_friends_onion ON friends(onion_address);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_defined() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_create_tables_not_empty() {
        assert!(!CREATE_TABLES.is_empty());
        assert!(CREATE_TABLES.contains("CREATE TABLE"));
    }
}
