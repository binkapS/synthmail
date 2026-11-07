# SynthMail synth-control — multi-stage with cargo-chef
FROM lukechampine.com/cargo-chef:latest-rust-1.83 AS chef
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY --from=chef /usr/local/cargo /usr/local/cargo

# cargo-chef prepare (separate layer for deps)
RUN cargo chef prepare --bin synth-control

FROM chef AS builder
WORKDIR /app
COPY --from=chef /usr/local/cargo /usr/local/cargo
COPY --from=chef /app /app
COPY . .
RUN cargo chef cook --bin synth-control
RUN cargo build --release --bin synth-control

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/synth-control /usr/local/bin/synth-control
COPY --from=builder /app/crates/synth-control/migrations /app/migrations
WORKDIR /app
ENV RUST_LOG=info
EXPOSE 8000
HEALTHCHECK CMD wget -q --spider http://localhost:8000/healthz || exit 1
ENTRYPOINT ["synth-control"]
