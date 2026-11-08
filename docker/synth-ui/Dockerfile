# SynthMail — synth-ui (Svelte 5 SPA) build + static serve.

# --- Stage 1: Node build ---
FROM node:22-alpine AS builder
WORKDIR /app/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

# --- Stage 2: static file server ---
FROM nginx:1.27-alpine AS runtime
COPY --from=builder /app/ui/build /usr/share/nginx/html
RUN printf 'server {\n  listen 80;\n  root /usr/share/nginx/html;\n  location /api/v1/mail/ { proxy_pass http://synth-webmail:3000; proxy_http_version 1.1; send_timeout 65; client_max_body_size 12m; }\n  location /api/ { proxy_pass http://synth-control:8000; proxy_http_version 1.1; }\n  location /jmap { proxy_pass http://synth-webmail:3000; proxy_http_version 1.1; }\n  location /jmap/ { proxy_pass http://synth-webmail:3000; proxy_http_version 1.1; }\n  location / { try_files $uri $uri/index.html /index.html; }\n  gzip on;\n  gzip_types text/css application/javascript;\n}\n' > /etc/nginx/conf.d/default.conf
EXPOSE 80
HEALTHCHECK CMD wget -q -O /dev/null http://127.0.0.1/ || exit 1
