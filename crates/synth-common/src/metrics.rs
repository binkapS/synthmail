use std::sync::atomic::{AtomicU64, Ordering};

static AUTH_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static AUTH_FAILURES: AtomicU64 = AtomicU64::new(0);
static API_REQUESTS: AtomicU64 = AtomicU64::new(0);
static MAIL_SENT: AtomicU64 = AtomicU64::new(0);
static IMAP_FETCHES: AtomicU64 = AtomicU64::new(0);

pub fn inc_auth_attempt() {
    AUTH_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}
pub fn inc_auth_failure() {
    AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
}
pub fn inc_api_request() {
    API_REQUESTS.fetch_add(1, Ordering::Relaxed);
}
pub fn inc_mail_sent() {
    MAIL_SENT.fetch_add(1, Ordering::Relaxed);
}
pub fn inc_imap_fetch() {
    IMAP_FETCHES.fetch_add(1, Ordering::Relaxed);
}

pub fn render_prometheus(service: &str) -> String {
    format!(
        "# HELP {service}_auth_attempts_total Auth attempts
# TYPE {service}_auth_attempts_total counter
{service}_auth_attempts_total {}
# HELP {service}_auth_failures_total Auth failures
# TYPE {service}_auth_failures_total counter
{service}_auth_failures_total {}
# HELP {service}_api_requests_total API requests
# TYPE {service}_api_requests_total counter
{service}_api_requests_total {}
# HELP {service}_mail_sent_total Mail sent
# TYPE {service}_mail_sent_total counter
{service}_mail_sent_total {}
# HELP {service}_imap_fetches_total IMAP fetches
# TYPE {service}_imap_fetches_total counter
{service}_imap_fetches_total {}
",
        AUTH_ATTEMPTS.load(Ordering::Relaxed),
        AUTH_FAILURES.load(Ordering::Relaxed),
        API_REQUESTS.load(Ordering::Relaxed),
        MAIL_SENT.load(Ordering::Relaxed),
        IMAP_FETCHES.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prometheus_renders() {
        let s = render_prometheus("synth_control");
        assert!(s.contains("synth_control_auth_attempts_total"));
        assert!(s.contains("# HELP"));
    }
}
