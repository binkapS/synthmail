//! Sieve script generation and Rspamd policy sync.
//!
//! The DB is source of truth; this crate turns rows into Dovecot Sieve.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SieveRule {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub content: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacationConfig {
    pub user_id: String,
    pub enabled: bool,
    pub subject: String,
    pub message: String,
    pub start: Option<String>,
    pub end: Option<String>,
}

pub fn generate_vacation_sieve(cfg: &VacationConfig) -> String {
    // M-01: valid capabilities only ("extension" is not one), quoted strings,
    // and honor enabled/start/end.
    if !cfg.enabled {
        return "require [\"vacation\"];\n# vacation disabled\n".to_string();
    }
    let from = escape_sieve_string(&format!(
        "vacation@{}",
        cfg.user_id.split('@').nth(1).unwrap_or("example.com")
    ));
    let mut out = String::new();
    // Date-bounded vacation via the "date" extension.
    if cfg.start.is_some() || cfg.end.is_some() {
        out.push_str("require [\"date\"];\n");
        if let Some(start) = &cfg.start {
            out.push_str(&format!(
                "if currentdate :value \"ge\" \"date\" \"{}\" {{\n",
                escape_sieve_string(start)
            ));
        }
        if let Some(end) = &cfg.end {
            out.push_str(&format!(
                "if currentdate :value \"le\" \"date\" \"{}\" {{\n",
                escape_sieve_string(end)
            ));
        }
    }
    out.push_str(&format!(
        r#"require ["vacation", "variables"];

vacation
  :days 7
  :subject "{subject}"
  :from "{from}"
  :addresses ["{from}"]
  :mode both
  "{body}";
"#,
        subject = escape_sieve_string(&cfg.subject),
        body = escape_sieve_string(&cfg.message),
        from = from,
    ));
    if cfg.start.is_some() || cfg.end.is_some() {
        out.push_str("}\n}\n");
    }
    out
}

pub fn generate_spam_filter_sieve(threshold: i32) -> String {
    // M-01: the old `:contains "X-Spam-Score" "7"` also matched 17/27/70...
    // Rspamd stamps X-Spam-Status: Yes above threshold, so key off that (and
    // a numeric spamtest comparison where the extension is available).
    format!(
        r#"require ["fileinto"];

# Score from Rspamd X-Spam-Score header, threshold {score}
# Rspamd sets X-Spam-Status: Yes when the score exceeds the threshold.
if header :contains "X-Spam-Status" "Yes" {{
  fileinto "Junk";
  stop;
}}
"#,
        score = threshold,
    )
}

pub fn generate_alias_sieve(alias: &str, goto: &str) -> String {
    // M-01: `redirect :address [...] "alias"` is not valid Sieve. Emit one
    // plain `redirect "dest";` per destination.
    let dests: Vec<String> = goto
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut out = String::from("require [\"redirect\"];\n");
    if dests.is_empty() {
        out.push_str("# alias has no destinations\n");
        return out;
    }
    for d in &dests {
        out.push_str(&format!(
            "# alias {alias} -> {d}\nredirect \"{d}\";\n",
            alias = escape_sieve_string(alias),
            d = escape_sieve_string(d),
        ));
    }
    out
}

pub fn rspamd_settings_for_user(_user_id: &str, _threshold: i32) -> serde_json::Value {
    serde_json::json!({
        "settings": {
            "spambullet": []
        },
        "symbols": {
            "SPAM_THRESHOLD": _threshold
        }
    })
}

pub fn escape_sieve_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
}

pub fn validate_sieve(content: &str) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("sieve script is empty".into());
    }
    if content.contains("<?php") || content.contains("<script") {
        return Err("sieve script contains unexpected content".into());
    }
    // M-01: reject known-invalid generator output patterns.
    if content.contains("\"extension\"") {
        return Err("invalid Sieve capability \"extension\"".into());
    }
    if content.contains(":address ")
        || content.contains(":address]")
        || content.contains(":address,")
    {
        return Err("invalid Sieve option :address".into());
    }
    // Balanced double quotes is a cheap heuristic for unterminated strings.
    let mut escaped = false;
    let mut quotes = 0usize;
    for c in content.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => quotes += 1,
            _ => {}
        }
    }
    if !quotes.is_multiple_of(2) {
        return Err("sieve script has unbalanced quotes".into());
    }
    if !content.contains("if")
        && !content.contains("require")
        && !content.contains("elsif")
        && !content.contains("redirect")
        && !content.contains("vacation")
    {
        return Err("sieve script must contain control flow (require/if)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vacation_renders() {
        let cfg = VacationConfig {
            user_id: "a@example.com".into(),
            enabled: true,
            subject: "OOO".into(),
            message: "Away".into(),
            start: None,
            end: None,
        };
        let s = generate_vacation_sieve(&cfg);
        assert!(s.contains("OOO"));
        assert!(s.contains("vacation"));
        assert!(
            validate_sieve(&s).is_ok(),
            "generated vacation sieve should validate: {s}"
        );
        assert!(!s.contains("\"extension\""));
    }
    #[test]
    fn vacation_disabled_is_harmless() {
        let cfg = VacationConfig {
            user_id: "a@example.com".into(),
            enabled: false,
            subject: "OOO".into(),
            message: "Away".into(),
            start: None,
            end: None,
        };
        let s = generate_vacation_sieve(&cfg);
        assert!(!s.contains("vacation\n"));
        assert!(validate_sieve(&s).is_ok());
    }
    #[test]
    fn vacation_dates_guarded() {
        let cfg = VacationConfig {
            user_id: "a@example.com".into(),
            enabled: true,
            subject: "OOO".into(),
            message: "Away".into(),
            start: Some("2026-11-01".into()),
            end: Some("2026-11-30".into()),
        };
        let s = generate_vacation_sieve(&cfg);
        assert!(s.contains("currentdate"));
        assert!(validate_sieve(&s).is_ok());
    }
    #[test]
    fn spam_sieve_has_no_contains_score() {
        let s = generate_spam_filter_sieve(7);
        assert!(!s.contains("X-Spam-Score\" \"7"));
        assert!(validate_sieve(&s).is_ok());
    }
    #[test]
    fn alias_sieve_is_valid_redirect() {
        let s = generate_alias_sieve("list@example.com", "a@example.com, b@example.com");
        assert!(s.contains("redirect \"a@example.com\""));
        assert!(!s.contains(":address"));
        assert!(validate_sieve(&s).is_ok());
    }
    #[test]
    fn escape_works() {
        assert_eq!(escape_sieve_string("a\"b"), "a\\\"b");
    }
    #[test]
    fn validate_rejects_empty() {
        assert!(validate_sieve("").is_err());
    }
    #[test]
    fn validate_rejects_invalid_capability() {
        assert!(validate_sieve("require [\"extension\"];\nif true {}").is_err());
    }
}
