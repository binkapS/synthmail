use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    #[serde(default)]
    pub session_secret: String,
    #[serde(default)]
    pub mail_host: String,
    #[serde(default)]
    pub mail_domain: String,
    #[serde(default = "default_max_quota")]
    pub max_quota_mb: i32,
    #[serde(default)]
    pub spam_threshold: i32,
    #[serde(default)]
    pub letsencrypt_email: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub control_url: String,
    #[serde(default)]
    pub webmail_url: String,
    #[serde(default)]
    pub admin_url: String,
    #[serde(default = "default_rate_limit_per_min")]
    pub rate_limit_per_min: u32,
    #[serde(default)]
    pub hibp_enabled: bool,
    #[serde(default = "default_totp_enforced_for_admin")]
    pub totp_enforced_for_admin: bool,
    #[serde(default = "default_min_password_len")]
    pub min_password_len: usize,
    #[serde(default)]
    pub encryption_key: String,
    /// CORS allow-list for the webmail/JMAP backend. Empty = allow
    /// `Any` (legacy behaviour, audit S-01). `*` = allow any origin
    /// explicitly. A comma-separated allow-list = restrict to those
    /// origins. Populated from `CORS_ORIGINS` in `from_env`.
    #[serde(default)]
    pub cors_origins: String,
    /// Parsed form of `cors_origins` — see `synth_webmail::routes`.
    #[serde(default, skip_deserializing)]
    pub cors_origins_list: Vec<String>,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8000
}
fn default_database_url() -> String {
    "postgres://synth:synth_dev@postgres:5432/synthmail".to_string()
}
fn default_redis_url() -> String {
    "redis://redis:6379".to_string()
}
fn default_max_quota() -> i32 {
    1024
}
fn default_timezone() -> String {
    "Etc/UTC".to_string()
}
fn default_rate_limit_per_min() -> u32 {
    120
}
fn default_totp_enforced_for_admin() -> bool {
    true
}
fn default_min_password_len() -> usize {
    12
}

