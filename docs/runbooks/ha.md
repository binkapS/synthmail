# HA runbook — Patroni Postgres overlay (manual failover only)

**This is not auto-failover.** The HA overlay (`docker/compose.ha.yaml`)
adds a streaming replica and Patroni control plane, but promotion is a
manual operator action. There is no floating IP/HAProxy, no automatic
failover, and etcd runs single-node. Until those exist, treat this as
"faster manual recovery", not "high availability".

## What the overlay does

- `postgres` (base service, redefined in the overlay) becomes the
  **Patroni primary**. `DATABASE_URL` for synth-control/webmail still
  points at `postgres:5432` — no app changes.
- `etcd` is the Patroni DCS (single-node, loopback, network-internal).
- `postgres-replica` is a streaming replica (profile `ha-replica`).
- Images pinned by verified version tags: `ongres/patroni:v4.0.4-pg16.6-build-6.39`
  (upstream Patroni OCI image; `zalando/patroni` returns "object not found"
  on Hub, `patroni/patroni` is not a Hub repo) and
  `bitnamilegacy/etcd:3.6.4` (frozen bitnami mirror).
- All HA services sit behind profiles: nothing changes unless you ask
  for `ha`. The base `compose.yaml` is untouched.

## Bring-up

Single-node patroni (etcd + primary; adequate fallback if you don't
need the replica yet):

```sh
docker compose -f compose.yaml -f docker/compose.ha.yaml --profile ha up -d
docker compose -f compose.yaml -f docker/compose.ha.yaml --profile ha ps
# primary healthy when its /health reports `leader`
```

Add the replica (both profiles):

```sh
docker compose -f compose.yaml -f docker/compose.ha.yaml --profile ha --profile ha-replica up -d
docker compose -f compose.yaml -f docker/compose.ha.yaml exec postgres-replica patronictl leader
# `postgres-primary` should be leader; replica in `streaming`
```

Verify replication lag with `pg_stat_replication` on the primary:
`postgres` → `SELECT client_addr, state, write_lag, flush_lag FROM pg_stat_replication;`

Required envs (same dummies in config validation): `POSTGRES_PASSWORD`,
`PATRONI_REPLICATION_PASSWORD`, plus `REDIS_PASSWORD`,
`GRAFANA_ADMIN_PASSWORD`, `SESSION_SECRET` for the base file.

## Primary/replica expectations

- The **replica never serves app traffic**. `DATABASE_URL` always
  points at the primary. The replica exists for (a) manual promotion and
  (b) a read-only target for backup/inspection.
- Failover does **not** move any IP: after promotion, apps must be
  restarted so they resolve the new leader (see below).
- `synth-smtp`'s Redis queue and `maildir` are not HA in this overlay —
  Postgres HA does not imply mail HA.

## Manual failover steps

1. Confirm the primary is actually down (don't fail over a healthy
   primary — split-brain with a single-node etcd is still possible).
2. Promote the replica:
   `docker compose -f compose.yaml -f docker/compose.ha.yaml exec postgres-replica patronictl failover --candidate postgres-replica`
   (if the dead primary is restarted later, treat it as a stale node —
   reinit it per step 4 rather than letting it rejoin as leader)
3. Restart app services so their connections land on the new leader:
   `docker compose restart synth-control synth-webmail synth-watchdog`
   (Postgres is `postgres` on the `mailnet` network; the former replica
   now answers as leader there — restart is what re-points pooled
   connections.)
4. Rebuild the old primary as a replica of the new leader:
   `docker compose exec postgres patronictl reinit --force`
   (this discards the old primary's data dir and re-clones from the
   new leader — **this destroys the old primary's data**; only do it
   once the new leader is confirmed healthy.)
5. Re-verify: `patronictl leader` everywhere + `pg_stat_replication`.

## Limits (read before relying on this)

- **No automatic failover.** Downtime = time-to-notice + time-to-promote.
- **Single-node etcd.** If etcd dies you cannot fail over; if the host dies
  you have no DCS at all. Three-node etcd on separate hosts is the real
  production shape — not present here.
- **No read/write split or connection proxy.** No HAProxy/PgBouncer; the
  replica gets no app reads.
- **Not yet exercised on a Docker host.** Bring-up was validated with
  `docker compose config` only (no daemon here). First real deployment
  is a rehearsal: practice §Bring-up and §Manual failover on a spare host.
- **Volumes are not shared.** Primary and replica use distinct volumes
  (`postgres`, `postgres-replica`); a container recreate does not
  mirror data — replication is via streaming, which is correct, but plan
  disk sizing accordingly.
