#!/usr/bin/env bash
# backup.sh <staging|production> [label]: one pg_dump (custom format) of the env's database.
#   - written as .partial, checked with pg_restore --list, then renamed: a dump file is whole;
#   - <dump>.meta keeps the schema version, for restore-test.sh;
#   - kept 14 days (production) or 3 (staging);
#   - copied off the box only when OPS_CONF/offsite.env exists, and only age-encrypted, to
#     <bucket>/<prefix>/<env>/ (the production B2 bucket, prefix db-backups, its own write-only key).
# Runs as root (timer genesy-backup@<env>, or sudo from deploy.sh): dumps are root-only.
set -Eeuo pipefail
. "$(dirname "$(readlink -f "$0")")/lib.sh"

env_setup "${1:-}"
label="${2:-}"
[[ -z "$label" || "$label" =~ ^[a-z0-9-]+$ ]] || die "label may hold a-z, 0-9 and -, got '$label'"
check="$GENESY_ENV-backup"
trap 'notify "$check" fail "line $LINENO"' ERR
notify "$check" start

out="$BACKUPS/$GENESY_ENV"
install -d -m 700 "$out"
umask 077
name="genesy-$GENESY_ENV-$(date -u +%Y%m%dT%H%M%SZ)${label:+-$label}.dump"
part="$out/$name.partial"

schema="$(schema_version)"
"${COMPOSE[@]}" exec -T db pg_dump -U genesy -d genesy -Fc > "$part"
[[ -s "$part" ]] || { rm -f "$part"; die "empty dump"; }
"${COMPOSE[@]}" exec -T db pg_restore --list < "$part" > /dev/null
printf 'schema=%s\n' "$schema" > "$out/$name.meta"
mv "$part" "$out/$name"
echo "backup: $out/$name ($(du -h "$out/$name" | cut -f1), schema $schema)"

# Retention by age (the dump just made is the newest, so one always stays).
keep_days=3
[[ "$GENESY_ENV" == production ]] && keep_days=14
find "$out" -maxdepth 1 -name "genesy-$GENESY_ENV-*.dump" -mtime +"$keep_days" -print \
  | while read -r old; do rm -f "$old" "$old.meta"; done
find "$out" -maxdepth 1 -name '*.partial' -mmin +360 -delete

# Off-site: needs both the bucket settings and an age recipient. Plaintext never leaves.
if [[ -f "$OPS_CONF/offsite.env" ]]; then
  [[ -s "$OPS_CONF/age.pub" ]] || die "offsite.env is set but $OPS_CONF/age.pub is missing: refusing to upload plaintext"
  # S3_ENDPOINT (https://s3.<region>.backblazeb2.com), S3_BUCKET, S3_REGION, S3_PREFIX.
  . "$OPS_CONF/offsite.env"
  remote="${S3_PREFIX:-db-backups}/$GENESY_ENV/$name.age"
  enc="$out/$name.age"
  age -R "$OPS_CONF/age.pub" -o "$enc" "$out/$name"
  # Key id and secret go to curl on stdin (a config file), never on the command line.
  printf 'user = "%s"\n' "$(tr -d '[:space:]' < "$OPS_CONF/offsite_key")" \
    | curl -fsS -m 600 --retry 3 -K - --aws-sigv4 "aws:amz:$S3_REGION:s3" \
        -T "$enc" "$S3_ENDPOINT/$S3_BUCKET/$remote" -o /dev/null
  rm -f "$enc"
  echo "backup: copied off-site as $remote"
else
  echo "backup: offsite not configured, local copy only"
fi
notify "$check" ok "$name"
