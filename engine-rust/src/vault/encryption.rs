//! Fallible Fernet framing using the same RustCrypto primitives as the reader.
//! The dependency's public encrypt API panics when its private IV RNG fails.
use super::VaultError;
use crate::credential_envelope;

pub(super) fn seal(key: &[u8; 32], plaintext: &[u8], timestamp: u64) -> Result<String, VaultError> {
    #[cfg(feature = "test-support")]
    super::testing::event("before-vault-iv-randomness", std::path::Path::new("")).map_err(
        |error| VaultError::caused(format!("Vault IV randomness unavailable: {error}"), error),
    )?;
    credential_envelope::seal(key, plaintext, timestamp).map_err(|error| match error {
        credential_envelope::Error::Randomness(error) => {
            VaultError::caused(format!("Vault IV randomness unavailable: {error}"), error)
        }
        credential_envelope::Error::EncryptionKey => {
            VaultError::message("vault derived key is invalid".into())
        }
        credential_envelope::Error::SigningKey => {
            VaultError::message("vault signing key is invalid".into())
        }
    })
}
