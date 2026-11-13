# syntax=docker/dockerfile:1
# Build: dependencies compile in their own layer, which changes only with Cargo.toml/Cargo.lock.
# CI keeps it in a registry layer cache (.github/workflows/ci.yml, genesy-buildcache) and the VPS
# in its build cache, so most builds compile only this crate. Cache mounts would not survive CI.
# Base images pinned by digest (index, all platforms); bump tag and digest together.
FROM rust:1.99.0-slim-trixie@sha256:2752b332db73fdbb7dc576f06c82ed1f312005784ef913d7e04a28f5f55dc581 AS build
WORKDIR /app
# sqlx query! macros read committed .sqlx/ instead of live DB at build time.
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
# Stub crate, build, then drop the stub's own artifacts so the real crate always rebuilds.
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs && echo 'fn main() {}' > build.rs \
    && cargo build --release --locked \
    && cargo clean --release -p genesy-360-api && rm -rf src build.rs
COPY . .
# Staging only (deploy/staging): 1 turns on debug assertions for this crate, which is what lets
# PAYMENTS_PROVIDER=fake boot. Default 0: the prod image refuses the fake provider. This crate only,
# so both variants share the dependency layer above.
ARG ALLOW_FAKE_PAYMENTS=0
RUN if [ "$ALLOW_FAKE_PAYMENTS" = 1 ]; then set -- --config 'profile.release.package.genesy-360-api.debug-assertions=true'; fi \
    && cargo build --release --locked "$@" \
    && cp target/release/genesy-360-api /genesy-360-api

# Runtime: distroless, no shell, non-root (uid 65532).
# Same Debian release as build stage (trixie = 13): binary links against its glibc.
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:08476c244888bfb425bae1ae4dd7c17f093e972aab8426883d97f3c6f280f7c1 AS runtime
# Links the GHCR package to the repo (Actions push access) and marks our images on the VPS.
LABEL org.opencontainers.image.source="https://github.com/COTIT/flow.ai"
COPY --from=build /genesy-360-api /genesy-360-api
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=6 \
    CMD ["/genesy-360-api", "healthcheck"]
ENTRYPOINT ["/genesy-360-api"]
