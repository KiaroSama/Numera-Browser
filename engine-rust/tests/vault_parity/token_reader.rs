//! Standalone Fernet-v2 verifier. It links neither numera_core nor fernet:
//! authenticated framing, HMAC and CBC/PKCS7 handling are checked independently.
use aes::cipher::{BlockDecrypt, KeyInit};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE},
};
use pbkdf2::sha2::{Digest, Sha256};
use serde_json::Value;
fn read_token(
    password: &str,
    salt: &str,
    envelope: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let salt = STANDARD.decode(salt)?;
    let mut key = pbkdf2::pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), &salt, 600_000);
    let token = URL_SAFE.decode(
        envelope
            .strip_prefix("enc:v2:")
            .ok_or("Native v2 envelope missing")?,
    )?;
    if token.len() < 73 || token[0] != 0x80 {
        return Err("Invalid Fernet framing".into());
    }
    let signed = token.len() - 32;
    if (signed - 25) % 16 != 0 {
        return Err("Invalid CBC ciphertext length".into());
    }
    let mut inner_pad = [0x36u8; 64];
    let mut outer_pad = [0x5cu8; 64];
    for index in 0..16 {
        inner_pad[index] ^= key[index];
        outer_pad[index] ^= key[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(&token[..signed]);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    assert_eq!(
        &outer.finalize()[..],
        &token[signed..],
        "Native token HMAC authentication failed"
    );
    let cipher = aes::Aes128::new_from_slice(&key[16..]).map_err(|_| "Invalid AES key")?;
    key.fill(0);
    let mut previous = token[9..25].to_vec();
    let mut plaintext = Vec::with_capacity(signed - 25);
    for ciphertext in token[25..signed].chunks_exact(16) {
        let mut block = aes::cipher::Block::<aes::Aes128>::default();
        block.copy_from_slice(ciphertext);
        cipher.decrypt_block(&mut block);
        plaintext.extend(block.iter().zip(&previous).map(|(byte, iv)| byte ^ iv));
        previous.copy_from_slice(ciphertext);
    }
    let padding = usize::from(*plaintext.last().ok_or("Missing PKCS7 padding")?);
    if !(1..=16).contains(&padding)
        || plaintext[plaintext.len() - padding..]
            .iter()
            .any(|byte| usize::from(*byte) != padding)
    {
        return Err("Invalid PKCS7 padding".into());
    }
    plaintext.truncate(plaintext.len() - padding);
    Ok(plaintext)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args_os()
        .nth(1)
        .ok_or("Native token document missing")?;
    let rows: Vec<Value> = serde_json::from_slice(&std::fs::read(input)?)?;
    for row in &rows {
        let actual = read_token(
            row["password"].as_str().ok_or("Password missing")?,
            row["salt"].as_str().ok_or("Salt missing")?,
            row["token"].as_str().ok_or("Token missing")?,
        )?;
        assert_eq!(
            actual,
            row["expected"]
                .as_str()
                .ok_or("Plaintext missing")?
                .as_bytes()
        );
    }
    println!("{}", rows.len());
    Ok(())
}
