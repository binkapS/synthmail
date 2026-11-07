#!/bin/sh
# SynthMail backup RESTORE (dry-run by default).
#
# Verifies the latest successful backup:
#   1. sha256sum checksums (if a checksums-*.txt exists)
#   2. pg_restore --list on the newest db-*.dump (sanity: no corruption)
#   3. tar -tzf listing of the newest maildir-*.tar.gz (sanity: readable)
#
# Nothing is restored unless RESTORE_EXECUTE=true (or --execute):
#   - pg_restore into ${PGHOST}/${PGDB} as ${PGUSER} (current prod DB!)
#   - tar extraction into $RESTORE_DIR (default /restore)
# A real restore also needs Postfix/Dovecot stopped — see
# docs/runbooks/backup.md. This script never stops services.
set -eu

EXECUTE=false
case "${1:-}" in
  --execute) EXECUTE=true ;;
  ""|--dry-run) ;;
  *) echo "usage: restore.sh [--dry-run|--execute]"; exit 2 ;;
esac
if [ "${RESTORE_EXECUTE:-false}" = "true" ]; then EXECUTE=true; fi

BACKUP_DIR="${BACKUP_DIR:-/backups}"
RESTORE_DIR="${RESTORE_DIR:-/restore}"
PGHOST="${PGHOST:-postgres}"
PGUSER="${PGUSER:-synth}"
PGDB="${PGDB:-synthmail}"
PGPASSWORD="${PGPASSWORD:-}"

if [ "$EXECUTE" = true ]; then
  echo "!! EXECUTE mode: pg_restore into ${PGDB}@${PGHOST} and tar into ${RESTORE_DIR}"
else
  echo "== Restore DRY-RUN (no writes) =="
fi

# --- 1. checksums -----------------------------------------------------------
# The backup writes one checksums-<date>.txt per run; verify the newest.
LATEST_SUM=$(ls -1t "${BACKUP_DIR}"/checksums-*.txt 2>/dev/null | head -1 || true)
if [ -n "${LATEST_SUM}" ]; then
  echo "-- checksums: verifying ${LATEST_SUM} --"
  (cd "${BACKUP_DIR}" && sha256sum -c "$(basename "${LATEST_SUM}")")
  echo "checksums OK"
else
  echo "WARN: no checksums-*.txt found — skipping checksum verification"
fi

# --- 2. pg_restore --list sanity -------------------------------------------
LATEST_DUMP=$(ls -1t "${BACKUP_DIR}"/db-*.dump 2>/dev/null | head -1 || true)
if [ -z "${LATEST_DUMP}" ]; then
  echo "ERROR: no db-*.dump found in ${BACKUP_DIR}" >&2
  exit 1
fi
echo "-- pg_restore --list on ${LATEST_DUMP} --"
pg_restore --list "${LATEST_DUMP}" | head -50
echo "pg_restore --list OK"

# --- 3. maildir tar listing --------------------------------------------------
LATEST_MAIL=$(ls -1t "${BACKUP_DIR}"/maildir-*.tar.gz 2>/dev/null | head -1 || true)
if [ -n "${LATEST_MAIL}" ]; then
  echo "-- tar listing of ${LATEST_MAIL} (first entries) --"
  tar -tzf "${LATEST_MAIL}" | head -20
  echo "tar listing OK"
else
  echo "WARN: no maildir-*.tar.gz found — skipping maildir validation"
fi

# --- execute ---------------------------------------------------------------
if [ "$EXECUTE" = true ]; then
  mkdir -p "${RESTORE_DIR}"
  echo "-- restoring ${LATEST_DUMP} into ${PGDB}@${PGHOST} --"
  PGPASSWORD="${PGPASSWORD}" pg_restore --clean --if-exists -h "${PGHOST}" -U "${PGUSER}" -d "${PGDB}" "${LATEST_DUMP}"
  if [ -n "${LATEST_MAIL}" ]; then
    echo "-- extracting ${LATEST_MAIL} into ${RESTORE_DIR} --"
    tar -xzf "${LATEST_MAIL}" -C "${RESTORE_DIR}"
  fi
  echo "== Restore complete (stop postfix/dovecot around it; see runbook) =="
else
  echo "== DRY-RUN complete. Set RESTORE_EXECUTE=true or pass --execute to actually restore. =="
fi
