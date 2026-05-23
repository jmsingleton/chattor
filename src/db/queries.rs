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
    /// Display name or truncated onion address. Truncation is by character
    /// count (not byte index), so a user-set multi-byte display name can't
    /// panic the renderer.
    pub fn display(&self) -> String {
        if let Some(ref name) = self.display_name {
            name.clone()
        } else {
            crate::ui::text::truncate_with_ellipsis(&self.onion_address, 13)
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

/// Delete a friend by row id. Cascades the removal across every table
/// that references them — conversation, messages, queued sends, Signal
/// session, channel subscription. Stored PreKey private material keyed
/// by their onion is also dropped so a future re-add starts fresh.
///
/// This is not a transaction: if a later step fails the earlier ones
/// remain. That's acceptable for an interactive delete — partial cleanup
/// is preferable to no cleanup, and any leftover rows are inert.
pub fn delete_friend(db: &Database, friend_id: i64) -> Result<()> {
    let conn = db.connection();

    // Resolve the onion address up front so we can target all the
    // onion-keyed tables that don't reference friend_id directly.
    let onion: Option<String> = conn.query_row(
        "SELECT onion_address FROM friends WHERE id = ?1",
        params![friend_id],
        |row| row.get::<_, String>(0),
    ).ok();

    // Conversation + messages.
    let conv_ids: Vec<i64> = conn.prepare("SELECT id FROM conversations WHERE friend_id = ?1")
        .and_then(|mut stmt| {
            stmt.query_map(params![friend_id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()
        })
        .unwrap_or_default();
    for conv_id in conv_ids {
        conn.execute("DELETE FROM messages WHERE conversation_id = ?1", params![conv_id])
            .map_err(|e| ChattorError::Database(format!("Failed to delete messages: {}", e)))?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![conv_id])
            .map_err(|e| ChattorError::Database(format!("Failed to delete conversation: {}", e)))?;
    }

    // Friend row itself.
    conn.execute("DELETE FROM friends WHERE id = ?1", params![friend_id])
        .map_err(|e| ChattorError::Database(format!("Failed to delete friend: {}", e)))?;

    // Onion-keyed cleanup.
    if let Some(o) = onion {
        conn.execute("DELETE FROM signal_sessions WHERE remote_onion = ?1", params![o])
            .map_err(|e| ChattorError::Database(format!("Failed to delete signal session: {}", e)))?;
        conn.execute("DELETE FROM message_queue WHERE peer_onion = ?1", params![o])
            .map_err(|e| ChattorError::Database(format!("Failed to delete queued messages: {}", e)))?;
        conn.execute("DELETE FROM channel_subscriptions WHERE publisher_onion = ?1", params![o])
            .map_err(|e| ChattorError::Database(format!("Failed to delete channel subscription: {}", e)))?;
        conn.execute("DELETE FROM channel_subscribers WHERE subscriber_onion = ?1", params![o])
            .map_err(|e| ChattorError::Database(format!("Failed to delete channel subscriber: {}", e)))?;
        // Cached posts authored by this peer — keyed on publisher_onion
        // since the v10 schema. Leaving them behind would let a removed
        // friend keep occupying retention storage.
        conn.execute("DELETE FROM channel_posts WHERE publisher_onion = ?1", params![o])
            .map_err(|e| ChattorError::Database(format!("Failed to delete cached channel posts: {}", e)))?;
        // Any pending friend_requests row from this peer that we never
        // accepted is also stale.
        conn.execute("DELETE FROM friend_requests WHERE from_onion = ?1", params![o])
            .map_err(|e| ChattorError::Database(format!("Failed to delete pending friend requests: {}", e)))?;
        // Read receipts they sent us for our own channel posts — drop them
        // too, so the friend's onion stops appearing in our seen-by counts
        // after the delete.
        conn.execute("DELETE FROM channel_post_receipts WHERE reader_onion = ?1", params![o])
            .map_err(|e| ChattorError::Database(format!("Failed to delete channel post receipts: {}", e)))?;
        conn.execute(
            "DELETE FROM app_settings WHERE key LIKE ?1 OR key LIKE ?2",
            params![format!("prekey_%:{}", o), format!("signal_identity_secret:{}", o)],
        ).map_err(|e| ChattorError::Database(format!("Failed to delete prekey material: {}", e)))?;
    }

    Ok(())
}

/// Insert an onion into the blocked_onions list. Returns Ok even if the
/// address is already blocked (uses INSERT OR REPLACE).
pub fn block_onion(db: &Database, onion_address: &str, reason: Option<&str>) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    db.connection().execute(
        "INSERT OR REPLACE INTO blocked_onions (onion_address, blocked_at, reason)
         VALUES (?1, ?2, ?3)",
        params![onion_address, now, reason],
    ).map_err(|e| ChattorError::Database(format!("Failed to block onion: {}", e)))?;
    Ok(())
}

/// True if `onion_address` appears in the blocked_onions table.
pub fn is_blocked(db: &Database, onion_address: &str) -> Result<bool> {
    let count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM blocked_onions WHERE onion_address = ?1",
        params![onion_address],
        |row| row.get(0),
    ).map_err(|e| ChattorError::Database(format!("Failed to check block status: {}", e)))?;
    Ok(count > 0)
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

/// A channel post for display. A post belongs to the `(publisher_onion,
/// channel_type)` tuple — local posts have `publisher_onion == own_onion`,
/// foreign posts have the remote peer's address.
#[derive(Debug, Clone)]
pub struct ChannelPost {
    #[allow(dead_code)]
    pub id: i64,
    #[allow(dead_code)] // surfaced in Phase B (sender-on-post UI)
    pub publisher_onion: String,
    #[allow(dead_code)]
    pub channel_type: String,
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

/// Store a channel post (dedup via post_id UNIQUE).
/// `channel_type` is "public" or "friends_only".
pub fn store_channel_post(
    db: &Database,
    publisher_onion: &str,
    channel_type: &str,
    content: &str,
    post_id: &str,
    created_at: i64,
    signature: &str,
) -> Result<()> {
    db.connection().execute(
        "INSERT OR IGNORE INTO channel_posts
            (publisher_onion, channel_type, content, post_id, created_at, signature)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![publisher_onion, channel_type, content, post_id, created_at, signature],
    ).map_err(|e| ChattorError::Database(format!("Failed to store channel post: {}", e)))?;
    Ok(())
}

/// Get posts for a channel `(publisher_onion, channel_type)`, newest first.
pub fn get_channel_posts(
    db: &Database,
    publisher_onion: &str,
    channel_type: &str,
    limit: usize,
) -> Result<Vec<ChannelPost>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, publisher_onion, channel_type, content, post_id, created_at, signature
         FROM channel_posts
         WHERE publisher_onion = ?1 AND channel_type = ?2
         ORDER BY created_at DESC, id DESC
         LIMIT ?3"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare channel posts query: {}", e)))?;

    let posts = stmt.query_map(params![publisher_onion, channel_type, limit as i64], |row| {
        Ok(ChannelPost {
            id: row.get(0)?,
            publisher_onion: row.get(1)?,
            channel_type: row.get(2)?,
            content: row.get(3)?,
            post_id: row.get(4)?,
            created_at: row.get(5)?,
            signature: row.get(6)?,
        })
    }).map_err(|e| ChattorError::Database(format!("Failed to query channel posts: {}", e)))?
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| ChattorError::Database(format!("Failed to collect channel posts: {}", e)))?;

    Ok(posts)
}

