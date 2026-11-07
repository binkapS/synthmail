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
# Postfix pgsql maps (audit H-06). The entrypoint substitutes the password
# with a portable awk helper (audit N-12): the old `sed s|...|...|` broke
# on passwords containing & or the delimiter.
COPY pgsql /etc/postfix/pgsql
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod 600 /etc/postfix/pgsql/*.cf && chown postfix:postfix /etc/postfix/pgsql/*.cf \
    && chmod +x /usr/local/bin/entrypoint.sh \
    && postconf -e "myhostname = mail.example.com" || true
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
EXPOSE 25 465 587
HEALTHCHECK CMD postfix status || exit 1
