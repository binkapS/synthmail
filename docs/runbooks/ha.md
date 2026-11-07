# HA runbook (stretch goal)

```bash
docker compose -f compose.yaml -f docker/compose.ha.yaml --profile ha up -d
# Postgres primary (postgres) + replica (postgres-replica, streaming via wal_level=replica)
# Postfix primary (postfix :25/:587) + secondary (postfix-2, shared maildir volume)
```

- **Promote replica**: `pg_ctl -D ... promote` on replica, repoint `DATABASE_URL` in `.env`, `docker compose restart synth-control synth-webmail`.
- **Postfix multi-node**: front with `edge` nginx upstream (round-robin), shared `maildir` volume, `postfix-spool-2` for queue (queues not replicated — use LMTP replay on failover).
- **Dovecot**: keep single-node behind LB; Dovecot has no native replication (use `dsync` for future).
- **Alternative**: Stalwart cluster (see `docs/adr/0003-pure-rust-evaluation.md`) — NATS/Redis coordination, RocksDB/Postgres shared store.
