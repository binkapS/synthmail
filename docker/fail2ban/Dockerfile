FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
# policy-rc.d exiting 101: package postinst must not try to start the daemon
# during build - there is no init system in the container.
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && chmod +x /usr/sbin/policy-rc.d
# Plan §2: host firewall enforcement. fail2ban's ban action (nft /
# iptables-multiport) needs to reach the host firewall. The container runs
# with `network_mode: host` (see compose.yaml) and the CAP_NET_ADMIN
# capability so its ban action mutates the host's nftables/iptables rules
# (not a container netns). nftables is the modern default; iptables is kept
# for hosts where nftables is unavailable.
RUN apt-get update && apt-get install -y --no-install-recommends \
    fail2ban \
    nftables \
    iptables \
    iproute2 \
    && rm -rf /var/lib/apt/lists/*
# Package-installed /etc/fail2ban keeps its fail2ban.conf, filter.d/ and
# action.d/; our jail.local only toggles jails and overrides bantimes.
COPY jail.local /etc/fail2ban/jail.local
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
HEALTHCHECK CMD fail2ban-client status || exit 1
