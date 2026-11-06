# Staging

A full Genesy stack on the VPS, next to the existing services. It runs the API with its own Postgres, Mailpit (a mail catcher), the five apps and the landing page, all served through the VPS's existing Traefik.

Think of it as a dress rehearsal stage: same sets as opening night, fake money in the till, and every letter lands in a box backstage.

| Host                        | Service | Container port |
| --------------------------- | ------- | -------------- |
| `staging-api.genesy.app`    | api     | 8080           |
| `staging-shop.genesy.app`   | shop    | 3000           |
| `staging-office.genesy.app` | office  | 3000           |
| `staging-till.genesy.app`   | till    | 3000           |
| `staging-field.genesy.app`  | field   | 8080           |
| `staging.genesy.app`        | landing | 3000           |
| `staging-files.genesy.app`  | files   | 3900 (GET/HEAD only, once "Image store" is on) |

**What staging is not:**

- No real payment provider. `PAYMENTS_PROVIDER=fake`, so checkout links go to `pay.fake.test`.
- No outgoing mail. Every mail lands in Mailpit.
- No PDF renderer: those routes answer 503, as designed. File uploads answer 503 too until the image store is on ("Image store"); it is staging's own Garage, never the production B2 bucket.
- Field is built with `VITE_FEATURE_RESOURCES=on` (equipment, loans, requests) and the demo banner.
- No prod secret, ever.

Every step below is for the user to run on the VPS, or for the coordinator once the user says go.

## Files

| File                         | What                                                                                 |
| ---------------------------- | ------------------------------------------------------------------------------------ |
| `docker-compose.staging.yml` | The stack, with Traefik labels                                                       |
| `docker-compose.files.yml`   | Image store overlay (Garage), added by `deploy.sh` once `secrets/files.env` exists |
| `garage.toml`                | Garage settings, no secrets                                                          |
| `docker-compose.local.yml`   | Local override: no Traefik, ports on `127.0.0.1:18080` and `18100`–`18103`           |
| `.env.staging.example`       | Every variable, no secrets. Copy it to `.env.staging` on the VPS.                    |
| `secrets/`                   | Secret files on the VPS only. Git ignores everything in it.                          |
| `../images/next.Dockerfile`  | shop, office and till (Next standalone, `--build-arg APP=…`); shared with production |
| `../images/field.Dockerfile` | field: a Vite build, served by nginx; shared with production                         |
| `../images/field.nginx.conf` | SPA fallback and the `/v1` forward to the API                                        |
| `smoke.sh`                   | Local check: build, start, probe, then remove everything it added                    |
| `../deploy.sh`               | Deploy, rollback, prune, and `compose` with the right `-f` chain (production README)  |
| `../ops/`                    | Backups, restore tests, alerts for both stacks (`../ops/README.md`)                   |
| `../../apps/api/Dockerfile`  | API image. The build arg `ALLOW_FAKE_PAYMENTS=1` is set by the staging compose only. |

### The fake-payments build arg

A release build normally refuses `PAYMENTS_PROVIDER=fake`. The staging compose builds the API with `ALLOW_FAKE_PAYMENTS=1`, which sets `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true`.

That has a side effect: every `debug_assert!` in the API and its crates runs on staging. They are extra self-checks. They can make staging a little slower, and an assertion that fails stops the request with a panic instead of letting it pass quietly.

In `apps/api/src`, the only thing `cfg!(debug_assertions)` decides is the payments choice. The default build (prod) never gets this arg.

## The VPS (read on 2026-11-01)

- Traefik v3.6, Docker provider, `exposedByDefault=false` (every router here sets `traefik.enable=true`).
- External network `traefik`. Entrypoints `web` (redirects to HTTPS) and `websecure`. Cert resolver `le` (HTTP challenge). These are the defaults in `.env.staging.example`.
- IPv4 `185.77.96.44`, IPv6 `2a02:4780:d:ca37::1`. 4 cores, 15 GB RAM.
- Deploy user `deploy`. Staging lives in `/srv/genesy-staging` and production in `/srv/genesy-prod`.
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
| A    | `staging`        | `185.77.96.44`        | 3600 |
| AAAA | `staging`        | `2a02:4780:d:ca37::1` | 3600 |
| A    | `@` (apex)       | `185.77.96.44`        | 3600 |
| AAAA | `@` (apex)       | `2a02:4780:d:ca37::1` | 3600 |
| A    | `www`            | `185.77.96.44`        | 3600 |
| AAAA | `www`            | `2a02:4780:d:ca37::1` | 3600 |

