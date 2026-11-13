# syntax=docker/dockerfile:1
# Field app (Vite PWA): static files served by nginx, which also forwards /v1 to the API.
# Context = repo root. Base images pinned by digest (index, all platforms).
ARG NODE_IMAGE=node:24.21.0-trixie-slim@sha256:173f125896c3b47ddf056734c7ea789d04595a6a08769a8f78e0df642781fb66

FROM ${NODE_IMAGE} AS build
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0
# Vite bakes VITE_* into the bundle at build time; each compose file sets them.
# VITE_DATA_MODE=demo shows the demo banner; VITE_FEATURE_RESOURCES=on shows equipment, loans
# and requests (release flag).
ARG VITE_DATA_MODE=
ARG VITE_FEATURE_RESOURCES=
ENV VITE_DATA_MODE=${VITE_DATA_MODE} VITE_FEATURE_RESOURCES=${VITE_FEATURE_RESOURCES}
RUN corepack enable
WORKDIR /repo
COPY . .
RUN --mount=type=cache,id=genesy-pnpm-store,target=/pnpm-store,sharing=locked \
    pnpm install --frozen-lockfile --store-dir /pnpm-store
RUN pnpm --filter @genesy/field build

# Non-root nginx (uid 101), listens on 8080.
FROM nginxinc/nginx-unprivileged:1.31.6-alpine3.24@sha256:884e0b7f42f303641b0f4e78755aea647521dca0b89ca3be7992ad78b5c07434 AS runtime
# Links the GHCR package to the repo (Actions push access) and marks our images on the VPS.
LABEL org.opencontainers.image.source="https://github.com/COTIT/flow.ai"
# Patched Alpine packages on every build: the base tag often lags fixes Trivy already knows.
USER root
RUN apk upgrade --no-cache
USER 101
COPY deploy/images/field.nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /repo/apps/field/dist /usr/share/nginx/html
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=6 \
    CMD ["wget", "-q", "-O", "/dev/null", "http://127.0.0.1:8080/"]
