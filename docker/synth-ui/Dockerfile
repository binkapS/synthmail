# SynthMail — SynthiUI (Leptos SPA) build + static serve.
# Audit H-11: previously the UI was never built or served.

# --- Stage 1: Trunk build (WASM) ---
FROM trunkwasm/trunk:latest AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
WORKDIR /app/crates/synth-ui
RUN trunk build --release

# --- Stage 2: static file server ---
FROM nginx:1.27-alpine AS runtime
COPY --from=builder /app/crates/synth-ui/dist /usr/share/nginx/html
# SPA fallback: every route serves index.html.
RUN printf 'server {\n  listen 80;\n  root /usr/share/nginx/html;\n  location / {\n    try_files $uri $uri/index.html /index.html;\n  }\n  gzip on;\n  gzip_types text/css application/javascript;\n}\n' > /etc/nginx/conf.d/default.conf
EXPOSE 80
HEALTHCHECK CMD wget -q --spider http://localhost/ || exit 1
