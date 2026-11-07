# HA runbook — REMOVED (audit M-17)

The `docker/compose.ha.yaml` overlay was deleted in the re-audit hardening
pass (2026-11-07): it did not form a working streaming replica
(`postgres-replica` never ran `pg_basebackup` and the `primary_conninfo`
string was never connected to), and it duplicated Postfix ports without a
load balancer. Keeping a non-functional HA story is worse than shipping
none, so this runbook is retired pending a real implementation.

Alternatives today:

- **Multi-instance at the container level** (no HA guarantee): scale
  `synth-control`/`synth-webmail` replicas — they are stateless except for
  Postgres/Redis; the rate limiter is now Redis-shared and both respect
  `token_version`.
- **Stalwart cluster** (`docker/compose.stalwart.yaml`, standalone only —
  see `docs/adr/0003-pure-rust-evaluation.md`): one profile-bound stack,
  not a cluster; do not run it alongside the base Postfix/Dovecot stack.
- **Future work**: Patroni/`repmgr` for Postgres + a real PostgreSQL
  replication slot, plus a `dsync`-based Dovecot story. Until then, rely
  on the `backup` profile plus documented restore.
