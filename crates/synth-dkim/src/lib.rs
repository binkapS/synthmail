//! DKIM key generation (RSA-2048 + Ed25519) and opendkim config sync.

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::SigningKey;
use pkcs8::der::pem::LineEnding;
use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};
use rsa::{RsaPrivateKey, RsaPublicKey};

#[derive(Debug, Clone)]
pub struct DkimKeyPair {
    pub selector: String,
    pub public_key_b64: String,
    pub private_key_pem: String,
    pub dns_txt: String,
    pub key_type: String,
}

pub fn generate_rsa2048(selector: &str) -> Result<DkimKeyPair> {
    let mut rng = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, 2048).context("rsa keygen")?;
    let pub_key: RsaPublicKey = priv_key.to_public_key();
    let pub_der = pub_key.to_pkcs1_der().context("pub der")?;
    let pub_b64 = B64.encode(pub_der.as_bytes());

    let priv_pem = priv_key.to_pkcs1_pem(LineEnding::LF).context("priv pem")?;

    let dns_txt = format!("v=DKIM1; k=rsa; p={}", pub_b64);

    Ok(DkimKeyPair {
        selector: selector.to_string(),
        public_key_b64: pub_b64,
        private_key_pem: priv_pem.to_string(),
        dns_txt,
        key_type: "rsa2048".to_string(),
    })
}

pub fn generate_ed25519(selector: &str) -> Result<DkimKeyPair> {
    let mut rng = rand::thread_rng();
    let signing = SigningKey::generate(&mut rng);
    let verifying = signing.verifying_key();
    let pub_bytes = verifying.to_bytes();
    let pub_b64 = B64.encode(pub_bytes);

    // Store raw seed as PEM-like; opendkim uses RSA, ed25519 for future use
    let priv_b64 = B64.encode(signing.to_bytes());
    let priv_pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        wrap_b64(&priv_b64)
    );

    let dns_txt = format!("v=DKIM1; k=ed25519; p={}", pub_b64);

    Ok(DkimKeyPair {
        selector: selector.to_string(),
        public_key_b64: pub_b64,
        private_key_pem: priv_pem,
        dns_txt,
        key_type: "ed25519".to_string(),
    })
}

pub fn generate(selector: &str, key_type: &str) -> Result<DkimKeyPair> {
    match key_type {
        "ed25519" => generate_ed25519(selector),
        _ => generate_rsa2048(selector),
    }
}

pub fn opendkim_keytable_entry(domain: &str, pair: &DkimKeyPair) -> String {
    format!(
        "{}._domainkey.{} {}:{}:/etc/opendkim/keys/{}/default.private\n",
        pair.selector, domain, domain, pair.selector, pair.selector
    )
}

pub fn opendkim_signingtable_entry(domain: &str, selector: &str) -> String {
    format!("@{} {}._domainkey.{}\n", domain, selector, domain)
}

fn wrap_b64(s: &str) -> String {
    s.chars()
        .collect::<Vec<char>>()
        .chunks(64)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rsa_generates() {
        let k = generate_rsa2048("default").unwrap();
        assert!(k.dns_txt.starts_with("v=DKIM1; k=rsa; p="));
        assert!(k.private_key_pem.contains("BEGIN"));
    }
    #[test]
    fn ed25519_generates() {
        let k = generate_ed25519("s2").unwrap();
        assert!(k.dns_txt.contains("k=ed25519"));
        assert!(!k.public_key_b64.is_empty());
    }
    #[test]
    fn keytable_format() {
        let k = DkimKeyPair {
            selector: "default".into(),
            public_key_b64: "x".into(),
            private_key_pem: "y".into(),
            dns_txt: "z".into(),
            key_type: "rsa2048".into(),
        };
        assert!(opendkim_keytable_entry("example.com", &k).contains("example.com"));
    }
}
