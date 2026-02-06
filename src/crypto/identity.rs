use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use rand::rngs::OsRng;
use crate::error::{Result, TorrentChatError};

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
