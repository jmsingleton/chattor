use crate::db::Database;
use crate::error::{ChattorError, Result};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
