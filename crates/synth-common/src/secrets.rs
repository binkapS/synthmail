//! At-rest encryption for sensitive strings (DKIM private keys, TOTP secrets).
//! Audit M-15 / H-07: `dkim_keys.private_key_enc` and `user_totp.secret` were
//! stored verbatim. Values are AES-256-GCM ciphertext, prefixed `enc:` and
//! base64-url-encoded (nonce ‖ ciphertext) so legacy plaintext remains
//! readable during rollout (decrypt falls back to plaintext).

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use anyhow::Result;

const PREFIX: &str = "enc:";

type AesGcm256 = Aes256Gcm;

fn key_from_hex(hex: &str) -> Result<Vec<u8>> {
    let hex = hex.trim();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("ENCRYPTION_KEY must be 64 hex characters (32 bytes)");
    }
    (0..32)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|e| e.into()))
        .collect()
}

/// Encrypt `plaintext` with AES-256-GCM. `secret` is the 64-hex-char
/// `ENCRYPTION_KEY` (see `Config::validate`).
pub fn encrypt(secret: &str, plaintext: &str) -> Result<String> {
    let key_bytes = key_from_hex(secret)?;
    let cipher = AesGcm256::new_from_slice(&key_bytes).map_err(|e| anyhow::anyhow!(e))?;
    let nonce_bytes: [u8; 12] = {
        use rand::RngCore;
        let mut b = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut b);
        b
    };
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut blob = Vec::with_capacity(12 + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    Ok(format!(
        "{}{}",
        PREFIX,
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &blob)
    ))
}

/// Decrypt a value produced by [`encrypt`]. Legacy values without the
/// `enc:` prefix are returned verbatim (rollout compatibility).
pub fn decrypt(secret: &str, encoded: &str) -> Result<String> {
    if let Some(b64) = encoded.strip_prefix(PREFIX) {
        let key_bytes = key_from_hex(secret)?;
        let cipher = AesGcm256::new_from_slice(&key_bytes).map_err(|e| anyhow::anyhow!(e))?;
        let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)?;
        if blob.len() < 13 {
            anyhow::bail!("ciphertext too short");
        }
        let (nonce_bytes, ciphertext) = blob.split_at(12);
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(String::from_utf8(plaintext)?)
    } else {
        // Legacy plaintext — decrypt is a no-op so existing rows keep working.
        Ok(encoded.to_string())
    }
}

/// Whether the value is encrypted (helper for admin tooling).
pub fn is_encrypted(encoded: &str) -> bool {
    encoded.starts_with(PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> String {
        "00".repeat(32)
    }

    #[test]
    fn roundtrip() {
        let ct = encrypt(&test_key(), "-----BEGIN PRIVATE KEY-----secret").unwrap();
        assert!(ct.starts_with("enc:"));
        assert_ne!(ct, "-----BEGIN PRIVATE KEY-----secret");
        assert_eq!(
            decrypt(&test_key(), &ct).unwrap(),
            "-----BEGIN PRIVATE KEY-----secret"
        );
    }

    #[test]
    fn wrong_key_fails() {
        let ct = encrypt(&test_key(), "hello").unwrap();
        assert!(decrypt(&"ff".repeat(32), &ct).is_err());
    }

    #[test]
    fn legacy_plaintext_passthrough() {
        assert_eq!(
            decrypt(&test_key(), "JBSWY3DPEHPK3PXP").unwrap(),
            "JBSWY3DPEHPK3PXP"
        );
        assert!(!is_encrypted("JBSWY3DPEHPK3PXP"));
        assert!(is_encrypted(&encrypt(&test_key(), "x").unwrap()));
    }

    #[test]
    fn rejects_bad_key_length() {
        assert!(encrypt("short", "x").is_err());
        assert!(encrypt(&"zz".repeat(32), "x").is_err());
    }
}
