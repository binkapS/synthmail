#!/bin/sh
# fail2ban entrypoint.
#
# Plan §2 (host enforcement):
#   Jails whose logpath files do not exist are fatal at fail2ban startup.
#   postfix/dovecot append lazily to the shared `logs` volume, so pre-create
#   the two files we tail (postfix.log + dovecot.log).
#   When a host-native /var/log is mounted read-only at /host-var-log
#   instead, the jail.local logpath is adjusted to that path (operator change).
#
#   The ban action (nft / iptables-multiport) runs against the host firewall
#   because this container is on `network_mode: host` (see compose.yaml).
set -eu

mkdir -p /var/log
touch /var/log/postfix.log /var/log/dovecot.log
chmod 0644 /var/log/postfix.log /var/log/dovecot.log

# If an operator mounted host /var/log read-only for host-native enforcement,
# point the jails at those files instead (handled in a jail.local override;
# pre-create here is best-effort since the mount is read-only).
# (no extra work; logpath resolution is in jail.local)

exec /usr/bin/fail2ban-server -f -x
