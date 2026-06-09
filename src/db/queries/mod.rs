use crate::db::Database;
use crate::error::{ChattorError, Result};
use rusqlite::params;

mod channels;
pub use channels::*;

mod messaging;
pub use messaging::*;

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

/// Get active friends with unread counts
pub fn get_friends_with_unread(db: &Database) -> Result<Vec<FriendEntry>> {
    let conn = db.connection();
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.onion_address, f.display_name,
                c.id as conversation_id,
                (SELECT COUNT(*) FROM messages m
                 WHERE m.conversation_id = c.id
                 AND m.status = 'received'
                 AND m.timestamp > COALESCE(c.last_read_at, 0)) as unread
         FROM friends f
         LEFT JOIN conversations c ON c.friend_id = f.id
         WHERE f.status = 'active'
         ORDER BY f.display_name, f.onion_address",
        )
        .map_err(|e| ChattorError::Database(format!("Failed to prepare friends query: {}", e)))?;

    let entries = stmt
        .query_map([], |row| {
            Ok(FriendEntry {
                friend_id: row.get(0)?,
                onion_address: row.get(1)?,
                display_name: row.get(2)?,
                conversation_id: row.get(3)?,
                unread_count: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
            })
        })
        .map_err(|e| ChattorError::Database(format!("Failed to query friends: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect friends: {}", e)))?;

    Ok(entries)
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
        Err(e) => Err(ChattorError::Database(format!(
            "Failed to find friend: {}",
            e
        ))),
    }
}

/// Count pending friend requests
pub fn get_pending_request_count(db: &Database) -> Result<i64> {
    let count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM friend_requests WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| ChattorError::Database(format!("Failed to count pending requests: {}", e)))?;

    Ok(count)
}

/// Get all pending friend requests
pub fn get_pending_friend_requests(db: &Database) -> Result<Vec<PendingFriendRequest>> {
    let conn = db.connection();
    let mut stmt = conn
        .prepare(
            "SELECT id, from_onion, COALESCE(friend_code, ''), received_at
         FROM friend_requests
         WHERE status = 'pending'
         ORDER BY received_at ASC",
        )
        .map_err(|e| {
            ChattorError::Database(format!("Failed to prepare friend requests query: {}", e))
        })?;

    let entries = stmt
        .query_map([], |row| {
            Ok(PendingFriendRequest {
                id: row.get(0)?,
                from_onion: row.get(1)?,
                friend_code: row.get(2)?,
                received_at: row.get(3)?,
            })
        })
        .map_err(|e| ChattorError::Database(format!("Failed to query friend requests: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ChattorError::Database(format!("Failed to collect friend requests: {}", e)))?;

    Ok(entries)
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
        Err(e) => Err(ChattorError::Database(format!(
            "Failed to get setting: {}",
            e
        ))),
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
    )
    .map_err(|e| ChattorError::Database(format!("Failed to set setting: {}", e)))?;
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
        .map_err(|e| ChattorError::Database(format!("Failed to query prekey timestamps: {}", e)))?;

    let stale_peers: Vec<String> = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let ts_str: String = row.get(1)?;
            Ok((key, ts_str))
        })
        .map_err(|e| ChattorError::Database(format!("Failed to read prekey timestamps: {}", e)))?
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
pub fn store_friend_pubkey(
    db: &crate::db::Database,
    onion: &str,
    pubkey: &[u8],
) -> crate::error::Result<()> {
    let conn = db.connection();
    conn.execute(
        "UPDATE friends SET ed25519_pubkey = ?1 WHERE onion_address = ?2",
        (pubkey, onion),
    )
    .map_err(|e| {
        crate::error::ChattorError::Database(format!("Failed to store friend pubkey: {}", e))
    })?;
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
        db.connection()
            .execute(
                "INSERT INTO friends (onion_address, display_name, added_at, status)
             VALUES ('alice.onion', 'Alice', 1000, 'active')",
                [],
            )
            .unwrap();

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

}
