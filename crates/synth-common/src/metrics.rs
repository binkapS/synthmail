use std::sync::atomic::{AtomicU64, Ordering};

static AUTH_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static AUTH_FAILURES: AtomicU64 = AtomicU64::new(0);
static API_REQUESTS: AtomicU64 = AtomicU64::new(0);
static MAIL_SENT: AtomicU64 = AtomicU64::new(0);
static IMAP_FETCHES: AtomicU64 = AtomicU64::new(0);
static QUEUE_DEPTH: AtomicU64 = AtomicU64::new(0);
/// Cumulative histogram of request latencies (seconds). Buckets: 0.005..5s.
static LATENCY_BUCKETS: [AtomicU64; 10] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
static LATENCY_COUNT: AtomicU64 = AtomicU64::new(0);
static LATENCY_SUM: AtomicU64 = AtomicU64::new(0); // nanoseconds
const LATENCY_LE: [f64; 10] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0];

pub fn inc_auth_attempt() {
    AUTH_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}
pub fn inc_auth_failure() {
    AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
}
pub fn inc_api_request() {
    API_REQUESTS.fetch_add(1, Ordering::Relaxed);
}
/// Record one request latency (audit M-20 latency histogram).
pub fn observe_request_latency(secs: f64) {
    LATENCY_COUNT.fetch_add(1, Ordering::Relaxed);
    LATENCY_SUM.fetch_add((secs * 1e9) as u64, Ordering::Relaxed);
    for (i, le) in LATENCY_LE.iter().enumerate() {
        if secs <= *le {
            LATENCY_BUCKETS[i].fetch_add(1, Ordering::Relaxed);
            return;
        }
    }
    LATENCY_BUCKETS[9].fetch_add(1, Ordering::Relaxed); // >5s lands in top bucket
}
pub fn inc_mail_sent() {
    MAIL_SENT.fetch_add(1, Ordering::Relaxed);
}
pub fn inc_imap_fetch() {
    IMAP_FETCHES.fetch_add(1, Ordering::Relaxed);
}
/// Set the current deferred-queue depth gauge (audit M-20).
pub fn set_queue_depth(depth: u64) {
    QUEUE_DEPTH.store(depth, Ordering::Relaxed);
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
# HELP {service}_queue_depth Deferred queue depth (gauge)
# TYPE {service}_queue_depth gauge
{service}_queue_depth {}
",
        AUTH_ATTEMPTS.load(Ordering::Relaxed),
        AUTH_FAILURES.load(Ordering::Relaxed),
        API_REQUESTS.load(Ordering::Relaxed),
        MAIL_SENT.load(Ordering::Relaxed),
        IMAP_FETCHES.load(Ordering::Relaxed),
        QUEUE_DEPTH.load(Ordering::Relaxed),
    ) + &latency_histogram(service)
}

fn latency_histogram(service: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# HELP {service}_http_request_latency_seconds HTTP request latency\n# TYPE {service}_http_request_latency_seconds histogram\n"
    ));
    let count = LATENCY_COUNT.load(Ordering::Relaxed);
    let mut cumulative = 0u64;
    for (i, le) in LATENCY_LE.iter().enumerate() {
        cumulative += LATENCY_BUCKETS[i].load(Ordering::Relaxed);
        out.push_str(&format!(
            "{service}_http_request_latency_seconds_bucket{{le=\"{}\"}} {cumulative}\n",
            le,
        ));
    }
    out.push_str(&format!(
        "{service}_http_request_latency_seconds_bucket{{le=\"+Inf\"}} {count}\n"
    ));
    out.push_str(&format!(
        "{service}_http_request_latency_seconds_sum {}\n",
        LATENCY_SUM.load(Ordering::Relaxed) as f64 / 1e9
    ));
    out.push_str(&format!(
        "{service}_http_request_latency_seconds_count {count}\n"
    ));
    out
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
    #[test]
    fn latency_histogram_updates() {
        observe_request_latency(0.01);
        observe_request_latency(3.0);
        let s = latency_histogram("svc");
        assert!(s.contains("le=\"0.005\""));
        assert!(s.contains("svc_http_request_latency_seconds_count"));
    }
}
