#!/usr/bin/env bash
# One-time ops kit for the VPS (deploy/ops/README.md). Run from the production checkout, as root;
# re-running is safe and installs the current scripts.
#   sudo bash deploy/ops/setup.sh           scripts, folders, sudo rule, timers
#   sudo bash deploy/ops/setup.sh ping      dead-man ping base URL (healthchecks.io), asked without echo
#   sudo bash deploy/ops/setup.sh age       age public key for off-site copies (age1…)
#   sudo bash deploy/ops/setup.sh offsite   S3 bucket settings + key, the key asked without echo
# PREFIX (tests only) installs under another root and skips systemctl.
set -Eeuo pipefail

KIT="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
P="${PREFIX:-}"
LIB="$P/usr/local/lib/genesy-ops" CONF="$P/etc/genesy-ops" UNITS="$P/etc/systemd/system"
die() { echo "setup: $*" >&2; exit 1; }
[[ -n "$P" || "$(id -u)" == 0 ]] || die "run as root (sudo)"
install -d -m 700 "$CONF"

# secret <file> <prompt>: one line read without echo, stored 600.
secret() {
  local value
  read -r -s -p "$2: " value && echo
  [[ -n "$value" ]] || die "empty, nothing written"
  (umask 077 && printf '%s\n' "$value" > "$CONF/$1")
}

case "${1:-}" in
  ping)
    secret ping_base "Ping base URL (https://hc-ping.com/<project ping key>)"
    exit 0 ;;
  age)
    read -r -p "age public key (age1…): " key
    [[ "$key" =~ ^age1[0-9a-z]{58}$ ]] || die "not an age public key"
    printf '%s\n' "$key" > "$CONF/age.pub" && chmod 644 "$CONF/age.pub"
    exit 0 ;;
  offsite)
    [[ -s "$CONF/age.pub" ]] || die "set the age key first (setup.sh age): copies leave the box encrypted only"
    command -v age > /dev/null || [[ -n "$P" ]] || die "age is not installed (apt install age)"
    # The production B2 bucket (the one uploads use), under its own prefix, with its own key.
    read -r -p "Bucket: " bucket
    read -r -p "Region [eu-central-003]: " region
    region="${region:-eu-central-003}"
    read -r -p "S3 endpoint [https://s3.$region.backblazeb2.com]: " endpoint
    endpoint="${endpoint:-https://s3.$region.backblazeb2.com}"
    read -r -p "Prefix [db-backups]: " prefix
    prefix="${prefix:-db-backups}"
    [[ "$endpoint" =~ ^https:// && -n "$bucket" ]] || die "endpoint must be https and the bucket set"
    [[ "$prefix" =~ ^[a-z0-9-]+$ ]] || die "prefix may hold a-z, 0-9 and -"
    secret offsite_key "Backup key (not the uploads key) as <keyID>:<applicationKey>"
    (umask 077 && printf 'S3_ENDPOINT=%s\nS3_BUCKET=%s\nS3_REGION=%s\nS3_PREFIX=%s\n' \
      "$endpoint" "$bucket" "$region" "$prefix" > "$CONF/offsite.env")
    exit 0 ;;
  "") ;;
  *) die "unknown mode '$1' (ping, age, offsite or nothing)" ;;
esac

for tool in docker flock curl openssl; do
  command -v "$tool" > /dev/null || [[ -n "$P" ]] || die "$tool is missing"
done

# Scripts root-owned: deploy may run two of them through sudo, but never change them.
install -d -m 755 "$LIB"
for f in lib.sh backup.sh restore.sh restore-test.sh watch.sh; do
  install -m 755 "$KIT/$f" "$LIB/$f"
done
install -d -m 700 "$P/srv/genesy-backups" "$P/var/lib/genesy-ops"

# deploy.sh (user deploy) takes the pre-deploy backup and, on rollback --restore-db, the restore.
# sudo resets the environment, so no GENESY_* override reaches them.
install -d -m 750 "$P/etc/sudoers.d"
rule="$P/etc/sudoers.d/genesy-ops"
printf 'deploy ALL=(root) NOPASSWD: %s, %s\n' "/usr/local/lib/genesy-ops/backup.sh" "/usr/local/lib/genesy-ops/restore.sh" > "$rule.new"
chmod 440 "$rule.new"
[[ -n "$P" ]] || visudo -cqf "$rule.new" || die "sudoers rule invalid"
mv "$rule.new" "$rule"

# unit <name> <description> <ExecStart> [User]: a oneshot service, low CPU and IO priority.
unit() {
  install -d "$UNITS"
  cat > "$UNITS/$1.service" << EOF
[Unit]
Description=$2
Wants=docker.service
After=docker.service

[Service]
Type=oneshot
User=${4:-root}
Nice=10
IOSchedulingClass=idle
ExecStart=$3
EOF
}
# timer <name> <OnCalendar>
timer() {
  cat > "$UNITS/$1.timer" << EOF
[Unit]
Description=Schedule for $1

[Timer]
OnCalendar=$2
RandomizedDelaySec=${3:-10m}
Persistent=true

[Install]
WantedBy=timers.target
EOF
  TIMERS+=("$1.timer")
}

TIMERS=()
unit genesy-backup-production "Genesy production database backup" "/usr/local/lib/genesy-ops/backup.sh production"
timer genesy-backup-production "*-*-* 02:30"
unit genesy-backup-staging "Genesy staging database backup" "/usr/local/lib/genesy-ops/backup.sh staging"
timer genesy-backup-staging "*-*-* 03:00"
unit genesy-restore-test-production "Genesy production restore test" "/usr/local/lib/genesy-ops/restore-test.sh production"
timer genesy-restore-test-production "*-*-* 04:00"
unit genesy-restore-test-staging "Genesy staging restore test" "/usr/local/lib/genesy-ops/restore-test.sh staging"
timer genesy-restore-test-staging "Sun *-*-* 04:30"
unit genesy-watch "Genesy uptime, certificates, restarts, disk, backup age" "/usr/local/lib/genesy-ops/watch.sh"
timer genesy-watch "*:0/5" 0
# Prune runs each env's own deploy.sh (its lock keeps it off a running deploy).
unit genesy-prune-production "Genesy production image prune" "/srv/genesy-prod/deploy/deploy.sh production prune" deploy
timer genesy-prune-production "Sun *-*-* 05:00"
unit genesy-prune-staging "Genesy staging image prune" "/srv/genesy-staging/deploy/deploy.sh staging prune" deploy
timer genesy-prune-staging "Sun *-*-* 05:15"

if [[ -z "$P" ]]; then
  systemctl daemon-reload
  systemctl enable --now "${TIMERS[@]}" > /dev/null
  systemctl list-timers 'genesy-*' --no-pager
fi
[[ -s "$CONF/ping_base" ]] || echo "setup: no ping URL yet (setup.sh ping): failures only show in journalctl"
[[ -f "$CONF/offsite.env" ]] || echo "setup: off-site copies off (setup.sh age, then setup.sh offsite): dumps stay on this disk"
echo "setup: done"
