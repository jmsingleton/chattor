use crate::error::{ChattorError, Result};
use std::path::Path;

/// Derive a 32-byte database encryption key from a passphrase using Argon2id.
///
/// Parameters (OWASP recommended):
/// - Memory: 64 MB
/// - Iterations: 3
/// - Parallelism: 4
pub fn derive_key(passphrase: &[u8], salt: &[u8; 16]) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(65536, 3, 4, Some(32))
        .map_err(|e| ChattorError::Crypto(format!("Argon2 params error: {}", e)))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| ChattorError::Crypto(format!("Argon2 key derivation failed: {}", e)))?;

    Ok(key)
}

/// Load or generate the salt file. Returns the 16-byte salt.
pub fn load_or_create_salt(salt_path: &Path) -> Result<[u8; 16]> {
    if salt_path.exists() {
        let bytes = std::fs::read(salt_path).map_err(ChattorError::Io)?;
        if bytes.len() != 16 {
            return Err(ChattorError::Crypto("Invalid salt file length".into()));
        }
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&bytes);
        Ok(salt)
    } else {
        use rand::Rng;
        let salt: [u8; 16] = rand::thread_rng().gen();
        std::fs::write(salt_path, salt).map_err(ChattorError::Io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(salt_path, std::fs::Permissions::from_mode(0o600))
                .map_err(ChattorError::Io)?;
        }
        Ok(salt)
    }
}

/// Check if a database file is unencrypted (for migration).
pub fn is_unencrypted(db_path: &Path) -> bool {
    if !db_path.exists() {
        return false; // No DB yet, will be created encrypted
    }
    // Try to open without a key and read
    match rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    {
        Ok(conn) => conn
            .query_row("SELECT count(*) FROM schema_version", [], |_| Ok(()))
            .is_ok(),
        Err(_) => false, // Can't open = likely encrypted or corrupt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_deterministic() {
        let salt = [1u8; 16];
        let key1 = derive_key(b"test_password", &salt).unwrap();
        let key2 = derive_key(b"test_password", &salt).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_derive_key_different_passphrase() {
        let salt = [1u8; 16];
        let key1 = derive_key(b"password1", &salt).unwrap();
        let key2 = derive_key(b"password2", &salt).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_key_different_salt() {
        let salt1 = [1u8; 16];
        let salt2 = [2u8; 16];
        let key1 = derive_key(b"password", &salt1).unwrap();
        let key2 = derive_key(b"password", &salt2).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_load_or_create_salt_creates_new() {
        let temp_dir = tempfile::tempdir().unwrap();
        let salt_path = temp_dir.path().join("test.salt");

        let salt = load_or_create_salt(&salt_path).unwrap();
        assert_eq!(salt.len(), 16);
        assert!(salt_path.exists());
    }

    #[test]
    fn test_load_or_create_salt_loads_existing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let salt_path = temp_dir.path().join("test.salt");

        let salt1 = load_or_create_salt(&salt_path).unwrap();
        let salt2 = load_or_create_salt(&salt_path).unwrap();
        assert_eq!(salt1, salt2);
    }

    #[test]
    fn test_is_unencrypted_no_file() {
        assert!(!is_unencrypted(std::path::Path::new(
            "/tmp/nonexistent-chattor-test.db"
        )));
    }

    #[test]
    fn test_is_unencrypted_plain_db() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let db = crate::db::Database::open(temp_file.path()).unwrap();
        drop(db);
        assert!(is_unencrypted(temp_file.path()));
    }
}
