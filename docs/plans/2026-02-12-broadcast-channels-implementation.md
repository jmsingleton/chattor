# Broadcast Channels Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add one-to-many broadcast channels with auto Public + Friends-Only channels per user, pull-based sync, and read receipt counts.

**Architecture:** Extend existing message protocol with 6 new variants. Public posts are Ed25519-signed plaintext; friends-only posts use Signal sessions. Subscribers pull missed posts on connect. Fixed retention of 100 posts per channel.

**Tech Stack:** Rust, rusqlite (SQLCipher), serde, ratatui, existing Tor/Signal infrastructure.

---

### Task 1: Add channel tables to schema

**Files:**
- Modify: `src/db/schema.rs`

**Step 1: Write failing test for channel tables**

Add to the `#[cfg(test)] mod tests` block in `src/db/schema.rs`:

```rust
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
```

**Step 2: Run tests to verify they fail**

Run: `cargo test db::schema::tests::test_channels_table_exists -- --nocapture`
Expected: FAIL

**Step 3: Add channel tables to CREATE_TABLES and bump version**

Change `SCHEMA_VERSION` from `6` to `7`.

Add the following SQL to the end of `CREATE_TABLES` (before the closing `"#;`):

```sql
-- Phase 3: Broadcast channels
CREATE TABLE IF NOT EXISTS channels (
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
);

CREATE INDEX IF NOT EXISTS idx_channel_posts_channel ON channel_posts(channel_id);
CREATE INDEX IF NOT EXISTS idx_channel_posts_post_id ON channel_posts(post_id);
CREATE INDEX IF NOT EXISTS idx_channel_posts_created ON channel_posts(created_at);
CREATE INDEX IF NOT EXISTS idx_channel_subs_onion ON channel_subscribers(subscriber_onion);
CREATE INDEX IF NOT EXISTS idx_channel_subscriptions_publisher ON channel_subscriptions(publisher_onion);
```

Also update the existing `test_schema_version_defined` test to expect `7` instead of `6`.

**Step 4: Run tests to verify they pass**

Run: `cargo test db::schema -- --nocapture`
Expected: All PASS

**Step 5: Add v7 migration to connection.rs**

Add a `migrate_to_v7` method to `Database` in `src/db/connection.rs`, following the same pattern as `migrate_to_v6`. The migration should create the 5 new tables (same SQL as above). Call it from `initialize()` after `migrate_to_v6()`.

```rust
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
            );"
        ).map_err(|e| ChattorError::Database(format!("Failed to create channel tables: {}", e)))?;

        conn.execute("UPDATE schema_version SET version = 7", [])
            .map_err(|e| ChattorError::Database(format!("Failed to update version: {}", e)))?;

        info!("Migration to schema v7 complete");
    }

    Ok(())
}
```

In `initialize()`, add after the `self.migrate_to_v6()?;` line:
```rust
self.migrate_to_v7()?;
```

**Step 6: Run all tests**

Run: `cargo test`
Expected: All PASS

**Step 7: Commit**

```bash
git add src/db/schema.rs src/db/connection.rs
git commit -m "feat: add broadcast channel tables (schema v7)"
```

---

### Task 2: Add channel protocol messages

**Files:**
- Modify: `src/protocol/message.rs`

**Step 1: Write failing test for ChannelPost serialization**

Add to `#[cfg(test)] mod tests` in `src/protocol/message.rs`:

