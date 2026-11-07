FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    opendmarc ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY opendmarc.conf /etc/opendmarc.conf
RUN mkdir -p /run/opendmarc \
    && chown opendmarc:opendmarc /run/opendmarc 2>/dev/null || chown -R opendmarc /run/opendmarc || true
EXPOSE 8893
ENTRYPOINT ["/usr/sbin/opendmarc", "-f", "-c", "/etc/opendmarc.conf"]
HEALTHCHECK CMD opendmarc-check || exit 1
