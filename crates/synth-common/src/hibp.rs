use sha1::{Digest, Sha1};

pub fn sha1_prefix(password: &str) -> (String, String) {
    let hash = Sha1::digest(password.as_bytes());
    let hex = format!("{:X}", hash);
    (hex[..5].to_string(), hex[5..].to_string())
}

pub fn in_breach_list(range_body: &str, suffix: &str) -> bool {
    range_body.lines().map(str::trim).any(|l| {
        l.split(':')
            .next()
            .unwrap_or("")
            .eq_ignore_ascii_case(suffix)
    })
}

pub async fn check_breached(password: &str) -> bool {
    let (prefix, suffix) = sha1_prefix(password);
    // Avoid blocking in request path if offline: callers treat Err as "not breached"
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok();
    if let Some(c) = client {
        let url = format!("https://api.pwnedpasswords.com/range/{}", prefix);
        if let Ok(resp) = c.get(&url).header("User-Agent", "SynthMail").send().await
            && let Ok(body) = resp.text().await
        {
            return in_breach_list(&body, &suffix);
        }
    }
    false
}

pub fn enforce_not_breached(password: &str, range_body: &str) -> Result<(), String> {
    let (_, suffix) = sha1_prefix(password);
    if in_breach_list(range_body, &suffix) {
        return Err("password appears in a known breach list".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_splits() {
        let (p, s) = sha1_prefix("password");
        assert_eq!(p.len(), 5);
        assert!(!s.is_empty());
        // SHA1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        assert_eq!(p, "5BAA6");
    }
    #[test]
    fn breach_match_case_insensitive() {
        let body = "1E68FD8:3730471\nABCDEF:1";
        assert!(in_breach_list(body, "1e68fd8"));
        assert!(!in_breach_list(body, "00000"));
    }
}
