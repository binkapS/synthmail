FROM lukechampine.com/cargo-chef:latest-rust-1.83 AS chef
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates

FROM chef AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin synth-watchdog

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/synth-watchdog /usr/local/bin/synth-watchdog
EXPOSE 8081
HEALTHCHECK CMD wget -q --spider http://localhost:8081/healthz || exit 1
ENTRYPOINT ["synth-watchdog"]
