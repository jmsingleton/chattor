use crate::db::Database;
use crate::error::{Result, ChattorError};
use rusqlite::params;

/// A friend entry for the sidebar
#[derive(Debug, Clone)]
pub struct FriendEntry {
    pub friend_id: i64,
    pub onion_address: String,
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub conversation_id: Option<i64>,
    pub unread_count: i64,
}

impl FriendEntry {
    /// Display name or truncated onion address
    pub fn display(&self) -> String {
        if let Some(ref name) = self.display_name {
            name.clone()
        } else {
            crate::ui::input::truncate_display_dots(&self.onion_address, 12)
        }
    }
}

/// A pending friend request entry
#[derive(Debug, Clone)]
pub struct PendingFriendRequest {
    pub id: i64,
    pub from_onion: String,
    pub friend_code: String,
    pub received_at: i64,
}

/// A message for the conversation view
#[derive(Debug, Clone)]
pub struct ChatMessage {
    #[allow(dead_code)]
    pub id: i64,
    #[allow(dead_code)]
    pub message_id: String,
    pub sender_onion: String,
    pub content: String,
    pub timestamp: i64,
    pub status: String,
    pub ephemeral_ttl: Option<i64>,
}

/// Get active friends with unread counts
pub fn get_friends_with_unread(db: &Database) -> Result<Vec<FriendEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT f.id, f.onion_address, f.display_name,
                c.id as conversation_id,
                (SELECT COUNT(*) FROM messages m
                 WHERE m.conversation_id = c.id
                 AND m.status = 'received'
                 AND m.timestamp > COALESCE(c.last_read_at, 0)) as unread
         FROM friends f
         LEFT JOIN conversations c ON c.friend_id = f.id
         WHERE f.status = 'active'
         ORDER BY f.display_name, f.onion_address"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare friends query: {}", e)))?;

    let entries = stmt.query_map([], |row| {
        Ok(FriendEntry {
            friend_id: row.get(0)?,
            onion_address: row.get(1)?,
            display_name: row.get(2)?,
            conversation_id: row.get(3)?,
            unread_count: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
        })
    }).map_err(|e| ChattorError::Database(format!("Failed to query friends: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect friends: {}", e)))?;

    Ok(entries)
}

/// Get or create a conversation for a friend
pub fn get_or_create_conversation(db: &Database, friend_id: i64) -> Result<i64> {
    let conn = db.connection();

    // Try to find existing conversation
    let existing: rusqlite::Result<i64> = conn.query_row(
        "SELECT id FROM conversations WHERE friend_id = ?1 LIMIT 1",
        params![friend_id],
        |row| row.get(0),
    );

    match existing {
        Ok(id) => Ok(id),
        Err(_) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO conversations (friend_id, is_ephemeral, created_at) VALUES (?1, 0, ?2)",
                params![friend_id, now],
            ).map_err(|e| ChattorError::Database(format!("Failed to create conversation: {}", e)))?;

            Ok(conn.last_insert_rowid())
        }
    }
}

/// Load messages for a conversation (most recent first, then reversed for display)
pub fn get_messages(db: &Database, conversation_id: i64, limit: usize, offset: usize) -> Result<Vec<ChatMessage>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, message_id, sender_onion, content, timestamp, status, ephemeral_ttl
         FROM messages
         WHERE conversation_id = ?1
         ORDER BY timestamp DESC, id DESC
         LIMIT ?2 OFFSET ?3"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare messages query: {}", e)))?;

    let mut messages: Vec<ChatMessage> = stmt.query_map(
        params![conversation_id, limit as i64, offset as i64],
        |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                message_id: row.get(1)?,
                sender_onion: row.get(2)?,
                content: row.get(3)?,
                timestamp: row.get(4)?,
                status: row.get(5)?,
                ephemeral_ttl: row.get(6)?,
            })
        },
    ).map_err(|e| ChattorError::Database(format!("Failed to query messages: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect messages: {}", e)))?;

    // Reverse so oldest is first (for display top-to-bottom)
    messages.reverse();
    Ok(messages)
}

