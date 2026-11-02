# Production

The live Genesy stack on the VPS. It runs the API with its own Postgres, the five apps and the landing page, all served through the existing Traefik.

It is a separate compose project, `genesy-prod`. It never touches the `control360` or `genesy-prod-green` projects, and its router names (`genesy-production-*`) don't clash with theirs.

| Host                | Service | Container port |
| ------------------- | ------- | -------------- |
| `api.genesy.app`    | api     | 8080           |
| `shop.genesy.app`   | shop    | 3000           |
| `office.genesy.app` | office  | 3000           |
| `till.genesy.app`   | till    | 3000           |
| `field.genesy.app`  | field   | 8080           |
| `genesy.app`        | landing | 3000           |
| `www.genesy.app`    | landing (301 to `https://genesy.app`) | 3000 |

**How it differs from staging** (`../staging/README.md`):

- **No Mailpit.** Mail goes to the real SMTP server in `secrets/smtp_url`.
- **No fake payments.** The API image is the default build, which refuses `PAYMENTS_PROVIDER=fake`. Web payments answer 503 until Stripe is turned on ("Turning on Stripe").
- **No demo seed, no self-signup.** `GENESY_ALLOW_DEMO_SEED` is unset and `SIGNUP_ENABLED=false` (also the API default). The first account comes from `create-owner`; everyone else is invited.
- **External Postgres volume.** It is called `genesy-prod-pg` and you make it by hand, so no `down -v` can delete it.
- **No `noindex` header, no IP allow-list.**
- **Field release flag off.** `VITE_FEATURE_RESOURCES=off` hides equipment, loans and requests; no demo banner.
- **No uploads bucket, no PDF renderer yet.** Those routes answer 503, as designed.

Every step below is for the user to run on the VPS, or for the coordinator once the user says go. VPS facts and DNS records: `../staging/README.md` ("The VPS", "DNS records"). Production needs the `api`, `shop`, `office`, `till` and `field` records, plus the apex (`@`) and `www` for the landing page. The landing page's contact address is `CONTACT_EMAIL` in `.env.production` (default `hello@genesy.app`, a placeholder).

## Files

| File                            | What                                                                 |
| ------------------------------- | -------------------------------------------------------------------- |
| `docker-compose.production.yml` | The stack, with Traefik labels                                       |
| `docker-compose.stripe.yml`     | Turns Stripe on when added with a second `-f` ("Turning on Stripe")  |
| `../set-provider-keys.sh`       | Writes Stripe or Resend keys into `secrets/` without echoing them    |
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

Mail goes through Resend ("Turning on Resend"). Until its key is in, put `smtps://unset.invalid:465` in `smtp_url`: the API boots, and mails fail and stay in the outbox.

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
# Keep Docker lean: drop unused old prod images, and cap the build cache at 10 GB
docker image prune -af --filter label=com.docker.compose.project=genesy-prod
docker builder prune -f --max-used-space 10GB
```

The kept cache makes the next build fast: an unchanged API is a cache hit, a changed one rebuilds only its own crate, and the web images reuse their package store.

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

## Turning on Stripe

Payments answer 503 until this is done. Code and contract: `apps/api/docs/api/payments.md`. One Stripe account serves every workspace.

1. **Stripe dashboard, live mode:**
   - Developers → API keys: copy the secret key (`sk_live_…`), or make a restricted key (`rk_live_…`) with Checkout Sessions and Refunds set to Write.
   - Developers → Webhooks → Add endpoint:
     - URL `https://api.genesy.app/v1/payments/webhooks/stripe` (one endpoint for every workspace; no workspace in the path).
     - Events `checkout.session.completed`, `checkout.session.async_payment_succeeded`, `checkout.session.async_payment_failed`, `checkout.session.expired`, `refund.updated`, `refund.failed`.
     - Copy its signing secret (`whsec_…`).
   - Settings → Payment methods: turn on what the shop offers (cards, iDEAL, Bancontact, SEPA debit). Checkout shows what is on there.
2. **On the VPS:** write the keys. They are asked for without echo, so they never land in shell history.

   ```bash
   cd /srv/genesy-prod && deploy/set-provider-keys.sh production stripe
   ```

3. **Restart the API with the Stripe file in the chain.** From now on, **every** `docker compose` command in this README needs the same `-f` chain. A command without it recreates the API without Stripe, and payments answer 503 again; nothing breaks beyond that.

   ```bash
   docker compose -f deploy/production/docker-compose.production.yml -f deploy/production/docker-compose.stripe.yml \
     --env-file deploy/production/.env.production up -d --wait api
   ```

   Until DNS is live, the VPS may still have an untracked `deploy/production/docker-compose.nodns.yml`. While that file exists, keep it in the chain, between the main file and the Stripe file:

   ```bash
   docker compose -f deploy/production/docker-compose.production.yml -f deploy/production/docker-compose.nodns.yml \
     -f deploy/production/docker-compose.stripe.yml --env-file deploy/production/.env.production up -d --wait api
   ```

4. **Check it:** `docker compose … logs api | grep -i payments` no longer shows `PAYMENTS_PROVIDER not set`. Pay one small real order in the shop, refund it from the office, and look for both in the Stripe dashboard (Payments, and Webhooks → the endpoint's deliveries, all 200).

A wrong or missing key stops the API at boot with a message that names the variable, never the value. To turn Stripe off, run the same command without `docker-compose.stripe.yml`. To roll a key, run the script again and restart.

## Turning on Resend (mail)

No code: Resend takes SMTP, so `secrets/smtp_url` becomes `smtps://resend:<API key>@smtp.resend.com:465`.

1. **Resend dashboard → Domains → Add domain** `genesy.app`. Resend then lists the DNS records to add. Copy the values from the dashboard: the DKIM key is unique, and the MX host depends on the region you pick.

   | Type | Name (zone `genesy.app`) | Value (from the dashboard)                          | Notes                 |
   | ---- | ------------------------ | --------------------------------------------------- | --------------------- |
   | MX   | `send`                   | `feedback-smtp.<region>.amazonses.com`, priority 10 | bounces (return path) |
   | TXT  | `send`                   | `v=spf1 include:amazonses.com ~all`                 | SPF                   |
   | TXT  | `resend._domainkey`      | `p=MIGfMA0…` (long key)                             | DKIM                  |
   | TXT  | `_dmarc`                 | `v=DMARC1; p=none;`                                 | optional, recommended |

   Add them in OpenProvider and press Verify in Resend; it can take up to a few hours. If `_dmarc` already exists, keep the existing one.

2. **Resend → API Keys:** make a key with Sending access, limited to `genesy.app`.
3. **On the VPS:** `deploy/set-provider-keys.sh production resend`. It rewrites `secrets/smtp_url` without echoing the key.
4. `MAIL_FROM` in `.env.production` must use the verified domain (the example already says `Genesy <no-reply@genesy.app>`).
5. Restart the API with your usual `-f` chain (`up -d --wait api`). Then invite yourself from the office and check that the mail arrives, and that Resend → Emails shows it as delivered.

## Never

- Never run `docker volume rm genesy-prod-pg`: it deletes production data. (`down -v` can't, since the volume is external.)
- Never set `GENESY_ALLOW_DEMO_SEED`, `PAYMENTS_PROVIDER=fake` or `ALLOW_FAKE_PAYMENTS` here.
- Never reuse staging's secret files.
- Never paste a Stripe or Resend key into chat, an issue, an env file or a command line: only into `set-provider-keys.sh`.
