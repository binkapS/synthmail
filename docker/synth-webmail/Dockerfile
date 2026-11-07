FROM rust:bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo build --release --bin synth-webmail

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/synth-webmail /usr/local/bin/synth-webmail
WORKDIR /app
EXPOSE 3000
HEALTHCHECK CMD wget -q --spider http://localhost:3000/healthz || exit 1
ENTRYPOINT ["synth-webmail"]
