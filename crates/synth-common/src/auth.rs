use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimsData {
    pub sub: String,
    pub email: String,
    pub kind: String,
    pub iat: u64,
    pub exp: u64,
}

pub fn hash_password(plain: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
}

pub fn verify_password(hash: &str, plain: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => {
            // Fallback: support dovecot {scram-sha-256} marker pass-through (no verify here)
            false
        }
    }
}

pub fn create_session_token(
    secret: &str,
    user_id: &str,
    email: &str,
    kind: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::hours(24);
    let claims = ClaimsData {
        sub: user_id.to_string(),
        email: email.to_string(),
        kind: kind.to_string(),
        iat: now.timestamp() as u64,
        exp: exp.timestamp() as u64,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

pub fn decode_session_token(
    secret: &str,
    token: &str,
) -> Result<ClaimsData, jsonwebtoken::errors::Error> {
    let dec_key = DecodingKey::from_secret(secret.as_bytes());
    let data = decode::<ClaimsData>(token, &dec_key, &Validation::default())?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_and_verify() {
        let h = hash_password("secret123").unwrap();
        assert!(h.starts_with("$argon2"));
        assert!(verify_password(&h, "secret123"));
        assert!(!verify_password(&h, "wrong"));
    }
    #[test]
    fn token_roundtrip() {
        let t = create_session_token("0123456789abcdef0123456789abcdef00", "u1", "a@b.c", "admin")
            .unwrap();
        let d = decode_session_token("0123456789abcdef0123456789abcdef00", &t).unwrap();
        assert_eq!(d.email, "a@b.c");
        assert_eq!(d.sub, "u1");
    }
}