/// Store an outgoing message
#[allow(dead_code)]
pub fn store_outgoing_message(
    db: &Database,
    conversation_id: i64,
    sender_onion: &str,
    content: &str,
    message_id: &str,
) -> Result<()> {
    store_outgoing_message_with_ttl(db, conversation_id, sender_onion, content, message_id, None)
}

/// Store an outgoing message with optional ephemeral TTL
pub fn store_outgoing_message_with_ttl(
    db: &Database,
    conversation_id: i64,
    sender_onion: &str,
    content: &str,
    message_id: &str,
    ephemeral_ttl: Option<i64>,
) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "INSERT INTO messages (message_id, conversation_id, sender_onion, content, timestamp, status, ephemeral_ttl)
         VALUES (?1, ?2, ?3, ?4, ?5, 'sent', ?6)",
        params![message_id, conversation_id, sender_onion, content, now, ephemeral_ttl],
    ).map_err(|e| ChattorError::Database(format!("Failed to store outgoing message: {}", e)))?;

    Ok(())
}

/// Store an incoming message
#[allow(dead_code)]
pub fn store_incoming_message(
    db: &Database,
    conversation_id: i64,
    sender_onion: &str,
    content: &str,
    message_id: &str,
) -> Result<()> {
    store_incoming_message_with_ttl(db, conversation_id, sender_onion, content, message_id, None)
}

/// Store an incoming message with optional ephemeral TTL
pub fn store_incoming_message_with_ttl(
    db: &Database,
    conversation_id: i64,
    sender_onion: &str,
    content: &str,
    message_id: &str,
    ephemeral_ttl: Option<i64>,
) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "INSERT OR IGNORE INTO messages (message_id, conversation_id, sender_onion, content, timestamp, status, ephemeral_ttl)
         VALUES (?1, ?2, ?3, ?4, ?5, 'received', ?6)",
        params![message_id, conversation_id, sender_onion, content, now, ephemeral_ttl],
    ).map_err(|e| ChattorError::Database(format!("Failed to store incoming message: {}", e)))?;

    Ok(())
}

/// Mark a conversation as read (update last_read_at to now)
pub fn mark_conversation_read(db: &Database, conversation_id: i64) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "UPDATE conversations SET last_read_at = ?1 WHERE id = ?2",
        params![now, conversation_id],
    ).map_err(|e| ChattorError::Database(format!("Failed to mark conversation read: {}", e)))?;

    Ok(())
}

/// Update a message's delivery status
pub fn update_message_status(db: &Database, message_id: &str, status: &str) -> Result<()> {
    db.connection().execute(
        "UPDATE messages SET status = ?1 WHERE message_id = ?2",
        params![status, message_id],
    ).map_err(|e| ChattorError::Database(format!("Failed to update message status: {}", e)))?;

    Ok(())
}

/// Find friend by onion address
pub fn find_friend_by_onion(db: &Database, onion_address: &str) -> Result<Option<i64>> {
    let result: rusqlite::Result<i64> = db.connection().query_row(
        "SELECT id FROM friends WHERE onion_address = ?1 AND status = 'active'",
        params![onion_address],
        |row| row.get(0),
    );

    match result {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(ChattorError::Database(format!("Failed to find friend: {}", e))),
    }
}

/// Count pending friend requests
pub fn get_pending_request_count(db: &Database) -> Result<i64> {
    let count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM friend_requests WHERE status = 'pending'",
        [],
        |row| row.get(0),
    ).map_err(|e| ChattorError::Database(format!("Failed to count pending requests: {}", e)))?;

    Ok(count)
}

/// Get all pending friend requests
pub fn get_pending_friend_requests(db: &Database) -> Result<Vec<PendingFriendRequest>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, from_onion, COALESCE(friend_code, ''), received_at
         FROM friend_requests
         WHERE status = 'pending'
         ORDER BY received_at ASC"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare friend requests query: {}", e)))?;

    let entries = stmt.query_map([], |row| {
        Ok(PendingFriendRequest {
            id: row.get(0)?,
            from_onion: row.get(1)?,
            friend_code: row.get(2)?,
            received_at: row.get(3)?,
        })
    }).map_err(|e| ChattorError::Database(format!("Failed to query friend requests: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect friend requests: {}", e)))?;

    Ok(entries)
}

