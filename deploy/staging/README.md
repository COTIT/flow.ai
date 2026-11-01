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
- No prod secret, ever.
- The field app still reads Supabase for most screens until its port to the API lands. On staging it loads, but those screens stay empty.

Every step below is for the user to run on the VPS, or for the coordinator once the user says go.

## Files

| File                         | What                                                                                 |
| ---------------------------- | ------------------------------------------------------------------------------------ |
| `docker-compose.staging.yml` | The stack, with Traefik labels                                                       |
| `docker-compose.local.yml`   | Local override: no Traefik, ports on `127.0.0.1:18080` and `18100`–`18103`           |
| `.env.staging.example`       | Every variable, no values. Copy it to `.env.staging` on the VPS.                     |
| `secrets/`                   | Secret files on the VPS only. Git ignores everything in it.                          |
| `next.Dockerfile`            | shop, office and till (Next standalone, `--build-arg APP=…`)                         |
| `field.Dockerfile`           | field: a Vite build, served by nginx                                                 |
| `field.nginx.conf`           | SPA fallback and the `/v1` forward to the API                                        |
| `smoke.sh`                   | Local check: build, start, probe, then remove everything it added                    |
| `../../apps/api/Dockerfile`  | API image. The build arg `ALLOW_FAKE_PAYMENTS=1` is set by the staging compose only. |

### The fake-payments build arg

A release build normally refuses `PAYMENTS_PROVIDER=fake`. The staging compose builds the API with `ALLOW_FAKE_PAYMENTS=1`, which sets `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true`.

That has a side effect: every `debug_assert!` in the API and its crates runs on staging. They are extra self-checks. They can make staging a little slower, and an assertion that fails stops the request with a panic instead of letting it pass quietly.

In `apps/api/src`, the only thing `cfg!(debug_assertions)` decides is the payments choice. The default build (prod) never gets this arg.

## Assumptions about the VPS (check before the first deploy)

- Traefik v3 (the label is `ipallowlist`; v2 calls it `ipwhitelist`), with the Docker provider. Every router here sets `traefik.enable=true`, so `exposedByDefault=false` is fine.
- Traefik already redirects HTTP to HTTPS on its own.
- Traefik is the first hop: no CDN in front of it. If there is one, the IP allow-list and the API's per-IP rate limit see the CDN's address instead of the visitor's.
- Three facts are unknown. They go in `.env.staging`, which refuses to start until they are filled in:
  - `TRAEFIK_NETWORK`: the network `control360-traefik-traefik-1` is attached to;
  - `TRAEFIK_ENTRYPOINT`: the HTTPS entrypoint (guess: `websecure`);
  - `TRAEFIK_CERTRESOLVER`: the Let's Encrypt resolver (guess: `letsencrypt`).

## DNS records (OpenProvider, zone `genesy.app`)

Point five names at the VPS. Either add one A record each:

| Type | Name             | Value        | TTL  |
| ---- | ---------------- | ------------ | ---- |
| A    | `staging-api`    | `<VPS IPv4>` | 3600 |
| A    | `staging-shop`   | `<VPS IPv4>` | 3600 |
| A    | `staging-office` | `<VPS IPv4>` | 3600 |
| A    | `staging-till`   | `<VPS IPv4>` | 3600 |
| A    | `staging-field`  | `<VPS IPv4>` | 3600 |

Add a matching AAAA record for each name if the VPS has IPv6.

Or use one A record, `staging` → `<VPS IPv4>`, plus five CNAMEs (`staging-api` → `staging.genesy.app.`, and so on). Then a new IP is a one-record change.

Let's Encrypt can only issue the certificates once these names resolve.

## One-time setup (on the VPS)

```bash
# 1. Read the Traefik facts for .env.staging
docker inspect control360-traefik-traefik-1 --format '{{json .NetworkSettings.Networks}}'
docker inspect control360-traefik-traefik-1 --format '{{json .Config.Cmd}}'   # entrypoints, resolver
```

```bash
# 2. A deploy checkout, separate from the CI runner's
sudo mkdir -p /srv/genesy-staging && sudo chown "$USER" /srv/genesy-staging
git clone git@github.com:<owner>/genesy.git /srv/genesy-staging
cd /srv/genesy-staging/deploy/staging
```

```bash
# 3. Secret files (random, made on the VPS, never copied anywhere)
chmod 700 secrets
pw="$(openssl rand -hex 24)"
printf '%s' "$pw" > secrets/db_password
printf 'postgres://genesy:%s@db:5432/genesy' "$pw" > secrets/database_url
openssl rand -hex 32 | tr -d '\n' > secrets/credentials_key
unset pw
chmod 444 secrets/*   # containers run as non-root users; the 700 folder keeps other users out
```

```bash
# 4. Env file: fill in TRAEFIK_NETWORK, TRAEFIK_ENTRYPOINT, TRAEFIK_CERTRESOLVER
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

This wipes the staging database and every caught mail, then starts clean:

```bash
cd /srv/genesy-staging
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging down -v
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging up -d --wait
```

`down -v` only removes this project's volume (`genesy-staging_pg`). The secret files stay, so the new database gets the same password.

To remove staging entirely, run `down -v --rmi all`, then delete `/srv/genesy-staging` and the DNS records.

## Demo data

Today, the RUAHA farm demo workspace:

```bash
docker compose -f deploy/staging/docker-compose.staging.yml --env-file deploy/staging/.env.staging --profile seed run --rm seed
```

This runs `genesy-360-api seed-farm-demo`. It prints the demo passwords once, so save them somewhere safe.

**Placeholder (after Phase 4c):** a `seed-demo` command will fill every app with demo data, and `seed-demo --reset` will wipe and reseed it. The `seed` service's `command` changes to `["seed-demo"]` then. Reset with seed becomes:

```bash
docker compose ... --profile seed run --rm seed seed-demo --reset
```

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