Traefik answers on IPv6 (checked 2026-11-01: HTTP 301 on `[2a02:4780:d:ca37::1]`), so the AAAA records are safe.

Let's Encrypt can only issue the certificates once these names resolve.

## One-time setup (on the VPS)

```bash
# 1. A deploy checkout (as user deploy)
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

```bash
# 4. Read-only login to GHCR, where CI pushes the private images (token: "GHCR pull token" below).
#    One login serves staging and production: both run as user deploy.
install -d -m 700 ~/.config/genesy && install -m 600 /dev/null ~/.config/genesy/ghcr-token
nano ~/.config/genesy/ghcr-token   # paste the token, save; it never goes on a command line
docker login ghcr.io -u <token owner> --password-stdin < ~/.config/genesy/ghcr-token
```

Then deploy (next section). Keep `credentials_key` once you have it: it encrypts workspace mail credentials stored in the database. A new key makes those stored credentials unreadable.

### GHCR pull token (once, in GitHub)

The VPS needs read access to the six private `ghcr.io/cotit/genesy-*` packages, nothing more. These steps are yours, in GitHub.

1. **COTIT → Settings → Packages:** allow private container packages to be created. The first green `images` run then creates `genesy-api`, `-shop`, `-office`, `-till`, `-field` and `-landing`. Check each one's settings: visibility **private**, and "Manage Actions access" lists `COTIT/genesy` with **Write**.
2. **A machine account** (for example `genesy-deploy-bot`) in COTIT, given **Read** on those six packages only (package settings → Manage access). It needs no repository access.
3. **On that account:** a **classic** personal access token with only `read:packages`, expiring in a year (set a reminder). GHCR does not take fine-grained tokens unless GitHub lists a Packages permission for them when you create one.

Fallback without a machine account: your own classic token with only `read:packages`. It can read every package you can, so the machine account is safer.

To rotate: write the new token into `~/.config/genesy/ghcr-token` on the VPS, run the `docker login` line again, then delete the old token.

## Deploy from main (after CI is green)

Only deploy a commit whose `ci` workflow passed on `main`, including its two `images` jobs (api and web). They built, scanned and pushed this commit's images, or copied the tags of the last green commit when their inputs didn't change (`ghcr.io/cotit/genesy-<app>:<sha>`, with `-staging` for api, office and field); the VPS only pulls them. It is the same script and the same steps as production (`../production/README.md`, "Deploy from main", "Rollback" and "Fallback: build on the VPS"); only the env name changes:

```bash
cd /srv/genesy-staging && deploy/deploy.sh staging <green-main-sha> --bundle /tmp/genesy.bundle
deploy/deploy.sh staging rollback            # back to the previous version
```

On staging the migrations run in a one-off `seed` container (`genesy-360-api migrate`, not the seed). Every manual compose command goes through `deploy/deploy.sh staging compose …`, which adds the Stripe and no-DNS files when they are there. The nightly backup, weekly restore test and alerts come from the ops kit, installed once from the production checkout (`../ops/README.md`).

**The shop's app key.** The shop needs a storefront key before it shows a catalog:

1. Log in to `staging-office`.
2. Make an app key with scope `catalog:read`.
3. Put it in `STOREFRONT_APP_KEY` in `.env.staging`.
4. Run `deploy/deploy.sh staging compose up -d shop`.

**Read mail (Mailpit).** From your own machine:

```bash
ssh -L 8025:127.0.0.1:18025 genesy
```

Then open <http://localhost:8025>.

## Turning on Stripe (test mode)

Staging runs the in-process fake until this is done. The staging image keeps debug assertions on, so the API refuses a live key here: only Stripe test mode, test cards (`4242 4242 4242 4242`) and no real money.

1. **Stripe dashboard, test mode:** copy the test secret key (`sk_test_…`). Then add a webhook endpoint `https://staging-api.genesy.app/v1/payments/webhooks/stripe` with the same events as production (`deploy/production/README.md`, "Turning on Stripe"), and copy its signing secret (`whsec_…`).
2. **On the VPS:** `deploy/set-provider-keys.sh staging stripe` (asks for both, no echo, refuses a live key).
3. **Restart the API.** `deploy.sh` adds `docker-compose.stripe.yml` by itself once `secrets/stripe_secret_key` exists. The `seed` service keeps its own fake whatever is set.

   ```bash
   deploy/deploy.sh staging compose up -d --wait api
   ```

