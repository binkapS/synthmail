use std::sync::Once;

static INIT: Once = Once::new();

pub fn init_telemetry(service: &str) {
    INIT.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_target(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NONE)
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .finish();

        let _ = tracing::subscriber::set_global_default(subscriber);
        tracing::info!(service = service, "telemetry initialized");
    });
}
