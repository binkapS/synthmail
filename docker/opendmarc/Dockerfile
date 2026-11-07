FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
# The opendmarc postinst (dh_installinit) runs
# `invoke-rc.d --skip-systemd-native opendmarc start || exit 1`, which fails
# the build when there is no init system to start the service with.
# policy-rc.d exiting 101 is the documented "do not start" code; it makes
# invoke-rc.d succeed without starting anything at build time.
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && chmod +x /usr/sbin/policy-rc.d
RUN apt-get update && apt-get install -y --no-install-recommends \
    opendmarc dbconfig-no-thanks ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Note: dbconfig-no-thanks satisfies opendmarc's
# `dbconfig-mysql | dbconfig-no-thanks` alternative dependency with the
# official no-op stub, so the postinst's dbc_go skips database setup
# entirely instead of trying to provision a MySQL backend that does not
# exist in this image. Per dbconfig-no-thanks docs, the milter still
# works; only automatic DB-backed aggregate-report storage is off.
COPY opendmarc.conf /etc/opendmarc.conf
RUN mkdir -p /run/opendmarc \
    && chown opendmarc:opendmarc /run/opendmarc
EXPOSE 8893
ENTRYPOINT ["/usr/sbin/opendmarc", "-f", "-c", "/etc/opendmarc.conf"]
HEALTHCHECK CMD opendmarc-check || exit 1