4. If `STAGING_IP_ALLOWLIST` is set, Stripe's webhook calls are blocked: add Stripe's webhook IP ranges (stripe.com/files/ips/ips_webhooks.txt) or leave the list empty.

Mail on staging stays on Mailpit; Resend is production only.

## Image store

Product photos and other uploads live in Garage, a small S3-compatible store, in its own container (`files`, volume `genesy-staging_files`, 0.25 CPU and 256 MB at most). The API writes to it inside Docker (`http://files:3900`). Browsers only read, through Traefik on `staging-files.genesy.app`, which passes GET and HEAD and nothing else. The links are signed by the API and expire after 10 minutes. The shop's and office's image CSP allows exactly that origin.

**Image:** `dxflrs/garage:v2.4.1`, pushed 2026-09-08, pinned by digest in `docker-compose.files.yml`.

**DNS:** one more record, like the other staging hosts: `A staging-files 185.77.96.44`, TTL 3600.

**One-time setup (on the VPS, as user deploy).** Random secrets made on the VPS. Nothing is printed, and nothing goes on a command line that `ps` could show to another user:

```bash
cd /srv/genesy-staging/deploy/staging
umask 077
kid="GK$(openssl rand -hex 12)"
secret="$(openssl rand -hex 32)"
printf 'GARAGE_RPC_SECRET=%s\nGARAGE_DEFAULT_ACCESS_KEY=%s\nGARAGE_DEFAULT_SECRET_KEY=%s\nGARAGE_DEFAULT_BUCKET=genesy-staging\n' \
  "$(openssl rand -hex 32)" "$kid" "$secret" > secrets/files.env
printf '%s' "$secret" > secrets/files_secret_key
echo "FILES_KEY_ID=$kid" >> .env.staging
unset kid secret
chmod 444 secrets/files_secret_key && chmod 400 secrets/files.env
```

`printf` is a shell builtin, so the secret never shows up in the process list. The key id isn't secret. `files.env` is read by Docker, not the container user, so it can stay `400`. From the next `deploy.sh staging …` on, the overlay is in every compose command. Keep both files: a new key or RPC secret won't match the stored data.

**Photos.** The overlay runs the seed as `seed-demo --images`. It fetches AFM's product photos one at a time from AFM's image CDN and keeps a copy in the store under `demo-cache/` (a sha256 of the URL), so later reseeds read the copy and never need AFM. If AFM can't be reached and no copy exists, the products are seeded without that photo and one warning line says how many are missing. The rest of the seed is unchanged.

## Reset

This wipes the staging database and every caught mail, starts clean, then reseeds:

```bash
cd /srv/genesy-staging
deploy/deploy.sh staging compose down -v
deploy/deploy.sh staging compose up -d --wait db
deploy/deploy.sh staging compose --profile seed run --rm seed migrate   # the API doesn't migrate at boot
deploy/deploy.sh staging compose up -d --wait
deploy/deploy.sh staging compose --profile seed run --rm seed
```

`down -v` removes this project's volumes: `genesy-staging_pg` and, with the image store on, `genesy-staging_files`, which holds the photo copies too, so the next seed fetches them from AFM again. The secret files stay, so the new database and store get the same password and key.

To remove staging entirely, run `deploy/deploy.sh staging compose down -v --rmi all`, then delete `/srv/genesy-staging`, the staging DNS records and the staging timers (`sudo systemctl disable --now genesy-backup-staging.timer genesy-restore-test-staging.timer genesy-prune-staging.timer`).

## Demo data

After the first deploy, and after each reset:

```bash
deploy/deploy.sh staging compose --profile seed run --rm seed
```

This runs `genesy-360-api seed-demo`, which fills every app with demo data: about 70 products, 300 orders, 40 invoices, a week of till shifts, and 60 farmers over four seasons (full list: `apps/api/docs/api/demo-seed.md`). It only runs where `GENESY_ALLOW_DEMO_SEED=1`, and only the staging compose sets that.

It prints a logins table (`app | role | login | secret | path`), then the storefront app key, sample paths (passports, shop orders) and `seeded in N.N s`. The passwords, PINs and keys are new on every run and are printed only this once, so save them somewhere safe before sharing them.

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
