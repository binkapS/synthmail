# Phase 3 — Pure-Rust evaluation

- **Shim:** `crates/synth-smtp` (ESMTP + Redis queue + `lettre` forward to Postfix). `cargo run -p synth-smtp -- --port 2525`. Not an inbound MTA.
- **Bench:** `cargo run -p synth-bench -- --iterations 5000` → TOTP p50/p95, HIBP p95, queue-serialize msg/s vs 10k/h target, p95 <100ms verdict.
- **Stalwart:** `docker compose -f compose.yaml -f docker/compose.stalwart.yaml --profile stalwart up -d` (image `stalwartlabs/stalwart:v1`). Verify with mail-tester + Thunderbird + `GET /jmap/session`.
- **Decision:** see `docs/adr/0003-pure-rust-evaluation.md` (keep Hybrid default; Stalwart as v1.0 candidate).
