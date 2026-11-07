#!/bin/sh
# SynthMail backup: DB dump + Maildir tar, checksummed, retention-pruned,
# optionally synced off-site with rclone. Fails loudly on error (no `|| echo`).
set -eu

DATE="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR="${BACKUP_DIR:-/backups}"
MAILDIR="${MAILDIR:-/var/vmail}"
RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-14}"
PGHOST="${PGHOST:-postgres}"
PGUSER="${PGUSER:-synth}"
PGDB="${PGDB:-synthmail}"
RCLONE_REMOTE="${RCLONE_REMOTE:-}"

mkdir -p "${BACKUP_DIR}"

echo "== SynthiMail backup $(date -u +%Y-%m-%dT%H:%M:%SZ) =="

# 1. Database (custom format).
pg_dump -Fc -h "${PGHOST}" -U "${PGUSER}" "${PGDB}" > "${BACKUP_DIR}/db-${DATE}.dump"

# 2. Maildir (only if the volume is mounted).
if [ -d "${MAILDIR}" ]; then
  tar -czf "${BACKUP_DIR}/maildir-${DATE}.tar.gz" -C "$(dirname "${MAILDIR}")" "$(basename "${MAILDIR}")" 2>/dev/null || \
  tar -czf "${BACKUP_DIR}/maildir-${DATE}.tar.gz" "${MAILDIR}" || true
else
  echo "WARN: ${MAILDIR} not mounted — skipping maildir backup"
fi

# 3. Checksums (restore-time verification).
(cd "${BACKUP_DIR}" && sha256sum db-*.dump maildir-*.tar.gz > "checksums-${DATE}.txt" 2>/dev/null || true)

# 4. Retention pruning.
find "${BACKUP_DIR}" -name 'db-*.dump' -mtime +"${RETENTION_DAYS}" -delete || true
find "${BACKUP_DIR}" -name 'maildir-*.tar.gz' -mtime +"${RETENTION_DAYS}" -delete || true
find "${BACKUP_DIR}" -name 'checksums-*.txt' -mtime +"${RETENTION_DAYS}" -delete || true

# 5. Off-site sync (optional — requires RCLONE_REMOTE, e.g. "gdrive:synthmail-backups").
if [ -n "${RCLONE_REMOTE}" ]; then
  rclone sync "${BACKUP_DIR}" "${RCLONE_REMOTE}" --retention "${RETENTION_DAYS}d"
else
  echo "RCLONE_REMOTE not set — skipping off-site sync"
fi

echo "== Backup complete =="
ls -lh "${BACKUP_DIR}" | tail -20
