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
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
EXPOSE 143 993 110 995 4190
# Entry point: fail-closed TLS + safe password substitution (audit N-12, C-06).
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
HEALTHCHECK CMD doveadm ping || exit 1
