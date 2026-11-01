# Staging

A full Genesy stack on the VPS, next to the existing services. It runs the API with its own Postgres, Mailpit (a mail catcher), and the five apps, all served through the VPS's existing Traefik.

Think of it as a dress rehearsal stage: same sets as opening night, fake money in the till, and every letter lands in a box backstage.

| Host                        | Service | Container port |
| --------------------------- | ------- | -------------- |
| `staging-api.genesy.app`    | api     | 8080           |
| `staging-shop.genesy.app`   | shop    | 3000           |
| `staging-office.genesy.app` | office  | 3000           |
| `staging-till.genesy.app`   | till    | 3000           |
| `staging-field.genesy.app`  | field   | 8080           |

**What staging is not:**

- No real payment provider. `PAYMENTS_PROVIDER=fake`, so checkout links go to `pay.fake.test`.
- No outgoing mail. Every mail lands in Mailpit.
- No uploads bucket, no PDF renderer. Those routes answer 503, as designed.
- Field is built with `VITE_FEATURE_RESOURCES=on` (equipment, loans, requests) and the demo banner.
- No prod secret, ever.

Every step below is for the user to run on the VPS, or for the coordinator once the user says go.

## Files

| File                         | What                                                                                 |
| ---------------------------- | ------------------------------------------------------------------------------------ |
| `docker-compose.staging.yml` | The stack, with Traefik labels                                                       |
| `docker-compose.local.yml`   | Local override: no Traefik, ports on `127.0.0.1:18080` and `18100`–`18103`           |
| `.env.staging.example`       | Every variable, no secrets. Copy it to `.env.staging` on the VPS.                    |
| `secrets/`                   | Secret files on the VPS only. Git ignores everything in it.                          |
| `../images/next.Dockerfile`  | shop, office and till (Next standalone, `--build-arg APP=…`); shared with production |
| `../images/field.Dockerfile` | field: a Vite build, served by nginx; shared with production                         |
| `../images/field.nginx.conf` | SPA fallback and the `/v1` forward to the API                                        |
| `smoke.sh`                   | Local check: build, start, probe, then remove everything it added                    |
| `../../apps/api/Dockerfile`  | API image. The build arg `ALLOW_FAKE_PAYMENTS=1` is set by the staging compose only. |

### The fake-payments build arg

A release build normally refuses `PAYMENTS_PROVIDER=fake`. The staging compose builds the API with `ALLOW_FAKE_PAYMENTS=1`, which sets `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true`.

That has a side effect: every `debug_assert!` in the API and its crates runs on staging. They are extra self-checks. They can make staging a little slower, and an assertion that fails stops the request with a panic instead of letting it pass quietly.

In `apps/api/src`, the only thing `cfg!(debug_assertions)` decides is the payments choice. The default build (prod) never gets this arg.

## The VPS (read on 2026-11-01)

- Traefik v3.6, Docker provider, `exposedByDefault=false` (every router here sets `traefik.enable=true`).
- External network `traefik`. Entrypoints `web` (redirects to HTTPS) and `websecure`. Cert resolver `le` (HTTP challenge). These are the defaults in `.env.staging.example`.
- IPv4 `185.77.96.44`, IPv6 `2a02:4780:d:ca37::1`. 4 cores, 15 GB RAM.
- Deploy user `deploy`. Staging lives in `/srv/genesy-staging`, production in `/srv/genesy-prod`, CI in `/srv/genesy-ci`.
- Existing hosts: `control.genesy.app` and `*.control.genesy.app` only. Nothing else under `genesy.app` is taken.
- Traefik is the first hop (no CDN), so the IP allow-list and the API's per-IP rate limit see the visitor's address.

## DNS records (OpenProvider, zone `genesy.app`)

Staging and production, one A and one AAAA record per host:

| Type | Name             | Value                 | TTL  |
| ---- | ---------------- | --------------------- | ---- |
| A    | `staging-api`    | `185.77.96.44`        | 3600 |
| AAAA | `staging-api`    | `2a02:4780:d:ca37::1` | 3600 |
| A    | `staging-shop`   | `185.77.96.44`        | 3600 |
| AAAA | `staging-shop`   | `2a02:4780:d:ca37::1` | 3600 |
| A    | `staging-office` | `185.77.96.44`        | 3600 |
| AAAA | `staging-office` | `2a02:4780:d:ca37::1` | 3600 |
| A    | `staging-till`   | `185.77.96.44`        | 3600 |
| AAAA | `staging-till`   | `2a02:4780:d:ca37::1` | 3600 |
| A    | `staging-field`  | `185.77.96.44`        | 3600 |
| AAAA | `staging-field`  | `2a02:4780:d:ca37::1` | 3600 |
| A    | `api`            | `185.77.96.44`        | 3600 |
| AAAA | `api`            | `2a02:4780:d:ca37::1` | 3600 |
| A    | `shop`           | `185.77.96.44`        | 3600 |
| AAAA | `shop`           | `2a02:4780:d:ca37::1` | 3600 |
| A    | `office`         | `185.77.96.44`        | 3600 |
| AAAA | `office`         | `2a02:4780:d:ca37::1` | 3600 |
| A    | `till`           | `185.77.96.44`        | 3600 |
| AAAA | `till`           | `2a02:4780:d:ca37::1` | 3600 |
| A    | `field`          | `185.77.96.44`        | 3600 |
| AAAA | `field`          | `2a02:4780:d:ca37::1` | 3600 |

