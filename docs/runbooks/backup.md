# Backup runbook
Daily `pg_dump -Fc` → `/backups/db-YYYYMMDD.dump` (keep 14d). Maildir tar → `/backups/maildir-YYYYMMDD.tar.gz`. DKIM/certs tar. Off-site via rclone (profile `backup`).
Restore: stop postfix/dovecot, `pg_restore -d synthmail`, extract maildir, `docker compose up -d`.