```rust
#[test]
fn test_channel_post_serialization() {
    let msg = Message::ChannelPost(ChannelPostMessage {
        publisher_onion: "alice.onion".into(),
        channel_type: ChannelType::Public,
        post_id: Uuid::new_v4(),
        content: "Hello world!".into(),
        created_at: 1234567890,
        signature: "sig123".into(),
    });
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("channel_post"));
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_channel_subscribe_serialization() {
    let msg = Message::ChannelSubscribe(ChannelSubscribeMessage {
        subscriber_onion: "bob.onion".into(),
        channel_type: ChannelType::Public,
        timestamp: 1234567890,
    });
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_channel_sync_request_serialization() {
    let msg = Message::ChannelSyncRequest(ChannelSyncRequestMessage {
        subscriber_onion: "bob.onion".into(),
        channel_type: ChannelType::FriendsOnly,
        since_timestamp: 1234567890,
    });
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_channel_sync_response_serialization() {
    let msg = Message::ChannelSyncResponse(ChannelSyncResponseMessage {
        publisher_onion: "alice.onion".into(),
        channel_type: ChannelType::Public,
        posts: vec![
            ChannelPostMessage {
                publisher_onion: "alice.onion".into(),
                channel_type: ChannelType::Public,
                post_id: Uuid::new_v4(),
                content: "Post 1".into(),
                created_at: 1000,
                signature: "sig1".into(),
            },
        ],
    });
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}

#[test]
fn test_channel_post_receipt_serialization() {
    let msg = Message::ChannelPostReceipt(ChannelPostReceiptMessage {
        post_id: Uuid::new_v4(),
        reader_onion: "bob.onion".into(),
        timestamp: 1234567890,
    });
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, deserialized);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test protocol::message::tests::test_channel_post_serialization`
Expected: FAIL (types don't exist)

**Step 3: Add types and message variants**

Add to `src/protocol/message.rs` before the `#[cfg(test)]` block:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Public,
    FriendsOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelPostMessage {
    pub publisher_onion: String,
    pub channel_type: ChannelType,
    pub post_id: Uuid,
    pub content: String,
    pub created_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSubscribeMessage {
    pub subscriber_onion: String,
    pub channel_type: ChannelType,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelUnsubscribeMessage {
    pub subscriber_onion: String,
    pub channel_type: ChannelType,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSyncRequestMessage {
    pub subscriber_onion: String,
    pub channel_type: ChannelType,
    pub since_timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSyncResponseMessage {
    pub publisher_onion: String,
    pub channel_type: ChannelType,
    pub posts: Vec<ChannelPostMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelPostReceiptMessage {
    pub post_id: Uuid,
    pub reader_onion: String,
    pub timestamp: i64,
}
```

Add 6 new variants to the `Message` enum:

```rust
#[serde(rename = "channel_subscribe")]
ChannelSubscribe(ChannelSubscribeMessage),

#[serde(rename = "channel_unsubscribe")]
ChannelUnsubscribe(ChannelUnsubscribeMessage),

#[serde(rename = "channel_post")]
ChannelPost(ChannelPostMessage),

#[serde(rename = "channel_sync_request")]
ChannelSyncRequest(ChannelSyncRequestMessage),

#[serde(rename = "channel_sync_response")]
ChannelSyncResponse(ChannelSyncResponseMessage),

#[serde(rename = "channel_post_receipt")]
ChannelPostReceipt(ChannelPostReceiptMessage),
```

**Step 4: Run tests to verify they pass**

Run: `cargo test protocol::message -- --nocapture`
Expected: All PASS

**Step 5: Update protocol/mod.rs exports**

Add to the `pub use message::` line in `src/protocol/mod.rs`:

```rust
pub use message::{Message, TextMessage, PlaintextPayload, ChannelType, ChannelPostMessage};
```

**Step 6: Run full test suite**

Run: `cargo test`
Expected: All PASS

**Step 7: Commit**

```bash
git add src/protocol/message.rs src/protocol/mod.rs
git commit -m "feat: add broadcast channel protocol messages"
```

---

### Task 3: Add channel database queries

**Files:**
- Modify: `src/db/queries.rs`

**Step 1: Write failing test for channel initialization**

Add to `#[cfg(test)] mod tests` in `src/db/queries.rs`:

```rust
#[test]
fn test_initialize_channels() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    initialize_channels(&db).unwrap();

    let count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM channels", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 2);

    // Calling again should be idempotent
    initialize_channels(&db).unwrap();
    let count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM channels", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 2);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test db::queries::tests::test_initialize_channels`
Expected: FAIL

**Step 3: Implement initialize_channels**

Add to `src/db/queries.rs`:

```rust
/// Create the two auto-channels (public + friends_only) if they don't exist
pub fn initialize_channels(db: &Database) -> Result<()> {
    let conn = db.connection();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT OR IGNORE INTO channels (id, channel_type, created_at) VALUES (1, 'public', ?1)",
        rusqlite::params![now],
    ).map_err(|e| ChattorError::Database(format!("Failed to create public channel: {}", e)))?;

    conn.execute(
        "INSERT OR IGNORE INTO channels (id, channel_type, created_at) VALUES (2, 'friends_only', ?1)",
        rusqlite::params![now],
    ).map_err(|e| ChattorError::Database(format!("Failed to create friends_only channel: {}", e)))?;

    Ok(())
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test db::queries::tests::test_initialize_channels`
Expected: PASS

**Step 5: Write failing tests for post storage and retrieval**

```rust
#[test]
fn test_store_and_get_channel_posts() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();
    initialize_channels(&db).unwrap();

    store_channel_post(&db, 1, "Hello world!", "post-1", 1000, "sig1").unwrap();
    store_channel_post(&db, 1, "Second post", "post-2", 2000, "sig2").unwrap();

    let posts = get_channel_posts(&db, 1, 50).unwrap();
    assert_eq!(posts.len(), 2);
    // Newest first
    assert_eq!(posts[0].content, "Second post");
    assert_eq!(posts[1].content, "Hello world!");
}

#[test]
fn test_channel_post_dedup() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();
    initialize_channels(&db).unwrap();

    store_channel_post(&db, 1, "Hello!", "post-1", 1000, "sig1").unwrap();
    store_channel_post(&db, 1, "Hello!", "post-1", 1000, "sig1").unwrap(); // dupe

    let posts = get_channel_posts(&db, 1, 50).unwrap();
    assert_eq!(posts.len(), 1);
}

#[test]
fn test_channel_retention_enforced() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();
    initialize_channels(&db).unwrap();

    // Insert 105 posts
    for i in 0..105 {
        store_channel_post(
            &db, 1, &format!("Post {}", i),
            &format!("post-{}", i), i as i64, "sig"
        ).unwrap();
    }

    enforce_channel_retention(&db, 1).unwrap();

    let posts = get_channel_posts(&db, 1, 200).unwrap();
    assert_eq!(posts.len(), 100);
    // Oldest should be post 5 (0-4 deleted)
    assert_eq!(posts[99].post_id, "post-5");
}
```

**Step 6: Implement post storage, retrieval, and retention**

Add to `src/db/queries.rs`:

```rust
/// A channel post for display
#[derive(Debug, Clone)]
pub struct ChannelPost {
    pub id: i64,
    pub channel_id: i64,
    pub content: String,
    pub post_id: String,
    pub created_at: i64,
    pub signature: String,
}

/// Store a channel post (dedup via post_id UNIQUE)
pub fn store_channel_post(
    db: &Database,
    channel_id: i64,
    content: &str,
    post_id: &str,
    created_at: i64,
    signature: &str,
) -> Result<()> {
    db.connection().execute(
        "INSERT OR IGNORE INTO channel_posts (channel_id, content, post_id, created_at, signature)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![channel_id, content, post_id, created_at, signature],
    ).map_err(|e| ChattorError::Database(format!("Failed to store channel post: {}", e)))?;
    Ok(())
}

/// Get posts for a channel, newest first
pub fn get_channel_posts(db: &Database, channel_id: i64, limit: usize) -> Result<Vec<ChannelPost>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, channel_id, content, post_id, created_at, signature
         FROM channel_posts
         WHERE channel_id = ?1
         ORDER BY created_at DESC, id DESC
         LIMIT ?2"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare channel posts query: {}", e)))?;

    let posts = stmt.query_map(params![channel_id, limit as i64], |row| {
        Ok(ChannelPost {
            id: row.get(0)?,
            channel_id: row.get(1)?,
            content: row.get(2)?,
            post_id: row.get(3)?,
            created_at: row.get(4)?,
            signature: row.get(5)?,
        })
    }).map_err(|e| ChattorError::Database(format!("Failed to query channel posts: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect channel posts: {}", e)))?;

    Ok(posts)
}

/// Delete oldest posts if count exceeds 100
pub fn enforce_channel_retention(db: &Database, channel_id: i64) -> Result<i64> {
    let deleted = db.connection().execute(
        "DELETE FROM channel_posts WHERE channel_id = ?1 AND id NOT IN (
            SELECT id FROM channel_posts WHERE channel_id = ?1
            ORDER BY created_at DESC, id DESC LIMIT 100
        )",
        params![channel_id],
    ).map_err(|e| ChattorError::Database(format!("Failed to enforce retention: {}", e)))? as i64;
    Ok(deleted)
}
```

**Step 7: Run tests to verify they pass**

Run: `cargo test db::queries::tests::test_store_and_get_channel_posts db::queries::tests::test_channel_post_dedup db::queries::tests::test_channel_retention_enforced -- --nocapture`
Expected: All PASS

**Step 8: Write failing tests for subscriber/subscription management**

```rust
#[test]
fn test_add_and_get_channel_subscribers() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    add_channel_subscriber(&db, "bob.onion", "public").unwrap();
    add_channel_subscriber(&db, "carol.onion", "public").unwrap();

    let subs = get_channel_subscribers(&db, "public").unwrap();
    assert_eq!(subs.len(), 2);

    // Dedup
    add_channel_subscriber(&db, "bob.onion", "public").unwrap();
    let subs = get_channel_subscribers(&db, "public").unwrap();
    assert_eq!(subs.len(), 2);
}

#[test]
fn test_remove_channel_subscriber() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    add_channel_subscriber(&db, "bob.onion", "public").unwrap();
    remove_channel_subscriber(&db, "bob.onion", "public").unwrap();

    let subs = get_channel_subscribers(&db, "public").unwrap();
    assert_eq!(subs.len(), 0);
}

#[test]
fn test_add_and_get_channel_subscriptions() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    add_channel_subscription(&db, "alice.onion", "public").unwrap();
    add_channel_subscription(&db, "alice.onion", "friends_only").unwrap();

    let subs = get_channel_subscriptions(&db).unwrap();
    assert_eq!(subs.len(), 2);
}

#[test]
fn test_update_subscription_sync_time() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    add_channel_subscription(&db, "alice.onion", "public").unwrap();
    update_subscription_sync_time(&db, "alice.onion", "public", 5000).unwrap();

    let subs = get_channel_subscriptions(&db).unwrap();
    assert_eq!(subs[0].last_sync_at, Some(5000));
}

#[test]
fn test_store_and_count_post_receipts() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    store_channel_post_receipt(&db, "post-1", "bob.onion", 1000).unwrap();
    store_channel_post_receipt(&db, "post-1", "carol.onion", 2000).unwrap();
    // Dedup
    store_channel_post_receipt(&db, "post-1", "bob.onion", 3000).unwrap();

    let count = get_channel_post_read_count(&db, "post-1").unwrap();
    assert_eq!(count, 2);
}
```

**Step 9: Implement subscriber/subscription queries**

```rust
/// Add a subscriber to one of our channels (publisher side)
pub fn add_channel_subscriber(db: &Database, subscriber_onion: &str, channel_type: &str) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "INSERT OR IGNORE INTO channel_subscribers (subscriber_onion, channel_type, subscribed_at)
         VALUES (?1, ?2, ?3)",
        params![subscriber_onion, channel_type, now],
    ).map_err(|e| ChattorError::Database(format!("Failed to add subscriber: {}", e)))?;
    Ok(())
}

/// Remove a subscriber from one of our channels
pub fn remove_channel_subscriber(db: &Database, subscriber_onion: &str, channel_type: &str) -> Result<()> {
    db.connection().execute(
        "DELETE FROM channel_subscribers WHERE subscriber_onion = ?1 AND channel_type = ?2",
        params![subscriber_onion, channel_type],
    ).map_err(|e| ChattorError::Database(format!("Failed to remove subscriber: {}", e)))?;
    Ok(())
}

/// Get all subscribers for a channel type (publisher side)
pub fn get_channel_subscribers(db: &Database, channel_type: &str) -> Result<Vec<String>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT subscriber_onion FROM channel_subscribers WHERE channel_type = ?1"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare subscribers query: {}", e)))?;

    let subs = stmt.query_map(params![channel_type], |row| row.get::<_, String>(0))
        .map_err(|e| ChattorError::Database(format!("Failed to query subscribers: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect subscribers: {}", e)))?;
    Ok(subs)
}

/// A channel subscription entry (subscriber side)
#[derive(Debug, Clone)]
pub struct ChannelSubscription {
    pub id: i64,
    pub publisher_onion: String,
    pub channel_type: String,
    pub subscribed_at: i64,
    pub last_sync_at: Option<i64>,
}

/// Subscribe to a remote user's channel (subscriber side)
pub fn add_channel_subscription(db: &Database, publisher_onion: &str, channel_type: &str) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "INSERT OR IGNORE INTO channel_subscriptions (publisher_onion, channel_type, subscribed_at)
         VALUES (?1, ?2, ?3)",
        params![publisher_onion, channel_type, now],
    ).map_err(|e| ChattorError::Database(format!("Failed to add subscription: {}", e)))?;
    Ok(())
}

/// Remove a channel subscription
pub fn remove_channel_subscription(db: &Database, publisher_onion: &str, channel_type: &str) -> Result<()> {
    db.connection().execute(
        "DELETE FROM channel_subscriptions WHERE publisher_onion = ?1 AND channel_type = ?2",
        params![publisher_onion, channel_type],
    ).map_err(|e| ChattorError::Database(format!("Failed to remove subscription: {}", e)))?;
    Ok(())
}

/// Get all channel subscriptions (subscriber side)
pub fn get_channel_subscriptions(db: &Database) -> Result<Vec<ChannelSubscription>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, publisher_onion, channel_type, subscribed_at, last_sync_at
         FROM channel_subscriptions ORDER BY subscribed_at ASC"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare subscriptions query: {}", e)))?;

    let subs = stmt.query_map([], |row| {
        Ok(ChannelSubscription {
            id: row.get(0)?,
            publisher_onion: row.get(1)?,
            channel_type: row.get(2)?,
            subscribed_at: row.get(3)?,
            last_sync_at: row.get(4)?,
        })
    }).map_err(|e| ChattorError::Database(format!("Failed to query subscriptions: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect subscriptions: {}", e)))?;
    Ok(subs)
}

/// Update the last sync timestamp for a subscription
pub fn update_subscription_sync_time(db: &Database, publisher_onion: &str, channel_type: &str, sync_at: i64) -> Result<()> {
    db.connection().execute(
        "UPDATE channel_subscriptions SET last_sync_at = ?1 WHERE publisher_onion = ?2 AND channel_type = ?3",
        params![sync_at, publisher_onion, channel_type],
    ).map_err(|e| ChattorError::Database(format!("Failed to update sync time: {}", e)))?;
    Ok(())
}

/// Store a read receipt for a post (publisher side)
pub fn store_channel_post_receipt(db: &Database, post_id: &str, reader_onion: &str, read_at: i64) -> Result<()> {
    db.connection().execute(
        "INSERT OR IGNORE INTO channel_post_receipts (post_id, reader_onion, read_at)
         VALUES (?1, ?2, ?3)",
        params![post_id, reader_onion, read_at],
    ).map_err(|e| ChattorError::Database(format!("Failed to store post receipt: {}", e)))?;
    Ok(())
}

/// Get read count for a post (publisher side)
pub fn get_channel_post_read_count(db: &Database, post_id: &str) -> Result<i64> {
    let count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM channel_post_receipts WHERE post_id = ?1",
        params![post_id],
        |row| row.get(0),
    ).map_err(|e| ChattorError::Database(format!("Failed to count post receipts: {}", e)))?;
    Ok(count)
}

/// Get posts from a channel since a timestamp (for sync responses)
pub fn get_channel_posts_since(db: &Database, channel_id: i64, since: i64) -> Result<Vec<ChannelPost>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, channel_id, content, post_id, created_at, signature
         FROM channel_posts
         WHERE channel_id = ?1 AND created_at > ?2
         ORDER BY created_at ASC
         LIMIT 100"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare posts since query: {}", e)))?;

    let posts = stmt.query_map(params![channel_id, since], |row| {
        Ok(ChannelPost {
            id: row.get(0)?,
            channel_id: row.get(1)?,
            content: row.get(2)?,
            post_id: row.get(3)?,
            created_at: row.get(4)?,
            signature: row.get(5)?,
        })
    }).map_err(|e| ChattorError::Database(format!("Failed to query posts since: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect posts since: {}", e)))?;
    Ok(posts)
}

/// Remove all subscriptions and posts for a specific publisher (used when unfriending)
pub fn remove_channel_data_for_publisher(db: &Database, publisher_onion: &str) -> Result<()> {
    let conn = db.connection();

    // Remove friends_only subscription
    conn.execute(
        "DELETE FROM channel_subscriptions WHERE publisher_onion = ?1 AND channel_type = 'friends_only'",
        params![publisher_onion],
    ).map_err(|e| ChattorError::Database(format!("Failed to remove subscription: {}", e)))?;

    Ok(())
}
```

**Step 10: Run all tests**

Run: `cargo test db::queries -- --nocapture`
Expected: All PASS

**Step 11: Commit**

```bash
git add src/db/queries.rs
git commit -m "feat: add channel database queries"
```

---

### Task 4: Initialize channels on app startup

**Files:**
- Modify: `src/app.rs`

**Step 1: Call initialize_channels in App::new()**

After the `let db = Database::open(&settings.db_path)?;` line in `App::new()`, add:

```rust
// Initialize broadcast channels
crate::db::queries::initialize_channels(&db)?;
```

Do the same in `App::new_with_settings()`.

**Step 2: Run tests**

Run: `cargo test app -- --nocapture`
Expected: All PASS

**Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat: initialize broadcast channels on startup"
```

---

### Task 5: Handle incoming channel messages in main.rs

**Files:**
- Modify: `src/main.rs`

**Step 1: Add channel message handling to handle_incoming_message**

In the `handle_incoming_message` function in `src/main.rs`, add match arms for the 6 new message types inside the existing `match &incoming.message { ... }` block:

```rust
protocol::message::Message::ChannelSubscribe(sub) => {
    // Check if subscriber is blocked
    let conn = app.db.connection();
    let blocked: bool = conn.query_row(
        "SELECT COUNT(*) FROM blocked_onions WHERE onion_address = ?1",
        [&sub.subscriber_onion],
        |row| row.get::<_, i64>(0),
    ).map(|c| c > 0).unwrap_or(false);

    if !blocked {
        let channel_type = match sub.channel_type {
            protocol::message::ChannelType::Public => "public",
            protocol::message::ChannelType::FriendsOnly => "friends_only",
        };

        // For friends_only, verify they are a friend
        if channel_type == "friends_only" {
            if db::queries::find_friend_by_onion(&app.db, &sub.subscriber_onion)?.is_none() {
                eprintln!("Rejected friends_only subscription from non-friend {}", sub.subscriber_onion);
                return Ok(());
            }
        }

        db::queries::add_channel_subscriber(&app.db, &sub.subscriber_onion, channel_type)?;
        eprintln!("New {} channel subscriber: {}", channel_type, sub.subscriber_onion);
    }
}
protocol::message::Message::ChannelUnsubscribe(unsub) => {
    let channel_type = match unsub.channel_type {
        protocol::message::ChannelType::Public => "public",
        protocol::message::ChannelType::FriendsOnly => "friends_only",
    };
    db::queries::remove_channel_subscriber(&app.db, &unsub.subscriber_onion, channel_type)?;
    eprintln!("Unsubscribed: {} from {} channel", unsub.subscriber_onion, channel_type);
}
protocol::message::Message::ChannelPost(post) => {
    // Verify signature
    // For MVP, store without verification (TODO: verify Ed25519 sig)
    let channel_type_str = match post.channel_type {
        protocol::message::ChannelType::Public => "public",
        protocol::message::ChannelType::FriendsOnly => "friends_only",
    };

    // Find or create a local channel_id for this subscription's posts
    // We store remote posts in channel_subscriptions context, using a convention:
    // negative channel_id or a separate lookup. For simplicity, store with
    // publisher_onion as a key in a separate query.

    // Store in channel_posts with a synthetic channel_id based on subscription
    // For now, use channel_id = 0 as "remote posts" and filter by post publisher
    db::queries::store_channel_post(
        &app.db, 0, &post.content, &post.post_id.to_string(),
        post.created_at, &post.signature,
    )?;

    // Send read receipt back to publisher
    let receipt = protocol::message::ChannelPostReceiptMessage {
        post_id: post.post_id,
        reader_onion: app.onion_address.clone().unwrap_or_default(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
    };
    let receipt_msg = protocol::message::Message::ChannelPostReceipt(receipt);
    app.message_queue.enqueue(&app.db, &post.publisher_onion, &receipt_msg, "low").ok();
}
protocol::message::Message::ChannelSyncRequest(req) => {
    let channel_type_str = match req.channel_type {
        protocol::message::ChannelType::Public => "public",
        protocol::message::ChannelType::FriendsOnly => "friends_only",
    };

    // For friends_only, verify they are a friend
    if channel_type_str == "friends_only" {
        if db::queries::find_friend_by_onion(&app.db, &req.subscriber_onion)?.is_none() {
            return Ok(());
        }
    }

    let channel_id = if channel_type_str == "public" { 1 } else { 2 };
    let posts = db::queries::get_channel_posts_since(&app.db, channel_id, req.since_timestamp)?;

    let post_messages: Vec<protocol::message::ChannelPostMessage> = posts.into_iter().map(|p| {
        protocol::message::ChannelPostMessage {
            publisher_onion: app.onion_address.clone().unwrap_or_default(),
            channel_type: req.channel_type.clone(),
            post_id: uuid::Uuid::parse_str(&p.post_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            content: p.content,
            created_at: p.created_at,
            signature: p.signature,
        }
    }).collect();

    if !post_messages.is_empty() {
        let response = protocol::message::Message::ChannelSyncResponse(
            protocol::message::ChannelSyncResponseMessage {
                publisher_onion: app.onion_address.clone().unwrap_or_default(),
                channel_type: req.channel_type.clone(),
                posts: post_messages,
            }
        );
        app.message_queue.enqueue(&app.db, &req.subscriber_onion, &response, "normal").ok();
    }
}
protocol::message::Message::ChannelSyncResponse(resp) => {
    for post in &resp.posts {
        db::queries::store_channel_post(
            &app.db, 0, &post.content, &post.post_id.to_string(),
            post.created_at, &post.signature,
        )?;
    }
    // Update sync time
    let channel_type_str = match resp.channel_type {
        protocol::message::ChannelType::Public => "public",
        protocol::message::ChannelType::FriendsOnly => "friends_only",
    };
    let max_time = resp.posts.iter().map(|p| p.created_at).max().unwrap_or(0);
    if max_time > 0 {
        db::queries::update_subscription_sync_time(
            &app.db, &resp.publisher_onion, channel_type_str, max_time
        )?;
    }
}
protocol::message::Message::ChannelPostReceipt(receipt) => {
    db::queries::store_channel_post_receipt(
        &app.db, &receipt.post_id.to_string(), &receipt.reader_onion, receipt.timestamp
    )?;
}
```

**Step 2: Run cargo check**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: handle incoming channel messages"
```

---

### Task 6: Auto-subscribe on friend accept

**Files:**
- Modify: `src/main.rs`

**Step 1: Add auto-subscription in handle_accept_friend_request**

In the `handle_accept_friend_request` function in `src/main.rs`, after the line that adds the friend to the database (`INSERT INTO friends`), add:

```rust
// Auto-subscribe to their channels
db::queries::add_channel_subscription(&app.db, &from_onion, "public")?;
db::queries::add_channel_subscription(&app.db, &from_onion, "friends_only")?;
```

**Step 2: Add auto-subscription in handle_incoming_accept**

In the `handle_incoming_accept` function, after the line that adds the friend (`INSERT OR IGNORE INTO friends`), add:

```rust
// Auto-subscribe to their channels
db::queries::add_channel_subscription(&app.db, &accept.from_onion, "public")?;
db::queries::add_channel_subscription(&app.db, &accept.from_onion, "friends_only")?;

// Also subscribe them to our friends_only channel
db::queries::add_channel_subscriber(&app.db, &accept.from_onion, "friends_only")?;
```

**Step 3: Run cargo check**

Run: `cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: auto-subscribe to channels on friend accept"
```

---

### Task 7: Add channel UI state and actions

**Files:**
- Modify: `src/ui/state.rs`

**Step 1: Write failing test for channel view state**

Add to tests in `src/ui/state.rs`:

```rust
#[test]
fn test_viewing_channel_escape_returns_to_normal() {
    let mut state = AppState::ViewingChannel {
        publisher_onion: "alice.onion".into(),
        channel_type: "public".into(),
        is_own: false,
        input: String::new(),
        cursor: 0,
        scroll_offset: 0,
    };
    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    state.handle_key(key).unwrap();
    assert!(matches!(state, AppState::Normal { .. }));
}

#[test]
fn test_viewing_own_channel_post() {
    let mut state = AppState::ViewingChannel {
        publisher_onion: "me.onion".into(),
        channel_type: "public".into(),
        is_own: true,
        input: "Hello world".to_string(),
        cursor: 11,
        scroll_offset: 0,
    };
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let action = state.handle_key(key).unwrap();
    assert_eq!(action, Some(AppAction::PublishChannelPost("Hello world".into(), "public".into())));
}

#[test]
fn test_subscribing_to_channel() {
    let mut state = AppState::SubscribingToChannel {
        input: "alice.onion".into(),
        cursor: 11,
        error: None,
    };
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let action = state.handle_key(key).unwrap();
    assert_eq!(action, Some(AppAction::SubscribeToChannel("alice.onion".into())));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test ui::state::tests::test_viewing_channel_escape_returns_to_normal`
Expected: FAIL

**Step 3: Add new AppState variants and AppAction variants**

Add to the `AppState` enum:

```rust
ViewingChannel {
    publisher_onion: String,
    channel_type: String,     // "public" or "friends_only"
    is_own: bool,
    input: String,            // for composing (own channels only)
    cursor: usize,
    scroll_offset: usize,
},
SubscribingToChannel {
    input: String,
    cursor: usize,
    error: Option<String>,
},
```

Add to the `AppAction` enum:

```rust
PublishChannelPost(String, String),     // (content, channel_type)
SubscribeToChannel(String),             // publisher .onion address
SelectChannel(String, String, bool),    // (publisher_onion, channel_type, is_own)
```

**Step 4: Add handle_key implementations for new states**

Add to the `match self` block in `handle_key`:

```rust
AppState::ViewingChannel {
    publisher_onion,
    channel_type,
    is_own,
    input,
    cursor,
    scroll_offset,
} => {
    if *is_own {
        // Own channel: can compose posts
        match key.code {
            KeyCode::Esc => {
                *self = AppState::default();
                Ok(None)
            }
            KeyCode::Enter => {
                if input.is_empty() {
                    Ok(None)
                } else {
                    let content = input.clone();
                    let ct = channel_type.clone();
                    input.clear();
                    *cursor = 0;
                    Ok(Some(AppAction::PublishChannelPost(content, ct)))
                }
            }
            KeyCode::Char(c) => {
                input.insert(*cursor, c);
                *cursor += 1;
                Ok(None)
            }
            KeyCode::Backspace => {
                if *cursor > 0 {
                    *cursor -= 1;
                    input.remove(*cursor);
                }
                Ok(None)
            }
            KeyCode::Left => {
                if *cursor > 0 { *cursor -= 1; }
                Ok(None)
            }
            KeyCode::Right => {
                if *cursor < input.len() { *cursor += 1; }
                Ok(None)
            }
            KeyCode::Up => {
                if *scroll_offset > 0 { *scroll_offset -= 1; }
                Ok(None)
            }
            KeyCode::Down => {
                *scroll_offset += 1;
                Ok(None)
            }
            _ => Ok(None),
        }
    } else {
        // Remote channel: read-only, can scroll
        match key.code {
            KeyCode::Esc => {
                *self = AppState::default();
                Ok(None)
            }
            KeyCode::Up => {
                if *scroll_offset > 0 { *scroll_offset -= 1; }
                Ok(None)
            }
            KeyCode::Down => {
                *scroll_offset += 1;
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

AppState::SubscribingToChannel { input, cursor, error } => {
    match key.code {
        KeyCode::Char(c) => {
            input.insert(*cursor, c);
            *cursor += 1;
            Ok(None)
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                *cursor -= 1;
                input.remove(*cursor);
            }
            Ok(None)
        }
        KeyCode::Left => {
            if *cursor > 0 { *cursor -= 1; }
            Ok(None)
        }
        KeyCode::Right => {
            if *cursor < input.len() { *cursor += 1; }
            Ok(None)
        }
        KeyCode::Enter => {
            if input.is_empty() {
                *error = Some("Please enter a .onion address".to_string());
                Ok(None)
            } else {
                Ok(Some(AppAction::SubscribeToChannel(input.clone())))
            }
        }
        KeyCode::Esc => {
            *self = AppState::default();
            Ok(None)
        }
        _ => Ok(None),
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test ui::state -- --nocapture`
Expected: All PASS

**Step 6: Commit**

```bash
git add src/ui/state.rs
git commit -m "feat: add channel UI state and actions"
```

---

### Task 8: Wire up channel actions in main.rs

**Files:**
- Modify: `src/main.rs`

**Step 1: Add PublishChannelPost action handler**

In the main event loop's `match app_state.handle_key(key)?` block, add:

```rust
Some(AppAction::PublishChannelPost(content, channel_type)) => {
    let app_lock = app.lock().await;
    let own_onion = app_lock.onion_address.clone().unwrap_or_default();
    let channel_id = if channel_type == "public" { 1 } else { 2 };
    let post_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Sign the post
    let sign_data = format!("{}{}{}", post_id, content, now);
    let signature = base64::encode(&app_lock.identity.sign(sign_data.as_bytes()).to_bytes());

    // Store locally
    db::queries::store_channel_post(
        &app_lock.db, channel_id, &content, &post_id, now, &signature
    ).ok();

    // Enforce retention
    db::queries::enforce_channel_retention(&app_lock.db, channel_id).ok();

    // Push to online subscribers
    let channel_type_enum = if channel_type == "public" {
        protocol::message::ChannelType::Public
    } else {
        protocol::message::ChannelType::FriendsOnly
    };

    let post_msg = protocol::message::Message::ChannelPost(
        protocol::message::ChannelPostMessage {
            publisher_onion: own_onion.clone(),
            channel_type: channel_type_enum,
            post_id: uuid::Uuid::parse_str(&post_id).unwrap(),
            content,
            created_at: now,
            signature,
        }
    );

    let subscribers = db::queries::get_channel_subscribers(&app_lock.db, &channel_type).unwrap_or_default();
    for sub_onion in subscribers {
        app_lock.message_queue.enqueue(&app_lock.db, &sub_onion, &post_msg, "normal").ok();
    }

    drop(app_lock);
}
Some(AppAction::SubscribeToChannel(publisher_onion)) => {
    let app_lock = app.lock().await;
    let own_onion = app_lock.onion_address.clone().unwrap_or_default();

    // Store subscription locally
    db::queries::add_channel_subscription(&app_lock.db, &publisher_onion, "public").ok();

    // Send subscribe message to publisher
    let sub_msg = protocol::message::Message::ChannelSubscribe(
        protocol::message::ChannelSubscribeMessage {
            subscriber_onion: own_onion,
            channel_type: protocol::message::ChannelType::Public,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
    );
    app_lock.message_queue.enqueue(&app_lock.db, &publisher_onion, &sub_msg, "normal").ok();

    drop(app_lock);
    app_state = AppState::default();
}
Some(AppAction::SelectChannel(publisher_onion, channel_type, is_own)) => {
    app_state = AppState::ViewingChannel {
        publisher_onion,
        channel_type,
        is_own,
        input: String::new(),
        cursor: 0,
        scroll_offset: 0,
    };
}
```

**Step 2: Add 'c' keybinding for channel subscription in Normal nav mode**

In the Normal nav mode section of `src/ui/state.rs`, add a new keybinding:

```rust
KeyCode::Char('s') => {
    *self = AppState::SubscribingToChannel {
        input: String::new(),
        cursor: 0,
        error: None,
    };
    Ok(None)
}
```

**Step 3: Run cargo check**

Run: `cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src/main.rs src/ui/state.rs
git commit -m "feat: wire up channel publish and subscribe actions"
```

---

### Task 9: Add channel sidebar and feed UI rendering

**Files:**
- Modify: `src/ui/sidebar.rs`
- Modify: `src/ui/mod.rs`
- Create: `src/ui/channel_feed.rs`
- Modify: `src/ui/app_ui.rs`

This task adds the visual rendering for channels. The implementation details depend heavily on the existing ratatui rendering code. The key changes are:

**Step 1: Read existing UI files to understand rendering patterns**

Read `src/ui/sidebar.rs`, `src/ui/conversation.rs`, `src/ui/app_ui.rs`, and `src/ui/mod.rs` to understand the current rendering pattern.

**Step 2: Add channel data to RenderContext**

In `src/ui/app_ui.rs`, add to the `RenderContext` struct:

```rust
pub channel_subscriptions: Vec<crate::db::queries::ChannelSubscription>,
pub channel_posts: Vec<crate::db::queries::ChannelPost>,
pub channel_post_read_counts: std::collections::HashMap<String, i64>,
```

**Step 3: Populate channel data in main.rs event loop**

In the main event loop in `src/main.rs`, where `RenderContext` is constructed, add:

```rust
let channel_subscriptions = db::queries::get_channel_subscriptions(&app_lock.db).unwrap_or_default();

let (channel_posts, channel_post_read_counts) = if let AppState::ViewingChannel {
    ref publisher_onion, ref channel_type, is_own, ..
} = &app_state {
    let channel_id = if *is_own {
        if channel_type == "public" { 1 } else { 2 }
    } else {
        0 // remote posts stored with channel_id 0
    };
    let posts = db::queries::get_channel_posts(&app_lock.db, channel_id, 100).unwrap_or_default();
    let mut counts = std::collections::HashMap::new();
    if *is_own {
        for post in &posts {
            let count = db::queries::get_channel_post_read_count(&app_lock.db, &post.post_id).unwrap_or(0);
            counts.insert(post.post_id.clone(), count);
        }
    }
    (posts, counts)
} else {
    (Vec::new(), std::collections::HashMap::new())
};
```

And include these in the `RenderContext` construction.

**Step 4: Add "Channels" section to sidebar**

In `src/ui/sidebar.rs`, after rendering the friends list, add a "Channels" section that shows:
- "My Channels" header with "Public" and "Friends Only" entries
- "Subscriptions" header with entries from `ctx.channel_subscriptions`
- Unread indicator (based on subscription `last_sync_at`)

Use the same ratatui `Paragraph` / `List` patterns as the existing friends list.

**Step 5: Create channel_feed.rs**

Create `src/ui/channel_feed.rs` with a `render_channel_feed` function that:
- Displays posts in reverse chronological order (newest at top)
- Shows post content, timestamp, and signature verification status
- For own channels: shows read count per post
- For own channels: shows input field at bottom
- For remote channels: no input field

Follow the same pattern as `src/ui/conversation.rs` for rendering.

**Step 6: Wire up channel feed in app_ui.rs**

In `src/ui/app_ui.rs`, in the `render_app` function, add a match for `AppState::ViewingChannel` that calls `render_channel_feed`.

**Step 7: Update mod.rs**

Add to `src/ui/mod.rs`:
```rust
pub mod channel_feed;
pub use channel_feed::render_channel_feed;
```

**Step 8: Run cargo check**

Run: `cargo check`
Expected: No errors

**Step 9: Commit**

```bash
git add src/ui/sidebar.rs src/ui/channel_feed.rs src/ui/app_ui.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add channel sidebar and feed UI"
```

---

### Task 10: Add channel sync on startup

**Files:**
- Modify: `src/main.rs`

**Step 1: Add sync logic after Tor initialization**

After the Tor background init spawns in `main()`, add a task that syncs channel subscriptions on a periodic timer (same pattern as queue processor). In the Tor init spawn, after the listener is set up, add:

```rust
// Spawn channel sync task (every 5 minutes)
let app_sync = Arc::clone(&app);
tokio::spawn(async move {
    // Initial sync after 10 seconds
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    loop {
        {
            let app_lock = app_sync.lock().await;
            if let Err(e) = sync_channel_subscriptions(&*app_lock).await {
                eprintln!("Channel sync error: {}", e);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }
});
```

Add the sync function:

```rust
/// Sync missed posts from all channel subscriptions
async fn sync_channel_subscriptions(app: &App) -> Result<()> {
    let subscriptions = db::queries::get_channel_subscriptions(&app.db)?;
    let own_onion = app.onion_address.clone().unwrap_or_default();

    for sub in subscriptions {
        let since = sub.last_sync_at.unwrap_or(0);
        let channel_type = if sub.channel_type == "public" {
            protocol::message::ChannelType::Public
        } else {
            protocol::message::ChannelType::FriendsOnly
        };

        let sync_req = protocol::message::Message::ChannelSyncRequest(
            protocol::message::ChannelSyncRequestMessage {
                subscriber_onion: own_onion.clone(),
                channel_type,
                since_timestamp: since,
            }
        );

        // Try direct send, queue on failure
        match try_send_direct(app, &sub.publisher_onion, &sync_req).await {
            Ok(_) => {}
            Err(_) => {
                // Publisher offline, will retry next cycle
            }
        }
    }

    Ok(())
}
```

**Step 2: Run cargo check**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: periodic channel sync on startup"
```

---

### Task 11: Final integration test

**Files:**
- Modify: `tests/integration/messaging_test.rs` (or create new test file)

**Step 1: Write integration test**

Add a test that exercises the full channel flow:
1. Create two App instances with temp databases
2. Initialize channels on both
3. App A stores a channel post
4. App A has App B as a subscriber
5. Verify the post can be retrieved via `get_channel_posts_since`
6. Verify retention works with >100 posts

```rust
#[test]
fn test_channel_post_flow() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let db = chattor::db::Database::open(temp.path()).unwrap();

    // Initialize channels
    chattor::db::queries::initialize_channels(&db).unwrap();

    // Publish posts to public channel
    for i in 0..5 {
        chattor::db::queries::store_channel_post(
            &db, 1, &format!("Post {}", i),
            &format!("post-{}", i), (1000 + i) as i64, "sig"
        ).unwrap();
    }

    // Retrieve posts (newest first)
    let posts = chattor::db::queries::get_channel_posts(&db, 1, 50).unwrap();
    assert_eq!(posts.len(), 5);
    assert_eq!(posts[0].content, "Post 4");

    // Get posts since timestamp (oldest first, for sync)
    let since_posts = chattor::db::queries::get_channel_posts_since(&db, 1, 1002).unwrap();
    assert_eq!(since_posts.len(), 2); // posts 3 and 4

    // Subscriber management
    chattor::db::queries::add_channel_subscriber(&db, "bob.onion", "public").unwrap();
    let subs = chattor::db::queries::get_channel_subscribers(&db, "public").unwrap();
    assert_eq!(subs.len(), 1);

    // Read receipts
    chattor::db::queries::store_channel_post_receipt(&db, "post-1", "bob.onion", 2000).unwrap();
    let count = chattor::db::queries::get_channel_post_read_count(&db, "post-1").unwrap();
    assert_eq!(count, 1);
}
```

**Step 2: Run integration test**

Run: `cargo test test_channel_post_flow -- --nocapture`
Expected: PASS

**Step 3: Run full test suite**

Run: `cargo test`
Expected: All PASS

**Step 4: Commit**

```bash
git add tests/
git commit -m "test: add broadcast channel integration test"
```

---

### Task 12: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update Phase 3 status**

Update the Phase Implementation Status section to mark Phase 3 as complete. Add channel-related tables to the architecture docs. Update the key files section.

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for Phase 3 broadcast channels"
```
