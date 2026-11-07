#!/bin/sh
# Jails whose logpath files do not exist are fatal at fail2ban startup.
# postfix/dovecot append lazily to the shared logs volume, so pre-create.
touch /var/log/postfix.log /var/log/dovecot.log
chmod 0644 /var/log/postfix.log /var/log/dovecot.log
exec /usr/bin/fail2ban-server -f -x
