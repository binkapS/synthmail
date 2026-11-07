#!/bin/sh
# Postfix entrypoint (audit N-12): replace password placeholder safely.
# Uses a tiny awk helper instead of sed so passwords containing &, |, or
# slashes are substituted verbatim.
set -eu

substitute_password() {
  local file="$1"
  local pass="$2"
  local tmp
  tmp="$(mktemp)"
  PASS="$pass" awk '
    { gsub(/\$[{]POSTGRES_PASSWORD[}]/, ENVIRON["PASS"]); print }
  ' "$file" > "$tmp" && mv "$tmp" "$file"
}

for f in /etc/postfix/pgsql/*.cf; do
  substitute_password "$f" "${POSTGRES_PASSWORD:-}"
done

postfix start || true
# Keep the container in the foreground; postlogd writes to /var/log/postfix.log.
tail -f /var/log/postfix.log 2>/dev/null || tail -f /dev/null &
wait
