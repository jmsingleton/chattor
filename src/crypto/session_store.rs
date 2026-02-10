use crate::db::Database;
use crate::error::{Result, TorrentChatError};
use crate::crypto::signal::SignalSession;

/// Store for Signal Protocol sessions
pub struct SessionStore<'a> {
    db: &'a Database,
}

impl<'a> SessionStore<'a> {
    /// Create new session store
    pub fn new(db: &'a Database) -> Self {
        SessionStore { db }
    }

    /// Store session in database
    pub fn store_session(&self, session: &SignalSession) -> Result<()> {
        let conn = self.db.connection();
        let session_bytes = session.to_bytes();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            "INSERT OR REPLACE INTO signal_sessions (remote_onion, session_state, updated_at)
             VALUES (?1, ?2, ?3)",
            (
                &session.remote_onion,
                &session_bytes,
                now,
            ),
        ).map_err(|e| TorrentChatError::Database(format!("Failed to store session: {}", e)))?;

        Ok(())
    }

    /// Load session from database
    pub fn load_session(&self, remote_onion: &str) -> Result<Option<SignalSession>> {
        let conn = self.db.connection();

        let result: rusqlite::Result<Vec<u8>> = conn.query_row(
            "SELECT session_state FROM signal_sessions WHERE remote_onion = ?1",
            [remote_onion],
            |row| row.get(0),
        );

        match result {
            Ok(bytes) => {
                let session = SignalSession::from_bytes(remote_onion.to_string(), bytes)?;
                Ok(Some(session))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(TorrentChatError::Database(format!("Failed to load session: {}", e))),
        }
    }

    /// Delete session from database
    pub fn delete_session(&self, remote_onion: &str) -> Result<()> {
        let conn = self.db.connection();

        conn.execute(
            "DELETE FROM signal_sessions WHERE remote_onion = ?1",
            [remote_onion],
        ).map_err(|e| TorrentChatError::Database(format!("Failed to delete session: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_store_and_load_session() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();

        let store = SessionStore::new(&db);

        // Create and store session
        let bundle = crate::crypto::signal::PreKeyBundle::generate().unwrap();
        let session = crate::crypto::signal::SignalSession::from_prekey_bundle(
            "test.onion".into(),
            &bundle
        ).unwrap();

        store.store_session(&session).unwrap();

        // Load session
        let loaded = store.load_session("test.onion").unwrap();
        assert!(loaded.is_some());

        let loaded_session = loaded.unwrap();
        assert_eq!(loaded_session.remote_onion, "test.onion");
    }

    #[test]
    fn test_session_serialization() {
        let temp_db = NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_db.path()).unwrap();
        let store = SessionStore::new(&db);

        // Create real session
        let alice_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let bob_identity = crate::crypto::IdentityKeypair::generate().unwrap();
        let (bob_bundle, bob_private) = crate::crypto::signal::PreKeyBundle::generate_real(&bob_identity).unwrap();

        let session = crate::crypto::signal::SignalSession::from_prekey_bundle_real(
            "bob.onion".into(),
            &bob_bundle,
            &bob_private,
            &alice_identity,
        ).unwrap();

        // Store session
        store.store_session(&session).unwrap();

        // Load session
        let loaded = store.load_session("bob.onion").unwrap();
        assert!(loaded.is_some());

        // Verify session state is preserved
        let mut loaded_session = loaded.unwrap();
        // Encrypt with loaded session to verify it works
        let (ciphertext, _) = loaded_session.encrypt(b"test").unwrap();
        assert!(ciphertext.len() > 0);
    }
}
