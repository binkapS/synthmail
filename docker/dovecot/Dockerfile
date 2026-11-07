FROM debian:bookworm-slim
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
ENTRYPOINT ["/bin/sh", "-c", "doveadm start; dovecot; tail -f /var/log/dovecot.log 2>/dev/null || sleep infinity"]
HEALTHCHECK CMD doveadm ping || exit 1
