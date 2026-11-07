FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
# Prevent maintainer scripts from trying to start services during the image
# build (no init system exists in the build container).
# policy-rc.d exiting 101 is the documented "do not start" code.
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && chmod +x /usr/sbin/policy-rc.d
RUN apt-get update && apt-get install -y --no-install-recommends \
    dovecot-core dovecot-imapd dovecot-pop3d dovecot-lmtpd dovecot-sieve dovecot-managesieved dovecot-pgsql \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN groupadd -g 5000 vmail || true \
    && useradd -g 5000 -u 5000 -d /var/vmail -m -s /usr/sbin/nologin vmail \
    && mkdir -p /var/vmail /etc/dovecot /var/log/dovecot \
    && chown -R vmail:vmail /var/vmail
COPY dovecot.conf /etc/dovecot/dovecot.conf
COPY sql.conf /etc/dovecot/dovecot-sql.conf.ext
EXPOSE 143 993 110 995 4190
# Entry point: expand ${POSTGRES_PASSWORD} in the SQL driver config,
# enable TLS when Let's Encrypt certs exist, then start dovecot.
ENTRYPOINT ["/bin/sh", "-c", "sed -i \"s|password=[^ ]*|password=${POSTGRES_PASSWORD:-synth_dev}|\" /etc/dovecot/dovecot-sql.conf.ext; if [ -f /etc/letsencrypt/live/mail.example.com/fullchain.pem ]; then sed -i 's/^ssl = no/ssl = yes/' /etc/dovecot/dovecot.conf; sed -i '/^ssl = yes/a ssl_cert = </etc/letsencrypt/live/mail.example.com/fullchain.pem' /etc/dovecot/dovecot.conf; sed -i '/^ssl_cert =/a ssl_key = </etc/letsencrypt/live/mail.example.com/privkey.pem' /etc/dovecot/dovecot.conf; fi; mkdir -p /var/spool/postfix/private && chmod 0755 /var/spool/postfix /var/spool/postfix/private; doveadm -c /etc/dovecot/dovecot.conf stop 2>/dev/null; dovecot -c /etc/dovecot/dovecot.conf; sleep infinity"]
HEALTHCHECK CMD doveadm ping || exit 1
