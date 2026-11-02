# Ops kit: backups, restore tests, alerts

Everything that keeps the VPS stacks safe between deploys. It runs on the host as systemd timers, not in a container: the timers keep running while a stack is down, catch up on missed runs (`Persistent=true`), and log to `journalctl`.

Think of it as the night watch: it makes a copy of the books every night, checks once a day that the copy really opens, and walks the building every five minutes.

| Timer | When | What |
| --- | --- | --- |
| `genesy-backup-production` | daily 02:30 | `backup.sh production`: pg_dump, kept 14 days |
| `genesy-backup-staging` | daily 03:00 | `backup.sh staging`: kept 3 days |
| `genesy-restore-test-production` | daily 04:00 | `restore-test.sh production` |
| `genesy-restore-test-staging` | Sundays 04:30 | `restore-test.sh staging` |
| `genesy-watch` | every 5 min | `watch.sh`: hosts, certificates, restart loops, disk, backup age |
| `genesy-prune-production`, `-staging` | Sundays 05:00 / 05:15 | `deploy.sh <env> prune`: image tags other than live and previous, build cache above 10 GB |

## Files

| Where | What |
| --- | --- |
| `deploy/ops/*.sh` | The kit. `setup.sh` copies it to `/usr/local/lib/genesy-ops` (root-owned), so a `git checkout` never changes a running script. |
| `deploy/ops/*.test.sh` | Tests with stub `docker`/`curl`/`age`/`openssl`: `bash deploy/ops/backup.test.sh` and the others. No Docker or VPS needed. |
| `/srv/genesy-backups/<env>/` | Dumps (`genesy-<env>-<UTC time>[-label].dump`) and their `.meta` (schema version). Folder 700, files 600, root only. |
| `/etc/genesy-ops/` | Settings, root only: `ping_base`, `age.pub`, `offsite.env`, `offsite_key`. Each one is optional and turns on one feature. |
| `/var/lib/genesy-ops/` | `watch.sh` memory (restart counts). |
| `/etc/sudoers.d/genesy-ops` | Lets user `deploy` run `backup.sh` and `restore.sh`, nothing else. |

## One-time setup (as root, from the production checkout)

```bash
cd /srv/genesy-prod && sudo bash deploy/ops/setup.sh
```

It installs the kit, the folders, the sudo rule and the timers, then lists the timers. Re-run it after every update of `deploy/ops`: it installs the current scripts. Once it has run, the first backup can be taken right away:

```bash
sudo systemctl start genesy-backup-production genesy-restore-test-production
sudo journalctl -u genesy-backup-production -u genesy-restore-test-production --since -10min
```

Three features stay off until the user decides on them. Each one turns on later without code changes:

| Feature | Until then | Turn it on |
| --- | --- | --- |
| Alerts (healthchecks.io, free) | Failures only show in `journalctl` | Make the checks below, then `sudo bash deploy/ops/setup.sh ping` and paste the ping base URL (asked without echo) |
| Encryption (`age`) | Nothing leaves the box, so nothing needs encrypting | `sudo apt install age`; make a key pair on the laptop (below); `sudo bash deploy/ops/setup.sh age` and paste the **public** key |
| Off-site copies (Backblaze B2, decided) | Dumps live on this disk only | Backup key (below), then `sudo bash deploy/ops/setup.sh offsite` |

### Alerts (healthchecks.io)

1. In a healthchecks.io project, make these checks. Use slugs, so the ping URL is `https://hc-ping.com/<ping key>/<slug>`:

   | Slug | Period | Grace |
   | --- | --- | --- |
   | `production-backup` | 1 day | 2 h |
   | `staging-backup` | 1 day | 2 h |
   | `production-restore-test` | 1 day | 2 h |
   | `staging-restore-test` | 7 days | 1 day |
   | `vps-watch` | 5 min | 10 min |

2. Settings → copy the project's ping key. `sudo bash deploy/ops/setup.sh ping`, then paste `https://hc-ping.com/<ping key>`.

Each script pings `/start` when it begins, the plain URL when it succeeds and `/fail` with the reason when it fails. A ping that never comes also raises an alert. So a dead VPS, a broken timer or a script that crashes before it can report all reach the user's mailbox. healthchecks.io mails once when a check goes down and once when it recovers, not every five minutes.

### Off-site copies (Backblaze B2)

Backups go to the **existing production bucket** (the one file uploads use), under their own prefix `db-backups/<env>/`, with their **own key**. The uploads key can't see that prefix, and the backup key can't see the uploads under `genesy/` (`../production/README.md`, "Turning on file uploads", has both keys side by side).

What the user makes, once:

1. **Key pair, on the laptop:** `age-keygen -o genesy-backup.key`. Put the whole file in the password manager and delete it from disk. The line `# public key: age1…` is the public key. Only that goes to the VPS (`setup.sh age`). The VPS can encrypt but never decrypt.
2. **Backup key** with the b2 CLI (`b2 account authorize` with the master key first). The console's presets can include `deleteFiles`; the CLI sets exactly these capabilities:

   ```bash
   b2 key create --bucket <bucket> --name-prefix db-backups/ genesy-prod-db-backups listBuckets,writeFiles
   ```

   Check with `b2 key list --long`: no `deleteFiles`, no `readFiles`. Someone who takes over the VPS can then neither download nor delete the off-site dumps.