Traefik answers on IPv6 (checked 2026-11-01: HTTP 301 on `[2a02:4780:d:ca37::1]`), so the AAAA records are safe.

Let's Encrypt can only issue the certificates once these names resolve.

## One-time setup (on the VPS)

```bash
# 1. A deploy checkout (as user deploy), separate from the CI runner's
sudo mkdir -p /srv/genesy-staging && sudo chown deploy /srv/genesy-staging
git clone git@github.com:COTIT/genesy.git /srv/genesy-staging
cd /srv/genesy-staging/deploy/staging
```

```bash
# 2. Secret files (random, made on the VPS, never copied anywhere)
chmod 700 secrets
pw="$(openssl rand -hex 24)"
printf '%s' "$pw" > secrets/db_password
printf 'postgres://genesy:%s@db:5432/genesy' "$pw" > secrets/database_url
openssl rand -hex 32 | tr -d '\n' > secrets/credentials_key
unset pw
chmod 444 secrets/*   # containers run as non-root users; the 700 folder keeps other users out
```

```bash
# 3. Env file (Traefik values are already filled in)
cp .env.staging.example .env.staging && chmod 600 .env.staging
```

Then deploy (next section). Keep `credentials_key` once you have it: it encrypts workspace mail credentials stored in the database. A new key makes those stored credentials unreadable.

## Deploy from main (after CI is green)

Only deploy a commit whose `ci` workflow passed on `main`.

```bash
cd /srv/genesy-staging
git fetch origin && git checkout --detach <green-main-sha>
sed -i "s/^GENESY_REF=.*/GENESY_REF=$(git rev-parse --short HEAD)/" deploy/staging/.env.staging
```

```bash
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging build
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging up -d --wait
```

```bash
# Keep Docker lean: drop old staging images and the build cache
docker image prune -af --filter label=com.docker.compose.project=genesy-staging   # unused ones only
docker builder prune -af
```

The API runs its migrations when it boots. Check the deploy:

```bash
curl -fsS https://staging-api.genesy.app/healthz
for h in shop office till field; do curl -s -o /dev/null -w "$h %{http_code}\n" https://staging-$h.genesy.app/; done
```

**The shop's app key.** The shop needs a storefront key before it shows a catalog:

1. Log in to `staging-office`.
2. Make an app key with scope `catalog:read`.
3. Put it in `STOREFRONT_APP_KEY` in `.env.staging`.
4. Run `up -d shop`.

**Read mail (Mailpit).** From your own machine:

```bash
ssh -L 8025:127.0.0.1:18025 genesy
```

Then open <http://localhost:8025>.

## Reset

This wipes the staging database and every caught mail, starts clean, then reseeds:

```bash
cd /srv/genesy-staging
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging down -v
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging up -d --wait
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging --profile seed run --rm seed
```

`down -v` only removes this project's volume (`genesy-staging_pg`). The secret files stay, so the new database gets the same password.

To remove staging entirely, run `down -v --rmi all`, then delete `/srv/genesy-staging` and the staging DNS records.

## Demo data

After the first deploy, and after each reset:

```bash
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging --profile seed run --rm seed
```

This runs `genesy-360-api seed-demo` (from Phase 4c), which fills every app with demo data. It only runs where `GENESY_ALLOW_DEMO_SEED=1`, and only the staging compose sets that. If it prints demo passwords, save them somewhere safe.

`seed` is its own service with no Traefik labels, so the one-off container never receives web traffic.

## Local smoke check (no VPS, no Traefik)

```bash
deploy/staging/smoke.sh
```

It:

- builds all five images, including the release Rust build (10–20 minutes);
- starts the stack with throwaway secrets;
- checks `api /healthz`;
- checks each app's home page, and that each app's `/v1` forward reaches the API;
- checks that the API booted with fake payments.

When it finishes, it removes its containers and volume, deletes only the images that were not there before, and runs `docker builder prune -af`. Other containers (`genesy-dev-pg`, `genesy-pg-*`) are never touched.
