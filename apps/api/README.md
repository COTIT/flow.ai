# apps/api (crate `genesy-360-api`)

The Genesy backend: Axum 0.8, sqlx 0.9, PostgreSQL 18. Existing `/v1` paths keep their old shapes (frozen); new domains get clean REST paths. All business rules live here; the Next.js apps only display and collect data.

## Run locally

There is no compose file in this repo. You need a Postgres 18 you can reach; the shared dev one listens on `127.0.0.1:54320`.

```bash
cd apps/api
cp .env.example .env   # replace CHANGE_ME in both *_DATABASE_URL lines with the dev DB password
cargo run              # runs migrations at boot, listens on BIND_ADDR (default 0.0.0.0:8080)
curl localhost:8080/healthz
```

For invite and verification emails, point `SMTP_URL` at a local Mailpit (`smtp://127.0.0.1:1025`, UI on :8025).

Builds without a database: set `SQLX_OFFLINE=true` and the `query!` macros read the committed `.sqlx/` cache. On a laptop also set `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` to keep `target/` small.

## Environment

All read once at boot. A missing required var stops the server with the var name. The table lists the common ones; `src/config.rs` is the full list (uploads, jobs, per-key and per-workspace limits, mail allow-lists, credentials key) and `.env.example` has working local values.

| Var | Required | Default | Notes |
|---|---|---|---|
| `DATABASE_URL` | yes | | app + `#[sqlx::test]` |
| `GENESY_DATABASE_URL` | dev/CI | | same value; `query!` macros + sqlx-cli (see `sqlx.toml`) |
| `DATABASE_MAX_CONNECTIONS` | no | `20` | 1..=100 (empty = default), request pool size. Each connection runs with `statement_timeout` 30s, `idle_in_transaction_session_timeout` 60s, `lock_timeout` 10s; a full pool answers 503 (with `Retry-After: 5`) after 5s. `/healthz` and the hourly cleanup each have one connection of their own; migrations use their own connection at boot. On the VPS the role's `CONNECTION LIMIT` (`GENESY_PG_CONNECTION_LIMIT`, default 44) must cover both blue/green colors: 2 × (pool + 2). |
| `BIND_ADDR` | no | `0.0.0.0:8080` | |
| `TOKEN_TTL_DAYS` | no | `30` | 1..=365 |
| `FRONTEND_URL` | yes | | web app origin for email links |
| `API_PUBLIC_URL` | yes | | this API's public origin; verification links point here |
| `SMTP_ALLOW_INSECURE` | no | `false` | `true` lets `SMTP_URL` be plain `smtp://` (local Mailpit only) |
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
cargo sqlx prepare --check -- --all-targets   # .sqlx/ cache must match queries (needs the live DB)
```

Each `#[sqlx::test]` gets its own throwaway database, so tests run in parallel without sharing state.

Migrations are embedded in the binary and run at boot (`sqlx::migrate!`), so a fresh database needs no separate step. An hourly cleanup job (`src/cleanup.rs`) deletes expired tokens, invites answered or expired over 30 days ago, and quota logs older than a day.

## OpenAPI spec

Every route carries a `#[utoipa::path]`, and `openapi.json` is generated from them. After changing a route or its request/response types:

```bash
pnpm --filter @genesy/api openapi                 # rewrites openapi.json
pnpm turbo run generate --filter=@genesy/api-client   # regenerates the TS client (runs the line above first)
grep -ciE 'laravel|vimdesk' openapi.json          # must print 0
```

Commit `openapi.json` and `packages/api-client/src/generated` with the change. `cargo test --test openapi` fails on a stale spec, and the contract check in `tests/common/contract.rs` fails when a real reply doesn't match it.

## Why two DB URL vars

`sqlx.toml` points the compile-time `query!` macros and sqlx-cli at `GENESY_DATABASE_URL`. The macros read every `.env` up the directory tree and the last match wins, so the repo-root `.env` (with another app's `DATABASE_URL`) would otherwise take over. The app and `#[sqlx::test]` read `DATABASE_URL` from the nearest `.env`.

## Layout

See `.claude/skills/genesy-360-api/SKILL.md` (section 4) for where code goes and why.

## Docker

```bash
docker build -t genesy-360-api:local .
```

The runtime image is distroless and runs as non-root.

## CI and deploy

CI (`.github/workflows/ci.yml`) runs on a self-hosted runner; setup is in `deploy/ci-runner/`. There is no deploy pipeline in this repo yet.
