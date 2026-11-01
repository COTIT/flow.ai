# Production

The live Genesy stack on the VPS. It runs the API with its own Postgres and the five apps, all served through the existing Traefik.

It is a separate compose project, `genesy-prod`. It never touches the `control360` or `genesy-prod-green` projects, and its router names (`genesy-production-*`) don't clash with theirs.

| Host                | Service | Container port |
| ------------------- | ------- | -------------- |
| `api.genesy.app`    | api     | 8080           |
| `shop.genesy.app`   | shop    | 3000           |
| `office.genesy.app` | office  | 3000           |
| `till.genesy.app`   | till    | 3000           |
| `field.genesy.app`  | field   | 8080           |

**How it differs from staging** (`../staging/README.md`):

- **No Mailpit.** Mail goes to the real SMTP server in `secrets/smtp_url`.
- **No fake payments.** The API image is the default build, which refuses `PAYMENTS_PROVIDER=fake`. With no provider set, web payments answer 503 until a real provider is chosen (see `deferred-providers.md`).
- **No demo seed, no self-signup.** `GENESY_ALLOW_DEMO_SEED` is unset and `SIGNUP_ENABLED=false` (also the API default). The first account comes from `create-owner`; everyone else is invited.
- **External Postgres volume.** It is called `genesy-prod-pg` and you make it by hand, so no `down -v` can delete it.
- **No `noindex` header, no IP allow-list.**
- **Field release flag off.** `VITE_FEATURE_RESOURCES=off` hides equipment, loans and requests; no demo banner.
- **No uploads bucket, no PDF renderer yet.** Those routes answer 503, as designed.

Every step below is for the user to run on the VPS, or for the coordinator once the user says go. VPS facts and DNS records: `../staging/README.md` ("The VPS", "DNS records"). Production needs the `api`, `shop`, `office`, `till` and `field` records.

## Files

| File                            | What                                                                 |
| ------------------------------- | -------------------------------------------------------------------- |
| `docker-compose.production.yml` | The stack, with Traefik labels                                       |
| `.env.production.example`       | Every variable, no secrets. Copy it to `.env.production` on the VPS. |
| `secrets/`                      | Secret files on the VPS only. Git ignores everything in it.          |
| `../images/*`                   | Web images, shared with staging                                      |
| `../../apps/api/Dockerfile`     | API image (default build, no build args)                             |

## One-time setup (on the VPS, as user `deploy`)

```bash
# 1. A deploy checkout, separate from staging and CI
sudo mkdir -p /srv/genesy-prod && sudo chown deploy /srv/genesy-prod
git clone git@github.com:COTIT/genesy.git /srv/genesy-prod
cd /srv/genesy-prod/deploy/production
```

```bash
# 2. Secret files (random, made on the VPS, never copied anywhere)
chmod 700 secrets
pw="$(openssl rand -hex 24)"
printf '%s' "$pw" > secrets/db_password
printf 'postgres://genesy:%s@db:5432/genesy' "$pw" > secrets/database_url
openssl rand -hex 32 | tr -d '\n' > secrets/credentials_key
unset pw
# The mail provider's SMTP URL (TLS: smtps://… or smtp://…?tls=required).
printf '%s' 'smtps://USER:PASSWORD@HOST:465' > secrets/smtp_url
chmod 444 secrets/*   # containers run as non-root users; the 700 folder keeps other users out
```

There's no mail provider yet. Until one is chosen, put `smtps://unset.invalid:465` in `smtp_url`: the API boots, and mails fail and stay in the outbox.

```bash
# 3. The database volume (external: compose never creates or deletes it)
docker volume create genesy-prod-pg
```

```bash
# 4. Env file (Traefik values are already filled in; check MAIL_FROM)
cp .env.production.example .env.production && chmod 600 .env.production
```

Back up `secrets/credentials_key` off the VPS, for example in the password manager. It encrypts the workspace mail credentials stored in the database. If it's lost, those credentials can't be read, and a backup can't be restored with them.

## Deploy from main (after CI is green)

Only deploy a commit whose `ci` workflow passed on `main`, and that staging already runs.

```bash
cd /srv/genesy-prod
git fetch origin && git checkout --detach <green-main-sha>
sed -i "s/^GENESY_REF=.*/GENESY_REF=$(git rev-parse --short HEAD)/" deploy/production/.env.production
```

```bash
# Back up first (see Backup), then:
docker compose -f deploy/production/docker-compose.production.yml --env-file deploy/production/.env.production build
docker compose -f deploy/production/docker-compose.production.yml --env-file deploy/production/.env.production up -d --wait
```

```bash
# Keep Docker lean: drop unused old prod images and the build cache
docker image prune -af --filter label=com.docker.compose.project=genesy-prod
docker builder prune -af
```

The API runs its migrations when it boots. Check the deploy:

```bash
curl -fsS https://api.genesy.app/healthz
for h in shop office till field; do curl -s -o /dev/null -w "$h %{http_code}\n" https://$h.genesy.app/; done
```

**Roll back:** set `GENESY_REF` to the previous tag (its images are still there until pruned), then `up -d --wait`. Migrations only move forward. If the new release changed the database, restore the backup taken before the deploy.

## First owner

Self-signup is off, so the first account is made from the command line, once:

```bash
docker compose -f deploy/production/docker-compose.production.yml --env-file deploy/production/.env.production \
  --profile cli run --rm cli create-owner --email <email> --workspace <name>
```

This runs `genesy-360-api create-owner`. It creates the owner and their workspace, and prints a one-time password **once**. Hand that password to the owner over a private channel. Everyone else is invited from the office.

`cli` is its own service with no Traefik labels, so the one-off container never receives web traffic.

**The shop's app key:**

1. Log in to `office.genesy.app`.
2. Make an app key with scope `catalog:read`.
3. Put it in `STOREFRONT_APP_KEY` in `.env.production`.
4. Run `up -d shop`.

## Backup

A compressed dump of the whole database, taken while it runs:

```bash
mkdir -p /srv/genesy-prod/backups && chmod 700 /srv/genesy-prod/backups
docker compose -f deploy/production/docker-compose.production.yml --env-file deploy/production/.env.production \
  exec -T db pg_dump -U genesy -d genesy -Fc > /srv/genesy-prod/backups/genesy-$(date +%F-%H%M).dump
```

Copy dumps off the VPS: a backup on the same disk dies with the disk.

To restore into the running database (this replaces its contents):

```bash
docker compose -f deploy/production/docker-compose.production.yml --env-file deploy/production/.env.production stop api shop office till field
docker compose -f deploy/production/docker-compose.production.yml --env-file deploy/production/.env.production \
  exec -T db pg_restore -U genesy -d genesy --clean --if-exists < /srv/genesy-prod/backups/<file>.dump
docker compose -f deploy/production/docker-compose.production.yml --env-file deploy/production/.env.production up -d --wait
```

## Never

- Never run `docker volume rm genesy-prod-pg`: it deletes production data. (`down -v` can't, since the volume is external.)
- Never set `GENESY_ALLOW_DEMO_SEED`, `PAYMENTS_PROVIDER=fake` or `ALLOW_FAKE_PAYMENTS` here.
- Never reuse staging's secret files.
