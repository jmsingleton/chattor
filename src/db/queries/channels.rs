use crate::db::Database;
use crate::error::{ChattorError, Result};
use rusqlite::params;
use std::collections::HashMap;

/// A channel post for display
#[derive(Debug, Clone)]
pub struct ChannelPost {
    #[allow(dead_code)]
    pub id: i64,
    #[allow(dead_code)]
    pub channel_id: i64,
    pub content: String,
    pub post_id: String,
    pub created_at: i64,
    pub signature: String,
}

/// A channel subscription entry (subscriber side)
#[derive(Debug, Clone)]
pub struct ChannelSubscription {
    #[allow(dead_code)]
    pub id: i64,
    pub publisher_onion: String,
    pub channel_type: String,
    #[allow(dead_code)]
    pub subscribed_at: i64,
    pub last_sync_at: Option<i64>,
}

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
    )
    .map_err(|e| ChattorError::Database(format!("Failed to create public channel: {}", e)))?;

    conn.execute(
        "INSERT OR IGNORE INTO channels (id, channel_type, created_at) VALUES (2, 'friends_only', ?1)",
        rusqlite::params![now],
    ).map_err(|e| ChattorError::Database(format!("Failed to create friends_only channel: {}", e)))?;

    Ok(())
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
    let mut stmt = conn
        .prepare(
            "SELECT id, channel_id, content, post_id, created_at, signature
         FROM channel_posts
         WHERE channel_id = ?1
         ORDER BY created_at DESC, id DESC
         LIMIT ?2",
        )
        .map_err(|e| {
            ChattorError::Database(format!("Failed to prepare channel posts query: {}", e))
        })?;

    let posts = stmt
        .query_map(params![channel_id, limit as i64], |row| {
            Ok(ChannelPost {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                content: row.get(2)?,
                post_id: row.get(3)?,
                created_at: row.get(4)?,
                signature: row.get(5)?,
            })
        })
        .map_err(|e| ChattorError::Database(format!("Failed to query channel posts: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect channel posts: {}", e)))?;

    Ok(posts)
}

/// Delete oldest posts if count exceeds 100
pub fn enforce_channel_retention(db: &Database, channel_id: i64) -> Result<i64> {
    let deleted = db
        .connection()
        .execute(
            "DELETE FROM channel_posts WHERE channel_id = ?1 AND id NOT IN (
            SELECT id FROM channel_posts WHERE channel_id = ?1
            ORDER BY created_at DESC, id DESC LIMIT 100
        )",
            params![channel_id],
        )
        .map_err(|e| ChattorError::Database(format!("Failed to enforce retention: {}", e)))?
        as i64;
    Ok(deleted)
}

/// Add a subscriber to one of our channels (publisher side)
pub fn add_channel_subscriber(
    db: &Database,
    subscriber_onion: &str,
    channel_type: &str,
) -> Result<()> {
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
pub fn remove_channel_subscriber(
    db: &Database,
    subscriber_onion: &str,
    channel_type: &str,
) -> Result<()> {
    db.connection()
        .execute(
            "DELETE FROM channel_subscribers WHERE subscriber_onion = ?1 AND channel_type = ?2",
            params![subscriber_onion, channel_type],
        )
        .map_err(|e| ChattorError::Database(format!("Failed to remove subscriber: {}", e)))?;
    Ok(())
}

/// Get all subscribers for a channel type (publisher side)
pub fn get_channel_subscribers(db: &Database, channel_type: &str) -> Result<Vec<String>> {
    let conn = db.connection();
    let mut stmt = conn
        .prepare("SELECT subscriber_onion FROM channel_subscribers WHERE channel_type = ?1")
        .map_err(|e| {
            ChattorError::Database(format!("Failed to prepare subscribers query: {}", e))
        })?;

    let subs = stmt
        .query_map(params![channel_type], |row| row.get::<_, String>(0))
        .map_err(|e| ChattorError::Database(format!("Failed to query subscribers: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect subscribers: {}", e)))?;
    Ok(subs)
}

/// Subscribe to a remote user's channel (subscriber side)
pub fn add_channel_subscription(
    db: &Database,
    publisher_onion: &str,
    channel_type: &str,
) -> Result<()> {
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
#[allow(dead_code)]
pub fn remove_channel_subscription(
    db: &Database,
    publisher_onion: &str,
    channel_type: &str,
) -> Result<()> {
    db.connection()
        .execute(
            "DELETE FROM channel_subscriptions WHERE publisher_onion = ?1 AND channel_type = ?2",
            params![publisher_onion, channel_type],
        )
        .map_err(|e| ChattorError::Database(format!("Failed to remove subscription: {}", e)))?;
    Ok(())
}

/// Get all channel subscriptions (subscriber side)
pub fn get_channel_subscriptions(db: &Database) -> Result<Vec<ChannelSubscription>> {
    let conn = db.connection();
    let mut stmt = conn
        .prepare(
            "SELECT id, publisher_onion, channel_type, subscribed_at, last_sync_at
         FROM channel_subscriptions ORDER BY subscribed_at ASC",
        )
        .map_err(|e| {
            ChattorError::Database(format!("Failed to prepare subscriptions query: {}", e))
        })?;

    let subs = stmt
        .query_map([], |row| {
            Ok(ChannelSubscription {
                id: row.get(0)?,
                publisher_onion: row.get(1)?,
                channel_type: row.get(2)?,
                subscribed_at: row.get(3)?,
                last_sync_at: row.get(4)?,
            })
        })
        .map_err(|e| ChattorError::Database(format!("Failed to query subscriptions: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect subscriptions: {}", e)))?;
    Ok(subs)
}

/// Update the last sync timestamp for a subscription
pub fn update_subscription_sync_time(
    db: &Database,
    publisher_onion: &str,
    channel_type: &str,
    sync_at: i64,
) -> Result<()> {
    db.connection().execute(
        "UPDATE channel_subscriptions SET last_sync_at = ?1 WHERE publisher_onion = ?2 AND channel_type = ?3",
        params![sync_at, publisher_onion, channel_type],
    ).map_err(|e| ChattorError::Database(format!("Failed to update sync time: {}", e)))?;
    Ok(())
}

/// Store a read receipt for a post (publisher side)
pub fn store_channel_post_receipt(
    db: &Database,
    post_id: &str,
    reader_onion: &str,
    read_at: i64,
) -> Result<()> {
    db.connection()
        .execute(
            "INSERT OR IGNORE INTO channel_post_receipts (post_id, reader_onion, read_at)
         VALUES (?1, ?2, ?3)",
            params![post_id, reader_onion, read_at],
        )
        .map_err(|e| ChattorError::Database(format!("Failed to store post receipt: {}", e)))?;
    Ok(())
}

/// Get read count for a post (publisher side)
#[allow(dead_code)]
pub fn get_channel_post_read_count(db: &Database, post_id: &str) -> Result<i64> {
    let count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM channel_post_receipts WHERE post_id = ?1",
            params![post_id],
            |row| row.get(0),
        )
        .map_err(|e| ChattorError::Database(format!("Failed to count post receipts: {}", e)))?;
    Ok(count)
}

/// Get read counts for multiple posts in a single query (publisher side).
/// Returns a map of post_id -> count. Posts with zero reads are omitted.
pub fn get_channel_post_read_counts_batch(
    db: &Database,
    post_ids: &[&str],
) -> Result<HashMap<String, i64>> {
    if post_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders: Vec<String> = (1..=post_ids.len()).map(|i| format!("?{}", i)).collect();
    let sql = format!(
        "SELECT post_id, COUNT(*) FROM channel_post_receipts WHERE post_id IN ({}) GROUP BY post_id",
        placeholders.join(", ")
    );

    let conn = db.connection();
    let mut stmt = conn.prepare(&sql).map_err(|e| {
        ChattorError::Database(format!("Failed to prepare batch read counts: {}", e))
    })?;

    let params: Vec<&dyn rusqlite::types::ToSql> = post_ids
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| ChattorError::Database(format!("Failed to query batch read counts: {}", e)))?;

    let mut counts = HashMap::new();
    for row in rows {
        let (post_id, count) = row.map_err(|e| {
            ChattorError::Database(format!("Failed to read batch count row: {}", e))
        })?;
        counts.insert(post_id, count);
    }

    Ok(counts)
}

