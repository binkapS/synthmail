# SynthMail synth-control — multi-stage
FROM rust:bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --locked --bin synth-control

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/synth-control /usr/local/bin/synth-control
COPY --from=builder /app/crates/synth-control/migrations /app/migrations
WORKDIR /app
ENV RUST_LOG=info
EXPOSE 8000
HEALTHCHECK CMD wget -q --spider http://localhost:8000/healthz || exit 1
ENTRYPOINT ["synth-control"]
