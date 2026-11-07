use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

pub const TOKEN_ISSUER: &str = "synthmail";
pub const TOKEN_AUDIENCE: &str = "synthmail";
pub const SESSION_TTL_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimsData {
    pub sub: String,
    pub email: String,
    pub kind: String,
    pub iat: u64,
    pub exp: u64,
    #[serde(default)]
    pub tv: u64,
    #[serde(default)]
    pub iss: String,
    #[serde(default)]
    pub aud: String,
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
        Err(_) => false,
    }
}

pub fn create_session_token(
    secret: &str,
    user_id: &str,
    email: &str,
    kind: &str,
    token_version: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::hours(SESSION_TTL_HOURS);
    let claims = ClaimsData {
        sub: user_id.to_string(),
        email: email.to_string(),
        kind: kind.to_string(),
        iat: now.timestamp() as u64,
        exp: exp.timestamp() as u64,
        tv: token_version,
        iss: TOKEN_ISSUER.to_string(),
        aud: TOKEN_AUDIENCE.to_string(),
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
    let mut validation = Validation::default();
    validation.set_issuer(&[TOKEN_ISSUER]);
    validation.set_audience(&[TOKEN_AUDIENCE]);
    let dec_key = DecodingKey::from_secret(secret.as_bytes());
    let data = decode::<ClaimsData>(token, &dec_key, &validation)?;
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
        let t = create_session_token(
            "0123456789abcdef0123456789abcdef00",
            "u1",
            "a@b.c",
            "admin",
            3,
        )
        .unwrap();
        let d = decode_session_token("0123456789abcdef0123456789abcdef00", &t).unwrap();
        assert_eq!(d.email, "a@b.c");
        assert_eq!(d.sub, "u1");
        assert_eq!(d.tv, 3);
        assert_eq!(d.iss, TOKEN_ISSUER);
    }
    #[test]
    fn token_rejects_wrong_secret() {
        let t = create_session_token(
            "0123456789abcdef0123456789abcdef00",
            "u1",
            "a@b.c",
            "user",
            0,
        )
        .unwrap();
        assert!(decode_session_token("ffffffffffffffffffffffffffffffff00", &t).is_err());
    }
    #[test]
    fn token_is_24h() {
        let t = create_session_token(
            "0123456789abcdef0123456789abcdef00",
            "u1",
            "a@b.c",
            "user",
            0,
        )
        .unwrap();
        let d = decode_session_token("0123456789abcdef0123456789abcdef00", &t).unwrap();
        assert_eq!(d.exp - d.iat, (SESSION_TTL_HOURS as u64) * 3600);
    }
}
