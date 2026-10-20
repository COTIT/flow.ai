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
cargo sqlx prepare --check -- --all-targets   # .sqlx/ cache must match queries
```

Each `#[sqlx::test]` gets its own throwaway database, so tests run in parallel without sharing state.

Migrations are embedded in the binary and run at boot (`sqlx::migrate!`), so a fresh database needs no separate step. An hourly cleanup job (`src/cleanup.rs`) deletes expired tokens, invites answered or expired over 30 days ago, and quota logs older than a day.

## Why two DB URL vars

`sqlx.toml` points the compile-time `query!` macros and sqlx-cli at `GENESY_DATABASE_URL`. The macros read every `.env` up the directory tree and the last match wins, so the repo-root `.env` (with another app's `DATABASE_URL`) would otherwise take over. The app and `#[sqlx::test]` read `DATABASE_URL` from the nearest `.env`.

## Layout

See `.claude/skills/genesy-360-api/SKILL.md` (section 4) for where code goes and why.

## Docker

```bash
docker build -t genesy-360-api:local .
```

The runtime image is distroless and runs as non-root.

## Deploy (preview + prod)

`genesy-api` is a compose service behind the **`genesy` profile**, in `compose.yml` with Traefik labels in `compose.preview.yml`. Its Postgres is the shared `postgres` service in `deploy/shared/docker-compose.yml`, behind the same profile. Without the profile nothing changes for the other services.

`deploy/deploy.sh` turns it on per environment once both shared secrets exist (chmod 644):

- `/home/deploy/shared/secrets/pg_root_password`: the shared Postgres superuser password
- `/home/deploy/shared/secrets/genesy_smtp_url_preview`: the SMTP URL for previews, using a Postmark **sandbox** server so previews never mail real people. Prod sets `GENESY_SMTP_SECRET_NAME=genesy_smtp_url` and uses the real server. Example value: `smtp://TOKEN:TOKEN@smtp.postmarkapp.com:587?tls=required`

Then for each preview, `deploy.sh`:

1. generates a database password and writes the `genesy_database_url` secret file
2. creates the role and database with `deploy/genesy/provision-db.sh`, which is idempotent, owned by the app role, and revokes public connect
3. skips genesy-api (with a note) if the shared Postgres isn't reachable yet
4. starts `genesy-api` at `https://genesy-api-<slug>.dev.360.cotit.io`, or `HOST_GENESY_API` in prod, and waits for `/healthz`

The app runs its own migrations at boot. Its compose healthcheck is `/genesy-360-api healthcheck` (distroless has no shell).

One-time ops setup:

```bash
cd /home/deploy/shared && docker compose --profile genesy up -d postgres
```

Before production, check these. They live in the VPS setup, not in this repo:

- add `HOST_GENESY_API` to `deploy/monitor` `CERT_HOSTS`
- add a `pg_dump` loop to `deploy/backup` (backups are MariaDB-only today)
