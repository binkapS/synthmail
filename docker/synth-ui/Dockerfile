# SynthMail — SynthiUI (Leptos SPA) build + static serve.
# Audit H-11: previously the UI was never built or served.
# NOTE: there is no official Trunk Docker image (trunkwasm/trunk:latest does
# not resolve); build trunk from crates.io in the rust builder instead.

# --- Stage 1: Trunk build (WASM) ---
FROM rust:bookworm AS builder
RUN rustup toolchain install stable \
    && rustup target add wasm32-unknown-unknown --toolchain stable \
    && rustup target list --installed --toolchain stable | grep -q wasm32-unknown-unknown \
    && cargo install trunk --locked --root /usr/local \
    && cargo install wasm-bindgen-cli --version 0.2.130 --root /usr/local
ENV PATH="/usr/local/bin:${PATH}"
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
WORKDIR /app/crates/synth-ui
RUN trunk build --release

# --- Stage 2: static file server ---
FROM nginx:1.27-alpine AS runtime
COPY --from=builder /app/crates/synth-ui/dist /usr/share/nginx/html
# SPA fallback: every route serves index.html; API + JMAP reverse-proxied.
RUN printf 'server {\n  listen 80;\n  root /usr/share/nginx/html;\n  location /api/ { proxy_pass http://synth-control:8000; proxy_http_version 1.1; }\n  location /jmap { proxy_pass http://synth-webmail:3000; proxy_http_version 1.1; }\n  location /jmap/ { proxy_pass http://synth-webmail:3000; proxy_http_version 1.1; }\n  location / { try_files $uri $uri/index.html /index.html; }\n  gzip on;\n  gzip_types text/css application/javascript;\n}\n' > /etc/nginx/conf.d/default.conf
EXPOSE 80
# Probe with a real GET on 127.0.0.1: --spider (HEAD) and `localhost`
# (busybox resolves ::1 first) both refused this nginx.
HEALTHCHECK CMD wget -q -O /dev/null http://127.0.0.1/ || exit 1
