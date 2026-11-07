FROM debian:bookworm-slim
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
