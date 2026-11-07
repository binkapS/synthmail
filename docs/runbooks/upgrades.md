# Upgrades (Phase 4)

```bash
# One-click upgrade (pins honored in compose.yaml)
sudo docker compose pull
sudo docker compose up -d --remove-orphans
sudo docker compose ps
sudo docker compose logs -f synth-control | tail -n 50
```

- Migrations auto-run on `synth-control` boot (idempotent `sqlx::migrate!`).
- Reproducible builds: `docker buildx build --platform linux/amd64,arm64` (see CI).
- Rollback: `sudo docker compose up -d --force-recreate && sudo docker compose down -v` is destructive — use `pg_dump` backup first (see `backup.md`).
- HA (stretch): Postgres primary/replica + Postfix multi-node — see `docs/adr/0003-pure-rust-evaluation.md` for the Stalwart cluster alternative.
