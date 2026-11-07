# Monitoring runbook — Phase 2

## Prometheus
```bash
docker compose --profile monitoring up -d prometheus grafana
# scrape targets: synth-control:8000/metrics, synth-webmail:3000/metrics, synth-watchdog:8081/metrics
# Prometheus config: docker/monitoring/prometheus.yml
# Metrics: synth_*_auth_attempts_total, auth_failures_total, api_requests_total, mail_sent_total, imap_fetches_total
```

## Grafana
- Dashboard: `docker/monitoring/grafana-dashboards/synthmail-overview.json`
- Provisioning: `docker/monitoring/grafana-provisioning/datasources.yml`
- http://localhost:3001 (admin/admin)

## Health & alerts
- Watchdog polls postgres/redis/postfix/dovecot/rspamd/clamav every 60s; >2m down → webhook (`ALERT_WEBHOOK_URL`) or ntfy (`NTFY_TOPIC`).
- Status page: `GET http://synth-watchdog:8081/status`
- Postfix queue gauge: `postfix_queue_size` via qshape path (`POSTFIX_QSHAPE_PATH`) — `/api/v1/queues`.

## Logs
- JSON lines via tracing to stdout → `docker compose logs -f synth-control synth-webmail synth-watchdog`
