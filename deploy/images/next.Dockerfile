# syntax=docker/dockerfile:1
# One image recipe for the Next apps: --build-arg APP=shop|office|till|landing, context = repo root.
# Base image pinned by digest (index, all platforms); bump tag and digest together.
ARG NODE_IMAGE=node:24.21.0-trixie-slim@sha256:173f125896c3b47ddf056734c7ea789d04595a6a08769a8f78e0df642781fb66

FROM ${NODE_IMAGE} AS build
ARG APP
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0 NEXT_TELEMETRY_DISABLED=1 NEXT_OUTPUT=standalone
# Next bakes the /v1 forward (rewrites) and NEXT_PUBLIC_* in at build time. `api` is the compose
# service name, the same locally and on the VPS, so one image serves both.
ARG API_URL=http://api:8080/v1
ARG NEXT_PUBLIC_SHOP_URL=
ENV API_URL=${API_URL} NEXT_PUBLIC_SHOP_URL=${NEXT_PUBLIC_SHOP_URL}
RUN corepack enable
WORKDIR /repo
COPY . .
RUN --mount=type=cache,id=genesy-pnpm-store,target=/pnpm-store,sharing=locked \
    pnpm install --frozen-lockfile --store-dir /pnpm-store
# office, till and landing have no public/ folder; an empty one keeps the COPY below uniform.
RUN pnpm --filter "@genesy/${APP}" build && mkdir -p "apps/${APP}/public"

FROM ${NODE_IMAGE} AS runtime
# Links the GHCR package to the repo (Actions push access) and marks our images on the VPS.
LABEL org.opencontainers.image.source="https://github.com/COTIT/genesy"
# The server needs only node: drop the package managers the base image ships (and their CVEs).
RUN rm -rf /usr/local/lib/node_modules /opt/yarn-* \
    && rm -f /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/corepack /usr/local/bin/yarn /usr/local/bin/yarnpkg
ARG APP
ENV NODE_ENV=production NEXT_TELEMETRY_DISABLED=1 HOSTNAME=0.0.0.0 PORT=3000
WORKDIR /app
COPY --from=build --chown=node:node /repo/apps/${APP}/.next/standalone ./
COPY --from=build --chown=node:node /repo/apps/${APP}/.next/static ./apps/${APP}/.next/static
COPY --from=build --chown=node:node /repo/apps/${APP}/public ./apps/${APP}/public
WORKDIR /app/apps/${APP}
USER node
EXPOSE 3000
# Any answer below 500 means the server is up (no app has a health route; / may need the API).
HEALTHCHECK --interval=10s --timeout=5s --start-period=20s --retries=6 \
    CMD ["node", "-e", "fetch('http://127.0.0.1:3000/healthz').then(r=>process.exit(r.status<500?0:1),()=>process.exit(1))"]
CMD ["node", "server.js"]
