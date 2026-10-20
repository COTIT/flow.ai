# syntax=docker/dockerfile:1
# Build: BuildKit cache mounts keep crate registry + target dir between builds (no cargo-chef needed).
FROM rust:1.99-slim-trixie AS build
WORKDIR /app
COPY . .
# sqlx query! macros read committed .sqlx/ instead of live DB at build time.
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked \
    && cp target/release/genesy-360-api /genesy-360-api

# Runtime: distroless, no shell, non-root.
# Same Debian release as build stage (trixie = 13): binary links against its glibc.
FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /genesy-360-api /genesy-360-api
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/genesy-360-api"]
