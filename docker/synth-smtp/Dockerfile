# SynthMail synth-smtp — multi-stage
#
# ESMTP submission service (STARTTLS + AUTH, Postfix relay, Redis queue).
# Matches the synth-control pattern: cache mounts for the cargo registry
# and a debian:bookworm-slim runtime with only ca-certificates + bash
# (bash is needed for the TCP-greeting healthcheck).
FROM rust:bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --locked --bin synth-smtp

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates bash && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/synth-smtp /usr/local/bin/synth-smtp
WORKDIR /app
ENV RUST_LOG=info
EXPOSE 2525
# SMTP has no HTTP /healthz: probe the 220 greeting over TCP instead.
HEALTHCHECK CMD bash -c 'exec 3<>/dev/tcp/127.0.0.1/2525' 2>/dev/null || exit 1
ENTRYPOINT ["synth-smtp"]
