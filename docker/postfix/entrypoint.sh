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

# Plan §1 (host-network mail edge): when MAIL_EDGE=1 the postfix service is on
# `network_mode: host`. The bridge mailnet subnet (172.17.0.0/16) is no longer
# the address space of our milter/LMTP peers, so it must NOT be trusted for
# unauthenticated relay. Tighten mynetworks to loopback-only. On the default
# bridge layout this is a no-op (MAIL_EDGE unset).
if [ "${MAIL_EDGE:-}" = "1" ]; then
  postconf -e "mynetworks = 127.0.0.0/8 [::1]/128"
fi

postfix start || true
# Keep the container in the foreground; postlogd writes to /var/log/postfix.log.
tail -f /var/log/postfix.log 2>/dev/null || tail -f /dev/null &
wait