3. **Lifecycle rule** on the bucket (Bucket Settings → Lifecycle Settings → custom rules). **Add** this rule and keep any rule already there for other prefixes:

   | File path (prefix) | Days till hide | Days till delete |
   | --- | --- | --- |
   | `db-backups/` | 30 | 1 |

   That is the 30-day off-site retention. B2 enforces it, so no key on the VPS ever needs delete rights. (Rejected: a keep-the-last-N script with a delete-capable key. That key would sit on the box it is meant to protect against.) If backups stopped for 30 days, the rule would empty the prefix. The `production-backup` and `vps-watch` alerts fire within a day, long before that.
4. **On the VPS:** `sudo apt install age`, `sudo bash deploy/ops/setup.sh age` (paste the public key), then `sudo bash deploy/ops/setup.sh offsite`. It asks for the bucket, region (default `eu-central-003`), endpoint (default `https://s3.<region>.backblazeb2.com`) and prefix (default `db-backups`), then the backup key as `<keyID>:<applicationKey>` without echo.

From then on each backup is age-encrypted and uploaded to `<bucket>/db-backups/<env>/<file>.dump.age` with `curl --aws-sigv4` (curl 7.75 or later; no other tool). The encrypted copy is then removed locally; the plain dump stays on the VPS for the restore test. `backup.sh` refuses to upload when `age.pub` is missing. Until `offsite.env` exists, nothing leaves the box.

## Backup now

```bash
sudo /usr/local/lib/genesy-ops/backup.sh production            # or staging
sudo /usr/local/lib/genesy-ops/backup.sh production before-fix # label goes in the file name
sudo ls -lt /srv/genesy-backups/production | head
```

`deploy.sh` takes one before every deploy (label `pre-<sha>`). Old dumps go after 14 days (production) or 3 (staging).

**Also back up `deploy/<env>/secrets/credentials_key`** to the password manager, once. It encrypts the workspace mail credentials stored in the database. A dump restored without the same key can't read them.

## Restore test

`restore-test.sh <env>` restores the newest dump into a throwaway Postgres: the same pinned image as the stack, `--network none`, removed afterwards. Then it checks three things:

- the dump is under 26 hours old;
- the restored `_sqlx_migrations` is at or past the version recorded at dump time;
- production has at least one workspace.

Run it by hand: `sudo /usr/local/lib/genesy-ops/restore-test.sh production`.

## Restore into the live database

Only to undo damage, or through `deploy.sh <env> rollback --restore-db`. It replaces the live data, and everything written after that dump is lost.

```bash
sudo ls -lt /srv/genesy-backups/production | head            # pick the dump
sudo /usr/local/lib/genesy-ops/restore.sh production /srv/genesy-backups/production/<file>.dump
deploy/deploy.sh production compose up -d --wait              # restore.sh leaves the apps stopped
```

`restore.sh` first takes a `pre-restore` dump of the current state, so a wrong restore can itself be undone. On production it asks for the words `restore production`.

## Off-site restore drill (quarterly, on the laptop)

Proves the off-site copy and the private key work together, without touching the VPS:

```bash
# Download db-backups/<env>/<file>.dump.age from the bucket (B2 web UI with the master login), then:
age -d -i genesy-backup.key -o genesy.dump genesy-production-….dump.age
docker run -d --name drill -e POSTGRES_USER=genesy -e POSTGRES_HOST_AUTH_METHOD=trust postgres:18-alpine
sleep 5 && docker exec -i drill pg_restore -U genesy -d genesy --no-owner < genesy.dump
docker exec drill psql -U genesy -d genesy -c 'SELECT count(*) FROM workspaces'
docker rm -fv drill && rm genesy.dump
```

## Watch (every 5 minutes)

`watch.sh` sends one alert, `vps-watch`, that lists everything wrong, each line prefixed with its env:

- **Hosts:** every public host answers through the local Traefik. The API `/healthz` must give 200, the apps 2xx/3xx, and the shop is also allowed 503 until its app key is set.
- **Certificates:** each one verifies (a missing Let's Encrypt certificate fails) and stays valid for 14 more days. Let's Encrypt renews at 30, so 14 means renewal is stuck.
- **Restart loops:** a container's restart count went up 3 or more since the last run, or a container is `restarting` or `unhealthy`.
- **Disk:** `/` or the backups disk at 80 % or more (`DISK_ALERT_PCT`).
- **Backup age:** no production dump in the last 26 hours.

An env with no checkout is skipped. By hand: `sudo /usr/local/lib/genesy-ops/watch.sh`.

## Day 2

- Next and last runs: `systemctl list-timers 'genesy-*'`.
- Logs: `journalctl -u 'genesy-*' --since today`.
- Free space: `df -h / /srv`, `docker system df`, `sudo du -sh /srv/genesy-backups/*`.
- Prune now: `sudo systemctl start genesy-prune-production`.

## Not done here (decisions for the user)

- Docker's daemon-wide log defaults (`/etc/docker/daemon.json`). Changing them restarts Docker, and that bounces every project on the box (control360 too). Genesy's containers already cap and compress their own logs.
- Where CI and staging should live: `decisions.md`.
