//! Password policy + breached-password enforcement (H-03).
//!
//! Minimum length is always enforced. HIBP (Have I Been Pwned) k-anonymity
//! checking is enabled when `Config::hibp_enabled` is true; the network check
//! fails open (treated as "not breached") when unreachable, per the documented
//! behavior of the underlying `hibp` module.

/// Trivially known/default passwords (C-05: default admin creds must never be
/// re-usable even if they satisfy the length rules).
const DENYLIST: &[&str] = &[
    "changeme123!admin",
    "changeme123",
    "admin",
    "admin1234",
    "password",
    "password1",
    "password123",
    "letmein",
    "qwerty123456",
    "123456789012",
];

pub fn validate_password_strength(password: &str, min_len: usize) -> Result<(), String> {
    if password.len() < min_len {
        return Err(format!(
            "password must be at least {min_len} characters"
        ));
    }
    if DENYLIST.contains(&password.to_lowercase().as_str()) {
        return Err("password is a well-known default/weak password".to_string());
    }
    if password.chars().all(|c| c.is_ascii_alphanumeric()) {
        // Encourage entropy but keep the policy simple: require one non-basic
        // character only when length is butting the minimum.
        if password.len() < min_len + 4 {
            return Err(
                "password needs at least one special or non-alphanumeric character".to_string()
            );
        }
    }
    if password.chars().next().is_some() && password.trim() != password {
        return Err("password must not contain leading/trailing whitespace".to_string());
    }
    Ok(())
}

/// Full reset/create-time check: strength first (cheap), then HIBP when enabled.
pub async fn check_password_allowed(
    password: &str,
    hibp_enabled: bool,
    min_len: usize,
) -> Result<(), String> {
    validate_password_strength(password, min_len)?;
    if hibp_enabled && crate::hibp::check_breached(password).await {
        return Err("password appears in a known breach list".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_too_short() {
        assert!(validate_password_strength("a", 12).is_err());
        assert!(validate_password_strength("password", 12).is_err());
    }

    #[test]
    fn accepts_long_enough() {
        assert!(validate_password_strength("correct-horse-battery", 12).is_ok());
        assert!(validate_password_strength("sup3r-Secret-99!", 12).is_ok());
    }

    #[test]
    fn rejects_whitespace_edges() {
        assert!(validate_password_strength("passw0rd-long  ", 12).is_err());
    }

    #[test]
    fn default_admin_password_is_rejected() {
        // Known weak defaults from C-05 must never pass policy.
        assert!(validate_password_strength("ChangeMe123!Admin", 12).is_err());
    }
}