/// Get message IDs from a peer that need read receipts (status = 'received')
pub fn get_unreceipted_message_ids(
    db: &Database,
    conversation_id: i64,
    own_onion: &str,
) -> Result<Vec<(String, String)>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT message_id, sender_onion FROM messages
         WHERE conversation_id = ?1
         AND status = 'received'
         AND sender_onion != ?2"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare unreceipted query: {}", e)))?;

    let entries = stmt.query_map(
        params![conversation_id, own_onion],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).map_err(|e| ChattorError::Database(format!("Failed to query unreceipted: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect unreceipted: {}", e)))?;

    Ok(entries)
}

/// Get the ephemeral TTL for a conversation (None = not ephemeral)
pub fn get_conversation_ephemeral_ttl(db: &Database, conversation_id: i64) -> Result<Option<i64>> {
    let result: rusqlite::Result<Option<i64>> = db.connection().query_row(
        "SELECT ephemeral_ttl FROM conversations WHERE id = ?1",
        params![conversation_id],
        |row| row.get(0),
    );

    match result {
        Ok(ttl) => Ok(ttl),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(ChattorError::Database(format!("Failed to get ephemeral TTL: {}", e))),
    }
}

/// Set the ephemeral TTL for a conversation (None = disable)
pub fn set_conversation_ephemeral_ttl(db: &Database, conversation_id: i64, ttl: Option<i64>) -> Result<()> {
    db.connection().execute(
        "UPDATE conversations SET ephemeral_ttl = ?1, is_ephemeral = ?2 WHERE id = ?3",
        params![ttl, ttl.is_some() as i32, conversation_id],
    ).map_err(|e| ChattorError::Database(format!("Failed to set ephemeral TTL: {}", e)))?;

    Ok(())
}

/// Set expires_at for unread ephemeral messages when conversation is read
pub fn activate_ephemeral_timers(db: &Database, conversation_id: i64) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.connection().execute(
        "UPDATE messages SET expires_at = ?1 + ephemeral_ttl
         WHERE conversation_id = ?2
         AND ephemeral_ttl IS NOT NULL
         AND expires_at IS NULL",
        params![now, conversation_id],
    ).map_err(|e| ChattorError::Database(format!("Failed to activate timers: {}", e)))?;

    Ok(())
}

// === Phase 3: Broadcast channel queries ===

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
    ).map_err(|e| ChattorError::Database(format!("Failed to create public channel: {}", e)))?;

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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn get_channel_post_read_count(db: &Database, post_id: &str) -> Result<i64> {
    let count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM channel_post_receipts WHERE post_id = ?1",
        params![post_id],
        |row| row.get(0),
    ).map_err(|e| ChattorError::Database(format!("Failed to count post receipts: {}", e)))?;
    Ok(count)
}

/// Get read counts for multiple posts in a single query (publisher side).
/// Returns a map of post_id -> count. Posts with zero reads are omitted.
pub fn get_channel_post_read_counts_batch(
    db: &Database,
    post_ids: &[&str],
) -> Result<std::collections::HashMap<String, i64>> {
    if post_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders: Vec<String> = (1..=post_ids.len()).map(|i| format!("?{}", i)).collect();
    let sql = format!(
        "SELECT post_id, COUNT(*) FROM channel_post_receipts WHERE post_id IN ({}) GROUP BY post_id",
        placeholders.join(", ")
    );

    let conn = db.connection();
    let mut stmt = conn.prepare(&sql)
        .map_err(|e| ChattorError::Database(format!("Failed to prepare batch read counts: {}", e)))?;

    let params: Vec<&dyn rusqlite::types::ToSql> = post_ids.iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    }).map_err(|e| ChattorError::Database(format!("Failed to query batch read counts: {}", e)))?;

    let mut counts = std::collections::HashMap::new();
    for row in rows {
        let (post_id, count) = row
            .map_err(|e| ChattorError::Database(format!("Failed to read batch count row: {}", e)))?;
        counts.insert(post_id, count);
    }

    Ok(counts)
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

