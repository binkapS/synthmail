#!/bin/sh
# Dovecot entrypoint (audit N-12 + C-06):
# - TLS is fail-closed: Dovecot refuses to start without a cert.
# - password substitution uses awk (safe for &, |, / in passwords),
#   not sed with a | delimiter.
set -eu

substitute_password() {
  local file="$1"
  local pass="$2"
  local tmp
  tmp="$(mktemp)"
  PASS="$pass" awk '
    { sub(/password=[^ ]*/, "password=" ENVIRON["PASS"]); print }
  ' "$file" > "$tmp" && mv "$tmp" "$file"
}

CERT="/etc/letsencrypt/live/mail.example.com/fullchain.pem"
if [ ! -f "$CERT" ]; then
  echo "ERROR: TLS certificate not found at ${CERT}." >&2
  echo "Provision certs first (compose --profile acme, or copy manually), then restart." >&2
  exit 1
fi

# dovecot.conf is fail-closed on its own (ssl = yes + required); the cert
# and key are already referenced there, so nothing to expand at runtime.
substitute_password /etc/dovecot/dovecot-sql.conf.ext "${POSTGRES_PASSWORD:-}"

mkdir -p /var/spool/postfix/private
chmod 0755 /var/spool/postfix /var/spool/postfix/private

doveadm -c /etc/dovecot/dovecot.conf stop 2>/dev/null || true
# -n: stay in the foreground (dovecot daemonizes by default, which would
# make PID 1 exit 0 and the container restart-loop forever).
exec dovecot -n -c /etc/dovecot/dovecot.conf
