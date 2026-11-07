FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
# The opendkim postinst (dh_installinit) runs
# `invoke-rc.d --skip-systemd-native opendkim start || exit 1`, which fails
# the build when there is no init system to start the service with.
# policy-rc.d exiting 101 is the documented "do not start" code; it makes
# invoke-rc.d succeed without starting anything at build time.
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && chmod +x /usr/sbin/policy-rc.d
RUN apt-get update && apt-get install -y --no-install-recommends \
    opendkim opendkim-tools ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY opendkim.conf /etc/opendkim.conf
RUN mkdir -p /etc/opendkim/keys /run/opendkim \
    && touch /etc/opendkim/KeyTable /etc/opendkim/SigningTable \
    && chown -R opendkim:opendkim /etc/opendkim /run/opendkim
EXPOSE 8892
ENTRYPOINT ["/usr/sbin/opendkim", "-f", "-c", "/etc/opendkim.conf"]
HEALTHCHECK CMD opendkim -V || exit 1
