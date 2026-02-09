use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use rand::rngs::OsRng;
use crate::error::{Result, TorrentChatError};
use crate::db::Database;

/// User identity keypair (Ed25519)
pub struct IdentityKeypair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl IdentityKeypair {
    /// Generate a new random identity keypair
    pub fn generate() -> Result<Self> {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        Ok(IdentityKeypair { signing_key, verifying_key })
    }

    /// Load identity from database or generate new one
    pub fn load_or_generate(db: &Database) -> Result<Self> {
        let conn = db.connection();

        // Try to load existing identity
        let existing: rusqlite::Result<Vec<u8>> = conn.query_row(
            "SELECT value FROM settings WHERE key = 'identity_keypair'",
            [],
            |row| row.get(0)
        );

        match existing {
            Ok(bytes) => {
                // Deserialize keypair
                Self::from_bytes(&bytes)
                    .map_err(|e| TorrentChatError::Crypto(format!("Failed to load identity: {}", e)))
            }
            Err(_) => {
                // Generate new keypair
                let keypair = Self::generate()?;
                let bytes = keypair.to_bytes();

                // Store in database
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('identity_keypair', ?1)",
                    [&bytes],
                ).map_err(|e| TorrentChatError::Database(format!("Failed to store identity: {}", e)))?;

                Ok(keypair)
            }
        }
    }

    /// Serialize keypair to bytes
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.signing_key.to_bytes().as_ref());
        bytes
    }

    /// Deserialize keypair from bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(TorrentChatError::Crypto("Invalid identity key length".into()));
        }

        let signing_key = SigningKey::from_bytes(bytes.try_into().unwrap());
        let verifying_key = signing_key.verifying_key();

        Ok(IdentityKeypair {
            signing_key,
            verifying_key,
        })
    }

    /// Derive .onion address from identity key (v3 format)
    pub fn to_onion_address(&self) -> String {
        use sha3::{Sha3_256, Digest};
        use base32::Alphabet;

        let public_key_bytes = self.verifying_key.to_bytes();

        // v3 onion address format: base32(public_key || checksum || version)
        let version = 0x03u8;
        let mut hasher = Sha3_256::new();
        hasher.update(b".onion checksum");
        hasher.update(&public_key_bytes);
        hasher.update(&[version]);
        let checksum = &hasher.finalize()[0..2];

        let mut address_bytes = Vec::new();
        address_bytes.extend_from_slice(&public_key_bytes);
        address_bytes.extend_from_slice(checksum);
        address_bytes.push(version);

        let encoded = base32::encode(Alphabet::RFC4648 { padding: false }, &address_bytes)
            .to_lowercase();

        format!("{}.onion", encoded)
    }

    /// Get the public key
    pub fn public_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Get the secret key bytes
    pub fn secret_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Verify a signature
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        self.verifying_key
            .verify(message, signature)
            .map_err(|e| TorrentChatError::Crypto(format!("Signature verification failed: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_identity_keypair() {
        let keypair = IdentityKeypair::generate();
        assert!(keypair.is_ok());
    }

    #[test]
    fn test_sign_and_verify() {
        let keypair = IdentityKeypair::generate().unwrap();
        let message = b"Hello, Tor!";

        let signature = keypair.sign(message);
        let result = keypair.verify(message, &signature);

        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_invalid_signature() {
        let keypair = IdentityKeypair::generate().unwrap();
        let message = b"Hello, Tor!";
        let wrong_message = b"Wrong message";

        let signature = keypair.sign(message);
        let result = keypair.verify(wrong_message, &signature);

        assert!(result.is_err());
    }
}
