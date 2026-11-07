#!/usr/bin/env bash
# Tests for setup.sh, installed under a temp PREFIX:  bash deploy/ops/setup.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
p="$(mktemp -d)"
trap 'rm -rf "$p"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
mode() { stat -c %a "$1" 2> /dev/null || stat -f %Lp "$1"; }
run() { PREFIX="$p" bash "$HERE/setup.sh" "$@" > "$p/out" 2>&1; }

run || fail "setup: $(cat "$p/out")"
for f in lib.sh backup.sh restore.sh restore-test.sh watch.sh; do
  cmp -s "$HERE/$f" "$p/usr/local/lib/genesy-ops/$f" || fail "$f not installed"
done
cmp -s "$HERE/../assistant-role.sql" "$p/usr/local/lib/genesy-ops/assistant-role.sql" || fail 'role bootstrap SQL not installed'
[[ "$(mode "$p/srv/genesy-backups")" == 700 && "$(mode "$p/etc/genesy-ops")" == 700 ]] || fail "dirs not root-only"
[[ "$(cat "$p/etc/sudoers.d/genesy-ops")" == "deploy ALL=(root) NOPASSWD: /usr/local/lib/genesy-ops/backup.sh, /usr/local/lib/genesy-ops/restore.sh" ]] \
  || fail "sudo rule: $(cat "$p/etc/sudoers.d/genesy-ops")"
[[ "$(mode "$p/etc/sudoers.d/genesy-ops")" == 440 ]] || fail "sudo rule mode"
u="$p/etc/systemd/system"
grep -qx "ExecStart=/usr/local/lib/genesy-ops/backup.sh production" "$u/genesy-backup-production.service" || fail "backup unit"
grep -qx "OnCalendar=\*-\*-\* 02:30" "$u/genesy-backup-production.timer" || fail "backup timer"
grep -qx "Persistent=true" "$u/genesy-backup-production.timer" || fail "missed runs not caught up"
grep -qx "OnCalendar=\*:0/5" "$u/genesy-watch.timer" || fail "watch timer"
grep -qx "User=deploy" "$u/genesy-prune-staging.service" || fail "prune not as deploy"
grep -qx "ExecStart=/srv/genesy-staging/deploy/deploy.sh staging prune" "$u/genesy-prune-staging.service" || fail "prune unit"
[[ "$(ls "$u"/*.timer | wc -l | tr -d ' ')" == 7 ]] || fail "timer count"
grep -q "no ping URL yet" "$p/out" || fail "no hint for the missing ping"

# Secrets: read without echo, 600, never printed.
printf 'https://hc-ping.com/SECRETPING\n' | run ping || fail "ping: $(cat "$p/out")"
[[ "$(cat "$p/etc/genesy-ops/ping_base")" == https://hc-ping.com/SECRETPING ]] || fail "ping_base"
[[ "$(mode "$p/etc/genesy-ops/ping_base")" == 600 ]] || fail "ping_base mode"
if grep -q SECRETPING "$p/out"; then fail "ping URL echoed"; fi

# Off-site needs the age key first; then bucket settings and a 600 key file.
if printf 'b\n\n\n\nid:KEY\n' | run offsite; then fail "offsite without age key"; fi
if echo "not-a-key" | run age; then fail "bad age key taken"; fi
echo "age1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqs3290gq" | run age || fail "age: $(cat "$p/out")"
if printf 'b\n\nhttp://s3.x\n\nid:KEY\n' | run offsite; then fail "plain http endpoint taken"; fi
if printf 'b\n\n\nDB/../x\nid:KEY\n' | run offsite; then fail "odd prefix taken"; fi
printf 'bkt\n\n\n\nkeyid:SECRETKEY\n' | run offsite || fail "offsite: $(cat "$p/out")"
[[ "$(cat "$p/etc/genesy-ops/offsite.env")" == "$(printf 'S3_ENDPOINT=https://s3.eu-central-003.backblazeb2.com\nS3_BUCKET=bkt\nS3_REGION=eu-central-003\nS3_PREFIX=db-backups')" ]] \
  || fail "offsite.env defaults: $(cat "$p/etc/genesy-ops/offsite.env")"
[[ "$(cat "$p/etc/genesy-ops/offsite_key")" == keyid:SECRETKEY ]] || fail "offsite_key"
[[ "$(mode "$p/etc/genesy-ops/offsite_key")" == 600 ]] || fail "offsite_key mode"
if grep -q SECRETKEY "$p/out"; then fail "offsite key echoed"; fi
if run nonsense; then fail "unknown mode taken"; fi

echo "setup.test.sh: all passed"
