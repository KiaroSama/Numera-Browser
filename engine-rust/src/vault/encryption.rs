//! Fallible Fernet framing using the same RustCrypto primitives as the reader.
//! The dependency's public encrypt API panics when its private IV RNG fails.
use super::VaultError;
use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::{Engine, engine::general_purpose::URL_SAFE};
use hmac::{KeyInit, Mac};
use pbkdf2::sha2::Sha256;

pub(super) fn seal(key: &[u8; 32], plaintext: &[u8], timestamp: u64) -> Result<String, VaultError> {
    let mut iv = [0u8; 16];
    #[cfg(feature = "test-support")]
    super::testing::event("before-vault-iv-randomness", std::path::Path::new("")).map_err(
        |error| VaultError::caused(format!("Vault IV randomness unavailable: {error}"), error),
    )?;
    getrandom::fill(&mut iv).map_err(|error| {
        VaultError::caused(format!("Vault IV randomness unavailable: {error}"), error)
    })?;
    // Fernet splits the 32-byte key into a 16-byte signing key followed by
    // the AES-128 key, and authenticates version, timestamp, IV and ciphertext.
    let encrypted = cbc::Encryptor::<aes::Aes128>::new_from_slices(&key[16..], &iv)
        .map_err(|_| VaultError::message("vault derived key is invalid".into()))?
        .encrypt_padded_vec_mut::<Pkcs7>(plaintext);
    let mut token = Vec::with_capacity(25 + encrypted.len() + 32);
    token.push(0x80);
    token.extend_from_slice(&timestamp.to_be_bytes());
    token.extend_from_slice(&iv);
    token.extend_from_slice(&encrypted);
    let mut authenticator = hmac::Hmac::<Sha256>::new_from_slice(&key[..16])
        .map_err(|_| VaultError::message("vault signing key is invalid".into()))?;
    authenticator.update(&token);
    token.extend_from_slice(&authenticator.finalize().into_bytes());
    Ok(URL_SAFE.encode(token))
}
