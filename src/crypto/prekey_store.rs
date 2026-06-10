use crate::crypto::signal::PreKeyPrivateMaterial;
use crate::db::Database;
use crate::error::{ChattorError, Result};
use base64::Engine as _;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Typed accessor for X3DH establishment material persisted in `app_settings`.
/// Owns the key-string format so it lives in exactly one place.
pub struct PreKeyStore<'a> {
    db: &'a Database,
}

impl<'a> PreKeyStore<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.db
            .connection()
            .execute(
                "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
                (key, value),
            )
            .map_err(|e| ChattorError::Database(format!("PreKeyStore set failed: {}", e)))?;
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.db.connection().query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ChattorError::Database(format!("PreKeyStore get failed: {}", e))),
        }
    }

    fn decode32(b64: &str, what: &str) -> Result<[u8; 32]> {
        B64.decode(b64)
            .map_err(|e| ChattorError::Crypto(format!("Failed to decode {}: {}", what, e)))?
            .try_into()
            .map_err(|_| ChattorError::Crypto(format!("{} has wrong length", what)))
    }

    /// Persist private material + signal identity secret + creation timestamp for `peer`.
    pub fn store(
        &self,
        peer: &str,
        material: &PreKeyPrivateMaterial,
        signal_identity_secret: &[u8; 32],
        created_at: i64,
    ) -> Result<()> {
        self.set(
            &format!("prekey_identity:{}", peer),
            &B64.encode(material.identity_secret),
        )?;
        self.set(
            &format!("prekey_spk:{}", peer),
            &B64.encode(material.signed_prekey_secret),
        )?;
        if let Some(opk) = material.prekey_secret {
            self.set(&format!("prekey_opk:{}", peer), &B64.encode(opk))?;
        }
        self.set(
            &format!("signal_identity_secret:{}", peer),
            &B64.encode(signal_identity_secret),
        )?;
        self.set(
            &format!("prekey_created_at:{}", peer),
            &format!("{}", created_at),
        )?;
        Ok(())
    }

    /// Load the stored `PreKeyPrivateMaterial` for `peer` (None if the identity row is absent).
    pub fn load(&self, peer: &str) -> Result<Option<PreKeyPrivateMaterial>> {
        let identity_b64 = match self.get(&format!("prekey_identity:{}", peer))? {
            Some(v) => v,
            None => return Ok(None),
        };
        let spk_b64 = self
            .get(&format!("prekey_spk:{}", peer))?
            .ok_or_else(|| ChattorError::Crypto(format!("Missing PreKey SPK for {}", peer)))?;
        let opk = match self.get(&format!("prekey_opk:{}", peer))? {
            Some(b64) => Some(Self::decode32(&b64, "PreKey OPK")?),
            None => None,
        };
        Ok(Some(PreKeyPrivateMaterial {
            identity_secret: Self::decode32(&identity_b64, "PreKey identity")?,
            signed_prekey_secret: Self::decode32(&spk_b64, "PreKey SPK")?,
            prekey_secret: opk,
        }))
    }

    /// Load the stored signal identity secret for `peer` (None if absent).
    pub fn load_signal_identity_secret(&self, peer: &str) -> Result<Option<[u8; 32]>> {
        match self.get(&format!("signal_identity_secret:{}", peer))? {
            Some(b64) => Ok(Some(Self::decode32(&b64, "signal identity secret")?)),
            None => Ok(None),
        }
    }

    /// Delete all establishment material for `peer` (idempotent).
    pub fn delete(&self, peer: &str) -> Result<()> {
        let conn = self.db.connection();
        conn.execute(
            "DELETE FROM app_settings WHERE key LIKE ?1",
            [&format!("prekey_%:{}", peer)],
        )
        .map_err(|e| ChattorError::Database(format!("PreKeyStore delete failed: {}", e)))?;
        conn.execute(
            "DELETE FROM app_settings WHERE key = ?1",
            [&format!("signal_identity_secret:{}", peer)],
        )
        .map_err(|e| ChattorError::Database(format!("PreKeyStore delete failed: {}", e)))?;
        Ok(())
    }

    /// Delete material older than `max_age_secs`; returns the count of peers cleaned.
    pub fn cleanup_stale(&self, max_age_secs: u64) -> Result<usize> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let conn = self.db.connection();
        let mut stmt = conn
            .prepare("SELECT key, value FROM app_settings WHERE key LIKE 'prekey_created_at:%'")
            .map_err(|e| ChattorError::Database(format!("cleanup query failed: {}", e)))?;
        let stale_peers: Vec<String> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| ChattorError::Database(format!("cleanup read failed: {}", e)))?
            .filter_map(|r| r.ok())
            .filter_map(|(key, ts_str)| {
                let ts: u64 = ts_str.parse().ok()?;
                if now.saturating_sub(ts) > max_age_secs {
                    key.strip_prefix("prekey_created_at:").map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        let count = stale_peers.len();
        for peer in &stale_peers {
            self.delete(peer).ok();
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn temp_db() -> (Database, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path()).unwrap();
        (db, temp)
    }

    fn material(with_opk: bool) -> PreKeyPrivateMaterial {
        PreKeyPrivateMaterial {
            identity_secret: [1u8; 32],
            signed_prekey_secret: [2u8; 32],
            prekey_secret: if with_opk { Some([3u8; 32]) } else { None },
        }
    }

    #[test]
    fn test_store_and_load() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        store.store("peer.onion", &material(true), &[9u8; 32], 1000).unwrap();
        let loaded = store.load("peer.onion").unwrap().unwrap();
        assert_eq!(loaded.identity_secret, [1u8; 32]);
        assert_eq!(loaded.signed_prekey_secret, [2u8; 32]);
        assert_eq!(loaded.prekey_secret, Some([3u8; 32]));
        assert_eq!(store.load_signal_identity_secret("peer.onion").unwrap(), Some([9u8; 32]));
    }

    #[test]
    fn test_load_absent_returns_none() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        assert!(store.load("nobody.onion").unwrap().is_none());
        assert!(store.load_signal_identity_secret("nobody.onion").unwrap().is_none());
    }

    #[test]
    fn test_store_without_opk() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        store.store("peer.onion", &material(false), &[9u8; 32], 1000).unwrap();
        let loaded = store.load("peer.onion").unwrap().unwrap();
        assert_eq!(loaded.prekey_secret, None);
    }

    #[test]
    fn test_delete_is_idempotent() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        store.store("peer.onion", &material(true), &[9u8; 32], 1000).unwrap();
        store.delete("peer.onion").unwrap();
        store.delete("peer.onion").unwrap();
        assert!(store.load("peer.onion").unwrap().is_none());
        assert!(store.load_signal_identity_secret("peer.onion").unwrap().is_none());
    }

    #[test]
    fn test_cleanup_stale() {
        let (db, _t) = temp_db();
        let store = PreKeyStore::new(&db);
        store.store("old.onion", &material(true), &[9u8; 32], 0).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        store.store("new.onion", &material(true), &[8u8; 32], now).unwrap();
        let cleaned = store.cleanup_stale(60).unwrap();
        assert_eq!(cleaned, 1);
        assert!(store.load("old.onion").unwrap().is_none());
        assert!(store.load("new.onion").unwrap().is_some());
    }
}
