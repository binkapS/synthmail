# SynthMail Action Plan

## Goal

Harden the existing Docker-based deployment for production mail delivery and
security validation. This plan focuses on the edge/mail path and operational
readiness gaps, not on changing the deployment model.

## 1. Preserve original SMTP client IPs

Postfix must see the real remote sender, not a Docker bridge/NAT address.

Tasks:

- Add a host-networked mail-edge profile for `postfix`, `dovecot`, `rspamd`,
  and `clamav` (or switch those services to `network_mode: host`).
- Keep shared volumes unchanged:
  - `maildir:/var/vmail`
  - `certs:/etc/letsencrypt`
  - `postfix-spool:/var/spool/postfix`
  - `dkim:/etc/opendkim/keys` or the equivalent sync volume
- Do not publish `25`, `465`, `587`, `993`, or `4190` through bridge NAT if host
  networking is enabled.
- Verify Postfix logs show the public remote IP as `client=` for inbound SMTP.
- Confirm Rspamd and Postfix DNSBL/RBL checks receive the same original IP.
- Run a controlled inbound SMTP test from an external host and save the log
  evidence in `docs/runbooks/deliverability.md` or a new validation note.

Files likely touched:

- `compose.yaml`
- `docker/postfix/main.cf`
- `docker/postfix/master.cf`
- `docker/dovecot/dovecot.conf`
- `docs/runbooks/deliverability.md`

## 2. Make fail2ban effective on the host

Container log collection alone is not enough for a public mail host.

Tasks:

- Decide whether `fail2ban` runs in a container with host network privileges or
  as a host-native service.
- If containerized:
  - mount `/var/log` read-only
  - grant only the minimum firewall capability needed
  - document how bans apply to host-level SMTP/IMAP traffic
- If host-native:
  - point jail paths at the same log location used by Postfix/Dovecot
  - keep the existing `docker/fail2ban/jail.local` and filter definitions as
    source input
- Test bans with repeated failed SMTP AUTH and IMAP LOGIN attempts.
- Verify banned IPs are blocked at the host firewall, not only inside a
  container namespace.
- Add rollback steps to unban and verify recovery.

Files likely touched:

- `compose.yaml`
- `docker/fail2ban/jail.local`
- `docker/fail2ban/filter.d/synth-webmail.conf`
- `docker/fail2ban/entrypoint.sh`
- `docs/runbooks/security.md`

## 3. Reduce Docker socket exposure

Compose currently mounts `/var/run/docker.sock` into `synth-control` for container
stats.

Tasks:

- Mark Docker socket access as optional, not mandatory.
- Disable or gate the container-stats feature unless explicitly needed.
- If retained, document that it is read-only local access and should not be
  enabled on untrusted hosts.
- If removed entirely, replace the server-panel container view with host-level
  process and resource metrics.
- Update UI messaging so the admin panel does not imply missing monitoring when
  socket access is intentionally disabled.

Files likely touched:

- `compose.yaml`
- `crates/synth-control/src/routes.rs`
- `ui/src/lib/components/admin/ServerPanel.svelte`
- `docs/runbooks/monitoring.md`

## 4. Finish TLS and public-edge validation

The stack is fail-closed without certificates.

Tasks:

- Run ACME in a non-staging mode after testing staging.
- Install the certificate for `MAIL_HOST`.
- Verify Postfix, Dovecot, and nginx load the same certificate bundle.
- Test:
  - SMTP TLS on 25/465/587
  - IMAPS on 993
  - HTTPS admin/webmail
  - HTTP-01 challenge renewal
- Document expiry/renewal ownership and calendar reminders.
- Add a backup/restore check for certificates before production cutover.

Files likely touched:

- `.env`
- `compose.yaml`
- `docker/edge/nginx.conf`
- `docs/runbooks/security.md`
- `docs/runbooks/backup.md`

## 5. Validate backups and restore procedures

Current backup tooling exists but must be proven on the target host.

Tasks:

- Run `docker compose --profile backup run --rm backup`.
- Verify `pg_dump`, Maildir tar, checksums, retention pruning, and optional rclone.
- Run `backup-restore-test` as a dry run.
- Perform one actual restore into a spare target or temporary database.
- Validate Maildir ownership and permissions after restore.
- Confirm restore time and write it into the runbook.

Files likely touched:

- `compose.yaml`
- `docker/backup/run-backup.sh`
- `docker/backup/restore.sh`
- `docs/runbooks/backup.md`

## 6. Production readiness rehearsal on a real host

The project is still pending Docker-host validation.

Tasks:

- Run `docker compose config` and fix all warnings.
- Bring up every default service.
- Confirm healthchecks pass for:
  - `edge`
  - `postfix`
  - `dovecot`
  - `rspamd`
  - `clamav`
  - `postgres`
  - `redis`
  - `synth-control`
  - `synth-webmail`
  - `synth-watchdog`
  - `synth-ui`
  - `fail2ban`
- Test full mail flow:
  - inbound Internet mail to local mailbox
  - IMAP read
  - webmail read/reply
  - outbound mail to external mailbox
  - attachment scanning
  - spam classification
- Test admin workflows:
  - user/domain creation
  - DKIM generation
  - queue inspection
  - DMARC report ingestion
- Record findings and corrections in `docs/history/` or a new validation report.

Files likely touched:

- `docs/runbooks/monitoring.md`
- `docs/runbooks/deliverability.md`
- `docs/runbooks/upgrades.md`
- `docs/history/`

## 7. HA overlay rehearsal

The HA compose file is validated by config only.

Tasks:

- Bring up single-node Patroni with `docker compose -f compose.yaml -f docker/compose.ha.yaml --profile ha up`.
- Verify Patroni leader health.
- Test manual failover to replica on a spare host.
- Validate application reconnection after failover.
- Document downtime observed during promotion.
- Decide whether HA is in scope for v1 or should remain optional.

Files likely touched:

- `docker/compose.ha.yaml`
- `docs/runbooks/ha.md`

## 8. Edge operational updates

Tasks:

- Keep Postfix/Dovecot/Rspamd image versions pinned wherever possible.
- Add explicit upgrade and rollback steps to `docs/runbooks/upgrades.md`.
- Confirm service update order:
  1. Postgres/Redis
  2. control/webmail/watchdog
  3. mail edge
  4. UI/edge proxy
- Take a pre-upgrade backup before applying updates.
- Test at least one full upgrade cycle.

Files likely touched:

- `compose.yaml`
- `docs/runbooks/upgrades.md`
- `docs/releases/`

## 9. Monitoring and alerting

Tasks:

- Configure Prometheus to scrape SynthMail metrics.
- Import or validate the Grafana dashboard in `docker/monitoring`.
- Set `ALERT_WEBHOOK_URL` or `NTFY_TOPIC`.
- Test watchdog alerts for service failure.
- Test queue-depth alerting.
- Document whichalerts are meaningful versus noisy.

Files likely touched:

- `docker/monitoring/prometheus.yml`
- `docker/monitoring/grafana-dashboards/synthmail-overview.json`
- `docs/runbooks/monitoring.md`

## 10. Final go-live checklist

- `MAIL_HOST` and DNS records correct
- PTR/rDNS configured
- SPF, DKIM, and DMARC published
- production TLS certificate installed
- host firewall permits only required ports
- backup job tested and documented
- fail2ban tested
- monitoring and alerts tested
- restore dry run successful
- upgrade rollback tested
- final `docker compose config` passes cleanly
- release notes updated

## Suggested priority order

1. SMTP client IP preservation
2. fail2ban host enforcement
3. TLS validation
4. production host rehearsal
5. backup/restore test
6. HA overlay rehearsal
7. monitoring tuning
8. optional Docker socket removal/replacement

## Success criteria

- Inbound SMTP logs show real client IPs.
- Public SMTP/IMAP/TLS endpoints pass external tests.
- Spam, antivirus, quota, and delivery flows work end to end.
- Backup and restore are proven.
- fail2ban blocks real failed-authentication attempts.
- Operators can upgrade and roll back using documented steps.
- Monitoring alerts fire and deduplicate correctly.