/// Get posts from a channel since a timestamp (for sync responses)
pub fn get_channel_posts_since(
    db: &Database,
    channel_id: i64,
    since: i64,
) -> Result<Vec<ChannelPost>> {
    let conn = db.connection();
    let mut stmt = conn
        .prepare(
            "SELECT id, channel_id, content, post_id, created_at, signature
         FROM channel_posts
         WHERE channel_id = ?1 AND created_at > ?2
         ORDER BY created_at ASC
         LIMIT 100",
        )
        .map_err(|e| {
            ChattorError::Database(format!("Failed to prepare posts since query: {}", e))
        })?;

    let posts = stmt
        .query_map(params![channel_id, since], |row| {
            Ok(ChannelPost {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                content: row.get(2)?,
                post_id: row.get(3)?,
                created_at: row.get(4)?,
                signature: row.get(5)?,
            })
        })
        .map_err(|e| ChattorError::Database(format!("Failed to query posts since: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect posts since: {}", e)))?;
    Ok(posts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_initialize_channels() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        initialize_channels(&db).unwrap();

        let count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM channels", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        // Calling again should be idempotent
        initialize_channels(&db).unwrap();
        let count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM channels", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

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
                &db,
                1,
                &format!("Post {}", i),
                &format!("post-{}", i),
                i as i64,
                "sig",
            )
            .unwrap();
        }

        enforce_channel_retention(&db, 1).unwrap();

        let posts = get_channel_posts(&db, 1, 200).unwrap();
        assert_eq!(posts.len(), 100);
        // Oldest remaining should be post 5 (0-4 deleted)
        assert_eq!(posts[99].post_id, "post-5");
    }

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

    #[test]
    fn test_get_channel_posts_since() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        initialize_channels(&db).unwrap();

        for i in 0..5 {
            store_channel_post(
                &db,
                1,
                &format!("Post {}", i),
                &format!("post-{}", i),
                (1000 + i) as i64,
                "sig",
            )
            .unwrap();
        }

        let posts = get_channel_posts_since(&db, 1, 1002).unwrap();
        assert_eq!(posts.len(), 2); // posts 3 and 4
        assert_eq!(posts[0].post_id, "post-3");
        assert_eq!(posts[1].post_id, "post-4");
    }

    #[test]
    fn test_batch_channel_post_read_counts() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        store_channel_post_receipt(&db, "post-1", "bob.onion", 1000).unwrap();
        store_channel_post_receipt(&db, "post-1", "carol.onion", 2000).unwrap();
        store_channel_post_receipt(&db, "post-2", "bob.onion", 3000).unwrap();
        // post-3 has no receipts

        let counts =
            get_channel_post_read_counts_batch(&db, &["post-1", "post-2", "post-3"]).unwrap();
        assert_eq!(counts.get("post-1"), Some(&2));
        assert_eq!(counts.get("post-2"), Some(&1));
        assert_eq!(counts.get("post-3"), None); // no receipts = not in map
    }

    #[test]
    fn test_batch_channel_post_read_counts_empty() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let counts = get_channel_post_read_counts_batch(&db, &[]).unwrap();
        assert!(counts.is_empty());
    }
}
