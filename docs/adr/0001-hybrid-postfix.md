# ADR 0001 — Hybrid Postfix/Dovecot v1

Decision: Keep Postfix + Dovecot for transport/storage in v1; Rust control plane for management.
Rationale: Deliverability + interop weighted higher than code purity. Pure-Rust MTA evaluated in Phase 3 (Stalwart / smtp-server).
Consequences: Config drift risk mitigated by templated configs generated from Postgres source of truth.