/// Delete expired ephemeral messages
pub fn cleanup_expired_messages(db: &Database) -> Result<i64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let deleted = db.connection().execute(
        "DELETE FROM messages WHERE expires_at IS NOT NULL AND expires_at < ?1",
        params![now],
    ).map_err(|e| ChattorError::Database(format!("Failed to cleanup expired: {}", e)))? as i64;

    Ok(deleted)
}

// === App settings queries ===

/// Get an application setting by key
pub fn get_app_setting(db: &Database, key: &str) -> Result<Option<String>> {
    let conn = db.connection();
    let result = conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    );
    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(ChattorError::Database(format!("Failed to get setting: {}", e))),
    }
}

/// Get display name for a friend by onion address
pub fn get_friend_display_name(db: &Database, onion: &str) -> Result<String> {
    let conn = db.connection();
    let result = conn.query_row(
        "SELECT COALESCE(display_name, onion_address) FROM friends WHERE onion_address = ?1",
        [onion],
        |row| row.get(0),
    );
    match result {
        Ok(name) => Ok(name),
        Err(_) => Ok(onion.to_string()),
    }
}

/// Set an application setting (insert or update)
pub fn set_app_setting(db: &Database, key: &str, value: &str) -> Result<()> {
    let conn = db.connection();
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (key, value),
    ).map_err(|e| ChattorError::Database(format!("Failed to set setting: {}", e)))?;
    Ok(())
}