/// Delete oldest posts if count exceeds 100, per (publisher_onion, channel_type).
/// This keeps both our own feeds and each subscribed publisher's feed bounded.
pub fn enforce_channel_retention(
    db: &Database,
    publisher_onion: &str,
    channel_type: &str,
) -> Result<i64> {
    let deleted = db.connection().execute(
        "DELETE FROM channel_posts
         WHERE publisher_onion = ?1 AND channel_type = ?2
           AND id NOT IN (
             SELECT id FROM channel_posts
             WHERE publisher_onion = ?1 AND channel_type = ?2
             ORDER BY created_at DESC, id DESC LIMIT 100
         )",
        params![publisher_onion, channel_type],
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
pub fn get_channel_post_read_count(db: &Database, post_id: &str) -> Result<i64> {
    let count: i64 = db.connection().query_row(
        "SELECT COUNT(*) FROM channel_post_receipts WHERE post_id = ?1",
        params![post_id],
        |row| row.get(0),
    ).map_err(|e| ChattorError::Database(format!("Failed to count post receipts: {}", e)))?;
    Ok(count)
}

/// Get posts from a channel since a timestamp (for sync responses).
/// `publisher_onion` is the channel owner — typically the local user's onion
/// when responding to a peer's sync request.
pub fn get_channel_posts_since(
    db: &Database,
    publisher_onion: &str,
    channel_type: &str,
    since: i64,
) -> Result<Vec<ChannelPost>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, publisher_onion, channel_type, content, post_id, created_at, signature
         FROM channel_posts
         WHERE publisher_onion = ?1 AND channel_type = ?2 AND created_at > ?3
         ORDER BY created_at ASC
         LIMIT 100"
    ).map_err(|e| ChattorError::Database(format!("Failed to prepare posts since query: {}", e)))?;

    let posts = stmt.query_map(params![publisher_onion, channel_type, since], |row| {
        Ok(ChannelPost {
            id: row.get(0)?,
            publisher_onion: row.get(1)?,
            channel_type: row.get(2)?,
            content: row.get(3)?,
            post_id: row.get(4)?,
            created_at: row.get(5)?,
            signature: row.get(6)?,
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

        let me = "me.onion";
        store_channel_post(&db, me, "public", "Hello world!", "post-1", 1000, "sig1").unwrap();
        store_channel_post(&db, me, "public", "Second post", "post-2", 2000, "sig2").unwrap();

        let posts = get_channel_posts(&db, me, "public", 50).unwrap();
        assert_eq!(posts.len(), 2);
        // Newest first
        assert_eq!(posts[0].content, "Second post");
        assert_eq!(posts[1].content, "Hello world!");
    }

    #[test]
    fn test_channel_posts_isolated_per_publisher() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        initialize_channels(&db).unwrap();

        store_channel_post(&db, "alice.onion", "public", "from alice", "p-a", 1000, "siga").unwrap();
        store_channel_post(&db, "bob.onion", "public", "from bob", "p-b", 1000, "sigb").unwrap();

        let alice_feed = get_channel_posts(&db, "alice.onion", "public", 50).unwrap();
        let bob_feed = get_channel_posts(&db, "bob.onion", "public", 50).unwrap();

        assert_eq!(alice_feed.len(), 1);
        assert_eq!(alice_feed[0].content, "from alice");
        assert_eq!(bob_feed.len(), 1);
        assert_eq!(bob_feed[0].content, "from bob");
    }

    #[test]
    fn test_channel_post_dedup() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        initialize_channels(&db).unwrap();

        store_channel_post(&db, "me.onion", "public", "Hello!", "post-1", 1000, "sig1").unwrap();
        store_channel_post(&db, "me.onion", "public", "Hello!", "post-1", 1000, "sig1").unwrap(); // dupe

        let posts = get_channel_posts(&db, "me.onion", "public", 50).unwrap();
        assert_eq!(posts.len(), 1);
    }

    #[test]
    fn test_channel_retention_enforced() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        initialize_channels(&db).unwrap();

        let pub_o = "me.onion";
        for i in 0..105 {
            store_channel_post(
                &db, pub_o, "public", &format!("Post {}", i),
                &format!("post-{}", i), i as i64, "sig"
            ).unwrap();
        }

        enforce_channel_retention(&db, pub_o, "public").unwrap();

        let posts = get_channel_posts(&db, pub_o, "public", 200).unwrap();
        assert_eq!(posts.len(), 100);
        // Oldest remaining should be post 5 (0-4 deleted)
        assert_eq!(posts[99].post_id, "post-5");
    }

    #[test]
    fn test_channel_retention_per_publisher() {
        // Retention runs per-publisher: filling alice's feed doesn't evict bob's.
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        initialize_channels(&db).unwrap();

        for i in 0..150 {
            store_channel_post(
                &db, "alice.onion", "public", &format!("a{}", i),
                &format!("alice-{}", i), i as i64, "sig"
            ).unwrap();
        }
        store_channel_post(&db, "bob.onion", "public", "only", "bob-1", 1, "sig").unwrap();

        enforce_channel_retention(&db, "alice.onion", "public").unwrap();

        let alice = get_channel_posts(&db, "alice.onion", "public", 200).unwrap();
        let bob = get_channel_posts(&db, "bob.onion", "public", 200).unwrap();
        assert_eq!(alice.len(), 100);
        assert_eq!(bob.len(), 1, "bob's feed must not be touched by alice's retention");
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

        let me = "me.onion";
        for i in 0..5 {
            store_channel_post(
                &db, me, "public", &format!("Post {}", i),
                &format!("post-{}", i), (1000 + i) as i64, "sig"
            ).unwrap();
        }

        let posts = get_channel_posts_since(&db, me, "public", 1002).unwrap();
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
        assert_eq!(entry.display(), "abcdefghijkl…");

        let entry2 = FriendEntry {
            friend_id: 2,
            onion_address: "test.onion".to_string(),
            display_name: Some("Alice".to_string()),
            conversation_id: None,
            unread_count: 0,
        };
        assert_eq!(entry2.display(), "Alice");
    }

    // === delete_friend / block / is_blocked tests =========================

    /// Insert a friend with the given onion and return its row id.
    fn insert_friend(db: &Database, onion: &str) -> i64 {
        let conn = db.connection();
        conn.execute(
            "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES (?1, ?2, 0, 'active')",
            params![onion, &onion[..onion.len().min(10)]],
        ).unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn test_delete_friend_cascades_across_tables() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        initialize_channels(&db).unwrap();

        let alice_onion = "alice.onion".to_string();
        let alice_id = insert_friend(&db, &alice_onion);
        let conv_id = get_or_create_conversation(&db, alice_id).unwrap();

        let conn = db.connection();
        // Populate every table delete_friend should clean.
        conn.execute(
            "INSERT INTO messages (message_id, conversation_id, sender_onion, content, timestamp, status)
             VALUES ('m1', ?1, ?2, 'hi', 1, 'sent')",
            params![conv_id, alice_onion],
        ).unwrap();
        conn.execute(
            "INSERT INTO signal_sessions (remote_onion, session_state, updated_at) VALUES (?1, X'00', 1)",
            params![alice_onion],
        ).unwrap();
        conn.execute(
            "INSERT INTO message_queue (peer_onion, message_json, priority, retry_count, next_retry_at, created_at)
             VALUES (?1, '{}', 'normal', 0, 1, 1)",
            params![alice_onion],
        ).unwrap();
        add_channel_subscription(&db, &alice_onion, "public").unwrap();
        add_channel_subscriber(&db, &alice_onion, "public").unwrap();
        store_channel_post(&db, &alice_onion, "public", "hello", "p1", 1, "sig").unwrap();
        conn.execute(
            "INSERT INTO friend_requests (from_onion, friend_code, received_at, status)
             VALUES (?1, 'fc', 1, 'pending')",
            params![alice_onion],
        ).unwrap();
        store_channel_post_receipt(&db, "our-post-1", &alice_onion, 1).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, 'x')",
            params![format!("prekey_identity:{}", alice_onion)],
        ).unwrap();

        // And one unrelated friend whose rows must survive.
        let bob_onion = "bob.onion".to_string();
        let bob_id = insert_friend(&db, &bob_onion);
        let bob_conv = get_or_create_conversation(&db, bob_id).unwrap();
        add_channel_subscription(&db, &bob_onion, "public").unwrap();
        store_channel_post(&db, &bob_onion, "public", "bob's post", "p-bob", 1, "sig").unwrap();

        delete_friend(&db, alice_id).unwrap();

        // Alice is gone everywhere.
        let friend_count: i64 = conn.query_row("SELECT COUNT(*) FROM friends WHERE id = ?1", params![alice_id], |r| r.get(0)).unwrap();
        let conv_count: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE id = ?1", params![conv_id], |r| r.get(0)).unwrap();
        let msg_count: i64 = conn.query_row("SELECT COUNT(*) FROM messages WHERE conversation_id = ?1", params![conv_id], |r| r.get(0)).unwrap();
        let sess_count: i64 = conn.query_row("SELECT COUNT(*) FROM signal_sessions WHERE remote_onion = ?1", params![alice_onion], |r| r.get(0)).unwrap();
        let queue_count: i64 = conn.query_row("SELECT COUNT(*) FROM message_queue WHERE peer_onion = ?1", params![alice_onion], |r| r.get(0)).unwrap();
        let sub_count: i64 = conn.query_row("SELECT COUNT(*) FROM channel_subscriptions WHERE publisher_onion = ?1", params![alice_onion], |r| r.get(0)).unwrap();
        let suber_count: i64 = conn.query_row("SELECT COUNT(*) FROM channel_subscribers WHERE subscriber_onion = ?1", params![alice_onion], |r| r.get(0)).unwrap();
        let post_count: i64 = conn.query_row("SELECT COUNT(*) FROM channel_posts WHERE publisher_onion = ?1", params![alice_onion], |r| r.get(0)).unwrap();
        let req_count: i64 = conn.query_row("SELECT COUNT(*) FROM friend_requests WHERE from_onion = ?1", params![alice_onion], |r| r.get(0)).unwrap();
        let receipt_count: i64 = conn.query_row("SELECT COUNT(*) FROM channel_post_receipts WHERE reader_onion = ?1", params![alice_onion], |r| r.get(0)).unwrap();
        let prekey_count: i64 = conn.query_row("SELECT COUNT(*) FROM app_settings WHERE key LIKE ?1", params![format!("prekey_%:{}", alice_onion)], |r| r.get(0)).unwrap();

        assert_eq!(friend_count, 0);
        assert_eq!(conv_count, 0);
        assert_eq!(msg_count, 0);
        assert_eq!(sess_count, 0);
        assert_eq!(queue_count, 0);
        assert_eq!(sub_count, 0);
        assert_eq!(suber_count, 0);
        assert_eq!(post_count, 0);
        assert_eq!(req_count, 0);
        assert_eq!(receipt_count, 0);
        assert_eq!(prekey_count, 0);

        // Bob is intact.
        let bob_friend: i64 = conn.query_row("SELECT COUNT(*) FROM friends WHERE id = ?1", params![bob_id], |r| r.get(0)).unwrap();
        let bob_conv_count: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE id = ?1", params![bob_conv], |r| r.get(0)).unwrap();
        let bob_sub: i64 = conn.query_row("SELECT COUNT(*) FROM channel_subscriptions WHERE publisher_onion = ?1", params![bob_onion], |r| r.get(0)).unwrap();
        let bob_posts: i64 = conn.query_row("SELECT COUNT(*) FROM channel_posts WHERE publisher_onion = ?1", params![bob_onion], |r| r.get(0)).unwrap();
        assert_eq!(bob_friend, 1);
        assert_eq!(bob_conv_count, 1);
        assert_eq!(bob_sub, 1);
        assert_eq!(bob_posts, 1);
    }

    #[test]
    fn test_block_onion_and_is_blocked() {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();

        assert!(!is_blocked(&db, "eve.onion").unwrap());
        block_onion(&db, "eve.onion", Some("test")).unwrap();
        assert!(is_blocked(&db, "eve.onion").unwrap());
        // INSERT OR REPLACE: blocking twice doesn't error.
        block_onion(&db, "eve.onion", None).unwrap();
        assert!(is_blocked(&db, "eve.onion").unwrap());
        // Other onions are unaffected.
        assert!(!is_blocked(&db, "alice.onion").unwrap());
    }
}
