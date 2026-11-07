FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
# Prevent maintainer scripts from trying to start services during the image
# build (no init system exists in the build container).
# policy-rc.d exiting 101 is the documented "do not start" code.
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && chmod +x /usr/sbin/policy-rc.d
RUN apt-get update && apt-get install -y --no-install-recommends \
    postfix postfix-pgsql supervisor ca-certificates procps \
    iproute2 iputils-ping \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /var/spool/postfix /var/spool/postfix/deferred /etc/postfix \
    && chown postfix:postdrop /var/spool/postfix \
    && chmod 2775 /var/spool/postfix
COPY main.cf /etc/postfix/main.cf
COPY master.cf /etc/postfix/master.cf
# Postfix pgsql maps (audit H-06). Entrypoint substitutes POSTGRES_PASSWORD
# into the files below; contents are checked in with a ${POSTGRES_PASSWORD}
# placeholder.
COPY pgsql /etc/postfix/pgsql
RUN chmod 600 /etc/postfix/pgsql/*.cf && chown postfix:postfix /etc/postfix/pgsql/*.cf
RUN postconf -e "myhostname = mail.example.com" || true
# entrypoint renders cert paths, substitutes the pgsql password, then starts postfix
ENTRYPOINT ["/bin/sh", "-c", "sed -i \"s/\\$\\{POSTGRES_PASSWORD\\}/${POSTGRES_PASSWORD:-}/g\" /etc/postfix/pgsql/*.cf; postfix start; tail -f /var/log/mail.log || sleep infinity"]
EXPOSE 25 465 587
HEALTHCHECK CMD postfix status || exit 1
