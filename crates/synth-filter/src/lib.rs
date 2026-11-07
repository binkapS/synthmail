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
    format!(
        r#"require ["vacation", "variables", "extension", "fileinto", "vnd.dovecot.debug"];

if true {{
  set "subject" "{subject}";
  set "bodytext" "{body}";
  set "from" "vacation@{domain}";
  set "days" "7";
  set "mode" "both";
  set "add_input" "off";
}}

vacation
  :days 7
  :subject "${{subject}}"
  :from {from}
  :addresses [${{from}}]
  :mode both
  "{body}";
"#,
        subject = escape_sieve_string(&cfg.subject),
        body = escape_sieve_string(&cfg.message),
        from = escape_sieve_string(&format!(
            "vacation@{}",
            cfg.user_id.split('@').nth(1).unwrap_or("example.com")
        )),
        domain = cfg.user_id.split('@').nth(1).unwrap_or("example.com"),
    )
}

pub fn generate_spam_filter_sieve(threshold: i32) -> String {
    format!(
        r#"require ["fileinto", "vnd.dovecot.debug", "vacation"];

# Score from Rspamd X-Spam-Score header, threshold {score}
if header :contains "X-Spam-Score" "{score}" {{
  # parsed externally; fallback to X-Spam header
}}

if header :contains "X-Spam-Status" "Yes" {{
  fileinto "Junk";
  stop;
}}

if header :contains "X-Spam-Threshold" "{t}" {{
  # placeholder
}}
"#,
        score = threshold,
        t = threshold,
    )
}

pub fn generate_alias_sieve(alias: &str, goto: &str) -> String {
    let dests: Vec<String> = goto
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let dest_list = dests
        .iter()
        .map(|d| format!("\"{}\"", escape_sieve_string(d)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"require ["fileinto", "redirect", "vacation"];
redirect :address [{dests}] "{alias}";
"#,
        alias = escape_sieve_string(alias),
        dests = dest_list,
    )
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
    // Cheap sanity checks; real validation via sieve-test in CI
    if content.trim().is_empty() {
        return Err("sieve script is empty".into());
    }
    if !content.contains("if") && !content.contains("require") && !content.contains("elsif") {
        return Err("sieve script must contain control flow (require/if)".into());
    }
    if content.contains("<?php") || content.contains("<script") {
        return Err("sieve script contains unexpected content".into());
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
    }
    #[test]
    fn escape_works() {
        assert_eq!(escape_sieve_string("a\"b"), "a\\\"b");
    }
    #[test]
    fn validate_rejects_empty() {
        assert!(validate_sieve("").is_err());
    }
}
