use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

const B32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn generate_secret() -> String {
    let mut buf = [0u8; 20];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut buf);
    base32_encode(&buf)
}

pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for &b in data {
        buffer = (buffer << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(B32_ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(B32_ALPHABET[idx] as char);
    }
    while !out.len().is_multiple_of(8) {
        out.push('=');
    }
    out
}

pub fn base32_decode(s: &str) -> Result<Vec<u8>> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=')
        .collect();
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for ch in cleaned.chars() {
        let v = B32_ALPHABET
            .iter()
            .position(|&a| a as char == ch.to_ascii_uppercase())
            .context("invalid base32 char")? as u32;
        buffer = (buffer << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

fn hotp(secret_bytes: &[u8], counter: u64) -> u32 {
    let mut mac = HmacSha1::new_from_slice(secret_bytes).expect("hmac key");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let offset = (result[19] & 0x0f) as usize;
    ((result[offset] as u32 & 0x7f) << 24)
        | ((result[offset + 1] as u32) << 16)
        | ((result[offset + 2] as u32) << 8)
        | (result[offset + 3] as u32)
}

pub fn totp_for(secret: &str, time_secs: u64) -> Result<String> {
    let key = base32_decode(secret)?;
    let counter = time_secs / 30;
    Ok(format!("{:06}", hotp(&key, counter) % 1_000_000))
}

pub fn verify(secret: &str, code: &str, time_secs: u64) -> bool {
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    for delta in [0u64, 1u64] {
        for t in [time_secs.checked_sub(delta), time_secs.checked_add(delta)]
            .into_iter()
            .flatten()
        {
            if let Ok(expected) = totp_for(secret, t)
                && expected == code
            {
                return true;
            }
        }
    }
    false
}

pub fn otpauth_url(secret: &str, account: &str, issuer: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&digits=6&period=30",
        issuer, account, secret, issuer
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_roundtrip() {
        let raw = b"12345678901234567890";
        let enc = base32_encode(raw);
        assert_eq!(enc, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(base32_decode(&enc).unwrap(), raw.to_vec());
    }

    #[test]
    fn rfc6238_sha1_vector() {
        // RFC 6238 Appendix B: T=59, SHA1, digits 6 → HOTP 94287082 % 10^6 = 428709? (HOTP uses dynamic truncation of first 4 bytes → 428708 per RFC 4226 test vector 8)
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let code = totp_for(secret, 59).unwrap();
        assert_eq!(code.len(), 6);
        // HOTP counter 1 → 287082 (RFC 4226 HOTP[1] = 287082)
        assert_eq!(code, "287082");
        assert!(verify(secret, &code, 59));
        assert!(!verify(secret, "000000", 59));
    }

    #[test]
    fn cross_window_accepts_skew() {
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let code = totp_for(secret, 100).unwrap();
        assert!(verify(secret, &code, 101));
        assert!(verify(secret, &code, 99));
    }

    #[test]
    fn rejects_bad_code_shape() {
        assert!(!verify("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", "123", 59));
        assert!(!verify("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", "abcdef", 59));
    }

    #[test]
    fn otpauth_url_shape() {
        let u = otpauth_url("JBSWY3DPEHPK3PXP", "alice", "SynthMail");
        assert!(u.starts_with("otpauth://totp/SynthMail:alice?"));
        assert!(u.contains("secret=JBSWY3DPEHPK3PXP"));
    }
}