impl std::default::Default for Config {
    fn default() -> Self {
        // NOTE: deliberately no default SESSION_SECRET — production deployments
        // must supply one (see validate()). Only :memory:-less dev flows that
        // override the secret themselves may construct Config directly.
        Self {
            host: default_host(),
            port: default_port(),
            database_url: default_database_url(),
            redis_url: default_redis_url(),
            session_secret: String::new(),
            mail_host: "mail.example.com".to_string(),
            mail_domain: "example.com".to_string(),
            max_quota_mb: default_max_quota(),
            spam_threshold: 7,
            letsencrypt_email: "admin@example.com".to_string(),
            timezone: default_timezone(),
            control_url: "https://mail.example.com/api".to_string(),
            webmail_url: "https://mail.example.com/mail".to_string(),
            admin_url: "https://mail.example.com/admin".to_string(),
            rate_limit_per_min: default_rate_limit_per_min(),
            hibp_enabled: false,
            totp_enforced_for_admin: true,
            min_password_len: default_min_password_len(),
            encryption_key: String::new(),
            cors_origins: String::new(),
            cors_origins_list: Vec::new(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("HOST") {
            cfg.host = v;
        }
        if let Ok(v) = std::env::var("PORT")
            && let Ok(p) = v.parse()
        {
            cfg.port = p;
        }
        if let Ok(v) = std::env::var("DATABASE_URL") {
            cfg.database_url = v;
        }
        if let Ok(v) = std::env::var("REDIS_URL") {
            cfg.redis_url = v;
        }
        if let Ok(v) = std::env::var("SESSION_SECRET") {
            cfg.session_secret = v;
        }
        if let Ok(v) = std::env::var("MAIL_HOST") {
            cfg.mail_host = v;
        }
        if let Ok(v) = std::env::var("MAIL_DOMAIN") {
            cfg.mail_domain = v;
        }
        if let Ok(v) = std::env::var("MAX_QUOTA_MB")
            && let Ok(p) = v.parse()
        {
            cfg.max_quota_mb = p;
        }
        if let Ok(v) = std::env::var("SPAM_THRESHOLD")
            && let Ok(p) = v.parse()
        {
            cfg.spam_threshold = p;
        }
        if let Ok(v) = std::env::var("LETSENCRYPT_EMAIL") {
            cfg.letsencrypt_email = v;
        }
        if let Ok(v) = std::env::var("TIMEZONE") {
            cfg.timezone = v;
        }
        if let Ok(v) = std::env::var("CONTROL_URL") {
            cfg.control_url = v;
        }
        if let Ok(v) = std::env::var("WEBMAIL_URL") {
            cfg.webmail_url = v;
        }
        if let Ok(v) = std::env::var("ADMIN_URL") {
            cfg.admin_url = v;
        }
        if let Ok(v) = std::env::var("RATE_LIMIT_PER_MIN")
            && let Ok(p) = v.parse()
        {
            cfg.rate_limit_per_min = p;
        }
        if let Ok(v) = std::env::var("HIBP_ENABLED") {
            cfg.hibp_enabled =
                v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes");
        }
        if let Ok(v) = std::env::var("TOTP_ENFORCED_FOR_ADMIN") {
            cfg.totp_enforced_for_admin =
                v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes");
        }
        if let Ok(v) = std::env::var("MIN_PASSWORD_LEN")
            && let Ok(p) = v.parse()
        {
            cfg.min_password_len = p;
        }
        if let Ok(v) = std::env::var("ENCRYPTION_KEY") {
            cfg.encryption_key = v;
        }
        if let Ok(v) = std::env::var("CORS_ORIGINS") {
            cfg.cors_origins = v.clone();
            cfg.cors_origins_list = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s != "*")
                .collect();
        }
        cfg
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.session_secret.is_empty() {
            return Err("SESSION_SECRET is required (no default secret is accepted)".to_string());
        }
        if self.session_secret.len() < 32 {
            return Err("SESSION_SECRET must be at least 32 bytes".to_string());
        }
        for placeholder in [
            "dev-secret-change-me-32bytes-min-0000",
            "CHANGE_ME_32B_MIN_0123456789ABCDEF_0123456789",
            "change-me",
            "changeme",
        ] {
            if self.session_secret.eq_ignore_ascii_case(placeholder)
                || (placeholder.contains("CHANGE_ME") && self.session_secret.contains("CHANGE_ME"))
            {
                return Err("SESSION_SECRET still set to a placeholder value; generate a unique random secret".to_string());
            }
        }
        if self.database_url.is_empty() {
            return Err("DATABASE_URL is required".to_string());
        }
        if self.encryption_key.is_empty() {
            return Err("ENCRYPTION_KEY is required (no default key is accepted)".to_string());
        }
        if self.encryption_key.len() != 64
            || !self.encryption_key.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("ENCRYPTION_KEY must be 64 hex characters (32 bytes)".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_config_requires_secret() {
        let cfg = Config::default();
        assert!(cfg.validate().is_err());
        assert_eq!(cfg.mail_host, "mail.example.com");
    }
    #[test]
    fn rejects_short_secret() {
        let cfg = Config {
            session_secret: "short".to_string(),
            ..Config::default()
        };
        assert!(cfg.validate().is_err());
    }
    #[test]
    fn accepts_valid_secret() {
        let cfg = Config {
            session_secret: "a-strong-random-secret-of-32-bytes-minimum".to_string(),
            encryption_key: "00".repeat(32),
            ..Config::default()
        };
        assert!(cfg.validate().is_ok());
    }
    #[test]
    fn rejects_placeholder_secret() {
        for placeholder in [
            "dev-secret-change-me-32bytes-min-0000",
            "CHANGE_ME_32B_MIN_0123456789ABCDEF_0123456789",
            "CHANGE_ME_00000000000000000000000000000000",
        ] {
            let cfg = Config {
                session_secret: placeholder.to_string(),
                ..Config::default()
            };
            assert!(
                cfg.validate().is_err(),
                "placeholder {placeholder} must be rejected"
            );
        }
    }
    #[test]
    fn env_roundtrip() {
        unsafe {
            std::env::set_var("SESSION_SECRET", "0123456789abcdef0123456789abcdef-cust");
            std::env::set_var("CONTROL_URL", "https://control/api");
            std::env::set_var("WEBMAIL_URL", "https://web/mail");
            std::env::set_var("ADMIN_URL", "https://admin/ui");
            std::env::set_var("RATE_LIMIT_PER_MIN", "42");
            std::env::set_var("HIBP_ENABLED", "true");
            std::env::set_var("MAIL_HOST", "mx.local");
            std::env::set_var("MAIL_DOMAIN", "local");
            std::env::remove_var("TOTP_ENFORCED_FOR_ADMIN");
            std::env::remove_var("MIN_PASSWORD_LEN");
        }
        let cfg = Config::from_env();
        assert_eq!(cfg.control_url, "https://control/api");
        assert_eq!(cfg.webmail_url, "https://web/mail");
        assert_eq!(cfg.admin_url, "https://admin/ui");
        assert_eq!(cfg.rate_limit_per_min, 42);
        assert!(cfg.hibp_enabled);
        assert_eq!(cfg.mail_host, "mx.local");
        assert_eq!(cfg.mail_domain, "local");
        assert!(cfg.totp_enforced_for_admin);
    }
}
