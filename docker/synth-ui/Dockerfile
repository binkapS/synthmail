# SynthMail — synth-ui (Leptos SPA) build + static serve.
#
# Why this was slow: the old builder ran `cargo install trunk` +
# `cargo install wasm-bindgen-cli` from source, then did a cache-less
# `trunk build --release`. Any UI edit changed the COPY layer hash, so
# Cargo threw away *every* dependency artifact from the previous build
# and recompiled the world (`synth_ui` alone is one giant leptos crate,
# ~600 MB rustc, opt-level=3) — 15+ minutes on a 4-core box.
#
# Fixes below:
#   1. BuildKit cache mounts for the cargo registry + /app/target —
#      deps and artifacts survive across builds; a style.css-only edit
#      rebuilds in seconds (incremental, see synth-ui profile override).
#   2. Pinned trunk/wasm-bindgen-cli versions — unpinned installs
#      cache-bust on every new upstream release.
#   3. `rust:bookworm` already ships stable; no redundant
#      `rustup toolchain install stable` (which re-downloaded it).
#   4. Do NOT COPY rust-toolchain.toml into the builder + pin rustup
#      offline. Copying it made rustup treat the image toolchain as
#      stale and re-download the whole stable channel (rustc tarball)
#      on the first `cargo metadata` inside `trunk build` — 29 min
#      hung on DNS/HTTPS then failed. Matching synth-control/webmail
#      (they never copy it) + RUSTUP_UPDATE_STRATEGY=none keeps the
#      build on the toolchain already in the image.
#
# Dev loop (much faster than docker for iteration): install trunk once
# on the host (`cargo install --locked trunk`) and run
#   cd crates/synth-ui && trunk serve --proxy-backend http://localhost:8000
# Docker is only needed for the deployable image.

# --- Stage 1: Trunk build (WASM) ---
FROM rust:bookworm AS builder

ARG TRUNK_VERSION=0.21.14
ARG WASM_BINDGEN_VERSION=0.2.130

# Never let rustup sync/download a toolchain inside the builder: the
# rust:bookworm image already has stable; RUSTUP_TOOLCHAIN pins it and
# UPDATE_STRATEGY=none makes any rust-toolchain.toml influence inert.
ENV CARGO_INCREMENTAL=1 \
    RUSTUP_TOOLCHAIN=stable \
    RUSTUP_UPDATE_STRATEGY=none \
    CARGO_NET_RETRY=5
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo install --locked --version "${TRUNK_VERSION}" trunk --root /usr/local \
    && cargo install --locked --version "${WASM_BINDGEN_VERSION}" wasm-bindgen-cli --root /usr/local

ENV PATH="/usr/local/bin:${PATH}"
WORKDIR /app
# NB: no rust-toolchain.toml here (see fix #4 above). The host file
# requests `stable` + rustfmt/clippy components; without it rustup
# just uses the image's preinstalled stable and never phones home.
COPY Cargo.toml Cargo.lock ./
COPY crates crates
WORKDIR /app/crates/synth-ui
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    trunk build --release --locked

# --- Stage 2: static file server ---
FROM nginx:1.27-alpine AS runtime
COPY --from=builder /app/crates/synth-ui/dist /usr/share/nginx/html
# SPA fallback: every route serves index.html; API + JMAP reverse-proxied.
RUN printf 'server {\n  listen 80;\n  root /usr/share/nginx/html;\n  location /api/ { proxy_pass http://synth-control:8000; proxy_http_version 1.1; }\n  location /jmap { proxy_pass http://synth-webmail:3000; proxy_http_version 1.1; }\n  location /jmap/ { proxy_pass http://synth-webmail:3000; proxy_http_version 1.1; }\n  location / { try_files $uri $uri/index.html /index.html; }\n  gzip on;\n  gzip_types text/css application/javascript;\n}\n' > /etc/nginx/conf.d/default.conf
EXPOSE 80
# Probe with a real GET on 127.0.0.1: --spider (HEAD) and `localhost`
# (busybox resolves ::1 first) both refused this nginx.
HEALTHCHECK CMD wget -q -O /dev/null http://127.0.0.1/ || exit 1
