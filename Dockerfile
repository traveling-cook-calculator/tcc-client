# syntax=docker/dockerfile:1
#
# Static frontend: Tailwind -> `dx bundle` (WebAssembly) -> nginx.
# No server code runs in the container; the API is a separate service.
#
# The image carries NO environment-specific settings. `config.json` (login server, API address,
# legal texts) is mounted at runtime to /usr/share/nginx/html/config.json, and the origins in the
# Content-Security-Policy come from the environment (API_ORIGIN, AUTH_ORIGIN).

# ── 1. Tailwind CSS ───────────────────────────────────────────────
FROM node:22-bookworm-slim AS css
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
# Tailwind scans the sources for the classes in use.
COPY assets/input.css assets/input.css
COPY src src
RUN npx @tailwindcss/cli -i ./assets/input.css -o ./assets/output.css --minify

# ── 2. WebAssembly bundle ─────────────────────────────────────────
FROM rust:1-bookworm AS build
ARG TARGETARCH
# Must match the `dioxus` version in Cargo.lock.
ARG DX_VERSION=0.7.10
RUN rustup target add wasm32-unknown-unknown \
 && case "${TARGETARCH:-amd64}" in \
      amd64) DX_ARCH=x86_64 ;; \
      arm64) DX_ARCH=aarch64 ;; \
      *) echo "unsupported architecture ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
 && curl -fsSL "https://github.com/DioxusLabs/dioxus/releases/download/v${DX_VERSION}/dx-${DX_ARCH}-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin dx
WORKDIR /app
COPY . .
COPY --from=css /app/assets/output.css assets/output.css
RUN dx bundle --platform web --release

# ── 3. Runtime ────────────────────────────────────────────────────
FROM nginxinc/nginx-unprivileged:stable-alpine AS runtime
COPY --from=build /app/target/dx/tcc/release/web/public /usr/share/nginx/html
# The placeholder config of the repository must never end up in an environment by accident.
USER root
RUN rm -f /usr/share/nginx/html/config.json
USER 101
# envsubst renders this to /etc/nginx/conf.d/default.conf when the container starts.
COPY docker/nginx.conf.template /etc/nginx/templates/default.conf.template
ENV API_ORIGIN="" AUTH_ORIGIN="" CSP_MODE="Content-Security-Policy-Report-Only"
EXPOSE 8080
