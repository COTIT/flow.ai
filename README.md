# genesy-360-api

Rust rewrite of `apps/vimdesk-api`: Axum 0.8, sqlx 0.9, PostgreSQL 18. It keeps the Laravel `/v1` paths and response shapes, so `vimdesk-web` can switch over by changing its API base URL.

## Run locally

```bash
# from repo root: start Postgres (host port 54320)
docker compose -f compose.yml -f compose.local.yml up -d postgres

cd apps/genesy-360-api
cp .env.example .env   # put ../../.docker/secrets/db_password value in both *_DATABASE_URL lines
sqlx migrate run       # sqlx-cli built with --features postgres,rustls,sqlx-toml
# invite emails: start Mailpit (UI on :8025), SMTP_URL in .env.example points at it
docker compose -f ../../compose.yml -f ../../compose.local.yml --profile devtools up -d mailpit
cargo run              # listens on BIND_ADDR, default 0.0.0.0:8080
curl localhost:8080/healthz
```

## Environment

All read once at boot. A missing required var stops the server with the var name. See `.env.example`.

| Var | Required | Default | Notes |
|---|---|---|---|
| `DATABASE_URL` | yes | | app + `#[sqlx::test]` |
| `GENESY_DATABASE_URL` | dev/CI | | same value; `query!` macros + sqlx-cli (see `sqlx.toml`) |
| `BIND_ADDR` | no | `0.0.0.0:8080` | |
| `TOKEN_TTL_DAYS` | no | `30` | 1..=365 |
| `FRONTEND_URL` | yes | | vimdesk-web origin for email links |
| `SMTP_URL` | yes | | `smtp://127.0.0.1:1025` (Mailpit), Postmark: `smtp://TOKEN:TOKEN@smtp.postmarkapp.com:587?tls=required` |
| `MAIL_FROM` | yes | | `Genesy <no-reply@...>` |
| `RATE_LIMIT_BURST` | no | `10` | public auth routes, per client IP |
| `RATE_LIMIT_REPLENISH_SECS` | no | `6` | |
| `TRUST_PROXY_HEADERS` | no | `false` | **Set `true` behind Traefik.** With `false` every client shares Traefik's IP, so 10 bad logins lock everyone out. Traefik must not trust client-sent X-Forwarded-For (keep `forwardedHeaders.insecure` off), and port 8080 must not be reachable directly. |
| `INVITES_PER_HOUR` | no | `20` | invite emails per user per hour |
| `RUST_LOG` | no | `info` | |

## Test

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo sqlx prepare --check -- --all-targets   # .sqlx/ cache must match queries
```

Each `#[sqlx::test]` gets its own throwaway database, so tests run in parallel without sharing state.

Migrations are embedded in the binary and run at boot (`sqlx::migrate!`), so a fresh database needs no separate step.

## Why two DB URL vars

`sqlx.toml` points the compile-time `query!` macros and sqlx-cli at `GENESY_DATABASE_URL`. The macros read every `.env` up the directory tree and the last match wins, so the repo-root `.env` (with another app's `DATABASE_URL`) would otherwise take over. The app and `#[sqlx::test]` read `DATABASE_URL` from the nearest `.env`.

## Layout

See `.claude/skills/genesy-360-api/SKILL.md` (section 4) for where code goes and why.

## Docker

```bash
docker build -t genesy-360-api:local .
```

The runtime image is distroless and runs as non-root.