/// Delete stale PreKey private material older than max_age_secs.
/// Returns the number of peers whose material was cleaned up.
pub fn cleanup_stale_prekey_material(db: &Database, max_age_secs: u64) -> Result<usize> {
    let conn = db.connection();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Find stale peers by checking prekey_created_at entries
    let mut stmt = conn
        .prepare("SELECT key, value FROM app_settings WHERE key LIKE 'prekey_created_at:%'")
        .map_err(|e| {
            ChattorError::Database(format!("Failed to query prekey timestamps: {}", e))
        })?;

    let stale_peers: Vec<String> = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let ts_str: String = row.get(1)?;
            Ok((key, ts_str))
        })
        .map_err(|e| {
            ChattorError::Database(format!("Failed to read prekey timestamps: {}", e))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(key, ts_str)| {
            let ts: u64 = ts_str.parse().ok()?;
            if now.saturating_sub(ts) > max_age_secs {
                // Extract onion from "prekey_created_at:<onion>"
                key.strip_prefix("prekey_created_at:")
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();

    let count = stale_peers.len();
    for peer in &stale_peers {
        conn.execute(
            "DELETE FROM app_settings WHERE key LIKE ?1",
            [&format!("prekey_%:{}", peer)],
        )
        .ok();
        conn.execute(
            "DELETE FROM app_settings WHERE key = ?1",
            [&format!("signal_identity_secret:{}", peer)],
        )
        .ok();
        conn.execute(
            "DELETE FROM app_settings WHERE key = ?1",
            [&format!("prekey_created_at:{}", peer)],
        )
        .ok();
        tracing::warn!(
            "Cleaned up stale PreKey material for {} (>7 days)",
            &peer[..8.min(peer.len())]
        );
    }

    Ok(count)
}

/// Store a peer's Ed25519 public key (TOFU binding).
pub fn store_friend_pubkey(db: &crate::db::Database, onion: &str, pubkey: &[u8]) -> crate::error::Result<()> {
    let conn = db.connection();
    conn.execute(
        "UPDATE friends SET ed25519_pubkey = ?1 WHERE onion_address = ?2",
        (pubkey, onion),
    ).map_err(|e| crate::error::ChattorError::Database(format!("Failed to store friend pubkey: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        // Add a test friend
        db.connection().execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
            [],
        ).unwrap();

        (db, temp)
    }

    #[test]
    fn test_get_friends_with_unread_empty() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends.len(), 0);
    }

    #[test]
    fn test_get_friends_with_unread() {
        let (db, _temp) = setup_test_db();
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].display_name, Some("Alice".to_string()));
        assert_eq!(friends[0].unread_count, 0);
    }

    #[test]
    fn test_get_or_create_conversation() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();
        assert!(conv_id > 0);

        // Should return same conversation
        let conv_id2 = get_or_create_conversation(&db, 1).unwrap();
        assert_eq!(conv_id, conv_id2);
    }

    #[test]
    fn test_store_and_get_messages() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_outgoing_message(&db, conv_id, "me.onion", "Hello!", "msg-1").unwrap();
        store_incoming_message(&db, conv_id, "alice.onion", "Hi!", "msg-2").unwrap();

        let messages = get_messages(&db, conv_id, 50, 0).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello!");
        assert_eq!(messages[1].content, "Hi!");
    }

    #[test]
    fn test_mark_conversation_read() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_incoming_message(&db, conv_id, "alice.onion", "Hey!", "msg-1").unwrap();

        // Before marking read, should have 1 unread
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends[0].unread_count, 1);

        // Mark read
        mark_conversation_read(&db, conv_id).unwrap();

        // After marking read, should have 0 unread
        let friends = get_friends_with_unread(&db).unwrap();
        assert_eq!(friends[0].unread_count, 0);
    }

    #[test]
    fn test_update_message_status() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_outgoing_message(&db, conv_id, "me.onion", "Hello!", "msg-1").unwrap();
        update_message_status(&db, "msg-1", "queued").unwrap();

        let messages = get_messages(&db, conv_id, 50, 0).unwrap();
        assert_eq!(messages[0].status, "queued");
    }

    #[test]
    fn test_find_friend_by_onion() {
        let (db, _temp) = setup_test_db();
        let found = find_friend_by_onion(&db, "alice.onion").unwrap();
        assert_eq!(found, Some(1));

        let not_found = find_friend_by_onion(&db, "unknown.onion").unwrap();
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_get_pending_request_count_empty() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        assert_eq!(get_pending_request_count(&db).unwrap(), 0);
    }

    #[test]
    fn test_get_pending_request_count() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        db.connection().execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status) VALUES ('a.onion', 'code-1', 1000, 'pending')",
            [],
        ).unwrap();
        db.connection().execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status) VALUES ('b.onion', 'code-2', 2000, 'pending')",
            [],
        ).unwrap();
        db.connection().execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status) VALUES ('c.onion', 'code-3', 3000, 'accepted')",
            [],
        ).unwrap();

        assert_eq!(get_pending_request_count(&db).unwrap(), 2);
    }

    #[test]
    fn test_get_pending_friend_requests() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        db.connection().execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status) VALUES ('a.onion', 'code-1', 1000, 'pending')",
            [],
        ).unwrap();
        db.connection().execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status) VALUES ('b.onion', 'code-2', 2000, 'pending')",
            [],
        ).unwrap();

        let requests = get_pending_friend_requests(&db).unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].from_onion, "a.onion");
        assert_eq!(requests[1].from_onion, "b.onion");
        assert_eq!(requests[0].friend_code, "code-1");
    }

    #[test]
    fn test_get_pending_friend_requests_excludes_accepted() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        db.connection().execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status) VALUES ('a.onion', 'code-1', 1000, 'accepted')",
            [],
        ).unwrap();

        let requests = get_pending_friend_requests(&db).unwrap();
        assert_eq!(requests.len(), 0);
    }

    #[test]
    fn test_get_unreceipted_message_ids() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        // Store incoming messages
        store_incoming_message(&db, conv_id, "alice.onion", "Hey!", "msg-1").unwrap();
        store_incoming_message(&db, conv_id, "alice.onion", "How are you?", "msg-2").unwrap();

        // Store an outgoing message (should not appear)
        store_outgoing_message(&db, conv_id, "me.onion", "Good!", "msg-3").unwrap();

        let unreceipted = get_unreceipted_message_ids(&db, conv_id, "me.onion").unwrap();
        assert_eq!(unreceipted.len(), 2);
        assert_eq!(unreceipted[0].0, "msg-1");
        assert_eq!(unreceipted[0].1, "alice.onion");

        // After marking one as read, it should disappear
        update_message_status(&db, "msg-1", "read").unwrap();
        let unreceipted = get_unreceipted_message_ids(&db, conv_id, "me.onion").unwrap();
        assert_eq!(unreceipted.len(), 1);
        assert_eq!(unreceipted[0].0, "msg-2");
    }

    #[test]
    fn test_ephemeral_ttl_set_and_get() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        // Initially no TTL
        assert_eq!(get_conversation_ephemeral_ttl(&db, conv_id).unwrap(), None);

        // Set TTL
        set_conversation_ephemeral_ttl(&db, conv_id, Some(300)).unwrap();
        assert_eq!(get_conversation_ephemeral_ttl(&db, conv_id).unwrap(), Some(300));

        // Disable
        set_conversation_ephemeral_ttl(&db, conv_id, None).unwrap();
        assert_eq!(get_conversation_ephemeral_ttl(&db, conv_id).unwrap(), None);
    }

    #[test]
    fn test_cleanup_expired_messages() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_outgoing_message(&db, conv_id, "me.onion", "will expire", "msg-exp").unwrap();
        store_outgoing_message(&db, conv_id, "me.onion", "will stay", "msg-stay").unwrap();

        // Set one message to expire in the past
        db.connection().execute(
            "UPDATE messages SET expires_at = 1 WHERE message_id = 'msg-exp'",
            [],
        ).unwrap();

        let deleted = cleanup_expired_messages(&db).unwrap();
        assert_eq!(deleted, 1);

        let messages = get_messages(&db, conv_id, 50, 0).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "will stay");
    }

    #[test]
    fn test_activate_ephemeral_timers() {
        let (db, _temp) = setup_test_db();
        let conv_id = get_or_create_conversation(&db, 1).unwrap();

        store_outgoing_message(&db, conv_id, "me.onion", "ephemeral msg", "msg-1").unwrap();

        // Set ephemeral_ttl on the message but no expires_at
        db.connection().execute(
            "UPDATE messages SET ephemeral_ttl = 300 WHERE message_id = 'msg-1'",
            [],
        ).unwrap();

        // Activate timers
        activate_ephemeral_timers(&db, conv_id).unwrap();

        // Check expires_at is now set
        let expires_at: Option<i64> = db.connection().query_row(
            "SELECT expires_at FROM messages WHERE message_id = 'msg-1'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(expires_at.is_some());
        assert!(expires_at.unwrap() > 0);
    }

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
            store_channel_post(&db, 1, &format!("Post {}", i), &format!("post-{}", i), (1000 + i) as i64, "sig").unwrap();
        }

        let posts = get_channel_posts_since(&db, 1, 1002).unwrap();
        assert_eq!(posts.len(), 2); // posts 3 and 4
        assert_eq!(posts[0].post_id, "post-3");
        assert_eq!(posts[1].post_id, "post-4");
    }

    #[test]
    fn test_get_set_app_setting() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        // No setting yet
        assert_eq!(get_app_setting(&db, "onion_address").unwrap(), None);

        // Set a value
        set_app_setting(&db, "onion_address", "abc123.onion").unwrap();
        assert_eq!(
            get_app_setting(&db, "onion_address").unwrap(),
            Some("abc123.onion".to_string())
        );

        // Update existing
        set_app_setting(&db, "onion_address", "xyz789.onion").unwrap();
        assert_eq!(
            get_app_setting(&db, "onion_address").unwrap(),
            Some("xyz789.onion".to_string())
        );
    }

    #[test]
    fn test_friend_entry_display() {
        let entry = FriendEntry {
            friend_id: 1,
            onion_address: "abcdefghijklmnopqrstuvwxyz.onion".to_string(),
            display_name: None,
            conversation_id: None,
            unread_count: 0,
        };
        assert_eq!(entry.display(), "abcdefghijkl...");

        let entry2 = FriendEntry {
            friend_id: 2,
            onion_address: "test.onion".to_string(),
            display_name: Some("Alice".to_string()),
            conversation_id: None,
            unread_count: 0,
        };
        assert_eq!(entry2.display(), "Alice");
    }

    #[test]
    fn test_batch_channel_post_read_counts() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        store_channel_post_receipt(&db, "post-1", "bob.onion", 1000).unwrap();
        store_channel_post_receipt(&db, "post-1", "carol.onion", 2000).unwrap();
        store_channel_post_receipt(&db, "post-2", "bob.onion", 3000).unwrap();
        // post-3 has no receipts

        let counts = get_channel_post_read_counts_batch(&db, &["post-1", "post-2", "post-3"]).unwrap();
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
