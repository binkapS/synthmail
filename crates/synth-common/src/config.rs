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

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            database_url: default_database_url(),
            redis_url: default_redis_url(),
            session_secret: "dev-secret-change-me-32bytes-min-0000".to_string(),
            mail_host: "mail.example.com".to_string(),
            mail_domain: "example.com".to_string(),
            max_quota_mb: default_max_quota(),
            spam_threshold: 7,
            letsencrypt_email: "admin@example.com".to_string(),
            timezone: default_timezone(),
            control_url: "https://mail.example.com/api".to_string(),
            webmail_url: "https://mail.example.com/mail".to_string(),
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

        cfg
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.session_secret.len() < 32 {
            return Err("SESSION_SECRET must be at least 32 bytes".to_string());
        }
        if self.database_url.is_empty() {
            return Err("DATABASE_URL is required".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_config_valid() {
        let cfg = Config::default();
        assert!(cfg.validate().is_ok());
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
}
