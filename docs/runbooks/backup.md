# Backup runbook

Daily `pg_dump -Fc` → `/backups/db-YYYYMMDD.dump` (keep 14d). Maildir tar → `/backups/maildir-YYYYMMDD.tar.gz`. DKIM/certs tar. Off-site via rclone (profile `backup`).

## Restore (dry-run first)

The restore helper never writes by default. It verifies the newest backup:

1. `sha256sum -c` against the newest `checksums-*.txt`
2. `pg_restore --list` sanity on the newest `db-*.dump`
3. `tar -tzf` listing of the newest `maildir-*.tar.gz`

Run it via the one-shot test service:

```sh
docker compose --profile backup-test run --rm backup-restore-test
# dry-run output; safe to run anytime the backups volume exists.
docker compose --profile backup-test run --rm backup-restore-test -- --execute
# also possible: RESTORE_EXECUTE=true docker compose --profile backup-test run --rm backup-restore-test
```

Directly (from the backups volume on a host with the tools):

```sh
docker/backup/restore.sh                # dry-run
docker/backup/restore.sh --execute      # actually restore
```

## Restore (real)

Stop services first, then restore database + mail, then bring up:

```sh
docker compose stop postfix dovecot synth-control synth-webmail
# Database (into a spare/empty synthmail DB):
PGPASSWORD=... pg_restore --clean --if-exists -h postgres -U synth -d synthmail /backups/db-<date>.dump
# Maildir:
tar -xzf /backups/maildir-<date>.tar.gz -C /
docker compose up -d
```

Warn: `--execute` restores **into the running prod DB** (`PGHOST`/`PGDB`).
For a rehearsal use a spare target (e.g. a scratch Postgres) and
`RESTORE_EXECUTE=true` with matching env — never against prod just to test.
