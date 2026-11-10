#!/usr/bin/env bash
# Tests for backup.sh, restore-test.sh and restore.sh with stub docker/age/curl on PATH:
#   bash deploy/ops/backup.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
mode() { stat -c %a "$1" 2> /dev/null || stat -f %Lp "$1"; }

mkdir -p "$t/bin" "$t/conf"
for env in production staging; do
  mkdir -p "$t/root-$env/deploy/$env"
  cp "$REPO/deploy/$env/docker-compose.$env.yml" "$t/root-$env/deploy/$env/"
  cp "$REPO/deploy/assistant-role.sql" "$t/root-$env/deploy/"
  echo "GENESY_REF=abc1234" > "$t/root-$env/deploy/$env/.env.$env"
done

# docker: compose exec on db answers psql/pg_dump/pg_restore; plain docker plays the throwaway
# Postgres. Every call is logged; STUB_* tune the answers.
cat > "$t/bin/docker" << 'EOF'
#!/usr/bin/env bash
echo "docker $*" >> "$CALLS"
if [[ "$1" == compose ]]; then
  while (( $# )) && [[ "$1" != exec && "$1" != stop ]]; do shift; done
  [[ "${1:-}" == exec ]] || exit 0
  shift 3   # exec -T db
  case "$1" in
    psql)
      case "$*" in
        *checkpoint_mode=verify*) [[ -z "${STUB_VERIFY_FAIL:-}" ]] || exit 1 ;;
        *to_regclass*|*to_regnamespace*) echo t ;;
        *) echo "${STUB_SCHEMA:-20261101000000}" ;;
      esac ;;
    pg_dump) printf '%s' "${STUB_DUMP-PGDMP-fake}" ;;
    pg_restore) grep -q PGDMP || exit 1 ;;
  esac
  exit 0
fi
case "$1" in
  exec)
    case "$*" in
      *checkpoint_mode=verify*) [[ -z "${STUB_VERIFY_FAIL:-}" ]] || exit 1 ;;
      *to_regnamespace*) echo t ;;
      *pg_isready*) ;;
      *pg_restore*) cat > /dev/null ;;
      *_sqlx_migrations*) echo "${STUB_RESTORED:-20261101000000}" ;;
      *workspaces*) echo "${STUB_WORKSPACES:-1}" ;;
    esac ;;
  run) echo container-id ;;
esac
EOF
cat > "$t/bin/age" << 'EOF'
#!/usr/bin/env bash
echo "age $*" >> "$CALLS"
while (( $# )); do [[ "$1" == -o ]] && { echo encrypted > "$2"; shift; }; shift; done
EOF
# curl: logs its arguments and what came on stdin (the key must only ever be there).
cat > "$t/bin/curl" << 'EOF'
#!/usr/bin/env bash
echo "curl $*" >> "$CALLS"
[[ " $* " == *" -K - "* ]] && sed 's/^/curl-stdin /' >> "$CALLS"
exit 0
EOF
chmod +x "$t/bin/"*

export CALLS="$t/calls" GENESY_OPS_CONF="$t/conf" GENESY_BACKUPS="$t/backups" PATH="$t/bin:$PATH"
run() { # run <script> <env> [args]: output in $t/out
  local script="$1" env="$2"; shift 2
  GENESY_ROOT="$t/root-$env" bash "$HERE/$script" "$env" "$@" > "$t/out" 2>&1
}
: > "$CALLS"

# --- backup.sh ---
run backup.sh production || fail "backup refused: $(cat "$t/out")"
dump="$(ls "$t/backups/production"/genesy-production-*.dump)"
[[ "$(cat "$dump")" == PGDMP-fake ]] || fail "dump content"
[[ "$(cat "$dump.meta")" == schema=20261101000000 ]] || fail "meta: $(cat "$dump.meta")"
[[ "$(mode "$t/backups/production")" == 700 ]] || fail "dir mode $(mode "$t/backups/production")"
[[ "$(mode "$dump")" == 600 ]] || fail "dump mode $(mode "$dump")"
grep -q "offsite not configured" "$t/out" || fail "no local-only note"
if grep -q "^curl" "$CALLS"; then fail "curl called without ping or offsite config"; fi
grep -q "exec -T db pg_dump -U genesy -d genesy -Fc" "$CALLS" || fail "pg_dump not custom format"

# The compose chain follows the files: Stripe on when its key file exists.
mkdir -p "$t/root-production/deploy/production/secrets" && touch "$t/root-production/deploy/production/secrets/stripe_secret_key"
: > "$CALLS"
run backup.sh production label-1 || fail "labelled backup refused: $(cat "$t/out")"
grep -q -- "-f $t/root-production/deploy/production/docker-compose.stripe.yml --env-file" "$CALLS" || fail "stripe overlay missing"
ls "$t/backups/production"/*-label-1.dump > /dev/null || fail "label not in name"

# Retention: staging keeps 3 days, production 14.
touch -t 202001010000 "$t/backups/production/genesy-production-20200101T000000Z.dump"
mkdir -p "$t/backups/staging"
touch -t 202001010000 "$t/backups/staging/genesy-staging-20200101T000000Z.dump" "$t/backups/staging/genesy-staging-20200101T000000Z.dump.meta"
run backup.sh staging || fail "staging backup refused: $(cat "$t/out")"
[[ ! -e "$t/backups/staging/genesy-staging-20200101T000000Z.dump" ]] || fail "old staging dump kept"
[[ ! -e "$t/backups/staging/genesy-staging-20200101T000000Z.dump.meta" ]] || fail "old meta kept"
[[ -e "$t/backups/production/genesy-production-20200101T000000Z.dump" ]] || fail "staging run touched production"

# A dump that fails pg_restore --list never becomes a .dump, and a fail ping goes out.
echo "https://hc.test/key" > "$t/conf/ping_base"
: > "$CALLS"
dumps() { find "$t/backups/staging" -name '*.dump' | wc -l | tr -d ' '; }
before="$(dumps)"
if STUB_DUMP=garbage run backup.sh staging; then fail "broken dump accepted"; fi
[[ "$(dumps)" == "$before" ]] || fail "broken dump kept as .dump"
grep -q "curl .*https://hc.test/key/staging-backup/fail" "$CALLS" || fail "no fail ping"
grep -q "curl .*https://hc.test/key/staging-backup/start" "$CALLS" || fail "no start ping"

if run backup.sh staging 'Bad Label'; then fail "bad label taken"; fi
if run backup.sh nowhere; then fail "unknown env taken"; fi

# Off-site set but no age key: refuses, uploads nothing.
printf 'S3_ENDPOINT=https://s3.eu.test\nS3_BUCKET=bkt\nS3_REGION=eu\nS3_PREFIX=db-backups\n' > "$t/conf/offsite.env"
echo "keyid:SECRETKEY123" > "$t/conf/offsite_key"
: > "$CALLS"
if run backup.sh production; then fail "offsite without age accepted"; fi
grep -q "refusing to upload plaintext" "$t/out" || fail "no plaintext refusal"
if grep -q "^curl .* -T " "$CALLS"; then fail "uploaded without encryption"; fi

# Off-site with age: encrypted copy uploaded, key only on curl's stdin, .age removed.
echo "age1testrecipient" > "$t/conf/age.pub"
: > "$CALLS"
run backup.sh production || fail "offsite backup refused: $(cat "$t/out")"
grep -q "^age -R $t/conf/age.pub" "$CALLS" || fail "age not used"
grep -q "curl .*--aws-sigv4 aws:amz:eu:s3 -T .*\.dump\.age https://s3.eu.test/bkt/db-backups/production/genesy-production-.*\.dump\.age" "$CALLS" || fail "upload: $(grep curl "$CALLS")"
grep -q '^curl-stdin user = "keyid:SECRETKEY123"' "$CALLS" || fail "key not passed on stdin"
if grep "^curl " "$CALLS" | grep -q SECRETKEY123; then fail "key on the command line"; fi
if grep -q SECRETKEY123 "$t/out"; then fail "key echoed"; fi
if ls "$t/backups/production"/*.age 2> /dev/null; then fail ".age left behind"; fi
rm -f "$t/conf/offsite.env" "$t/conf/age.pub" "$t/conf/offsite_key" "$t/conf/ping_base"

# --- restore-test.sh ---
: > "$CALLS"
run restore-test.sh production || fail "restore test failed: $(cat "$t/out")"
grep -q "docker run -d --name genesy-restore-test-production-.* --network none .*postgres:18" "$CALLS" || fail "throwaway not isolated or wrong image"
grep -q "docker rm -fv genesy-restore-test-production-" "$CALLS" || fail "throwaway not removed"
identities_line="$(grep -n 'checkpoint_mode=identities' "$CALLS" | head -1 | cut -d: -f1 || true)"
restore_test_line="$(grep -n 'pg_restore -U genesy -d genesy --no-owner' "$CALLS" | head -1 | cut -d: -f1)"
[[ -n "$identities_line" && "$identities_line" -lt "$restore_test_line" ]] || fail 'roles not bootstrapped before ACL restore'
verify_line="$(grep -n 'checkpoint_mode=verify' "$CALLS" | head -1 | cut -d: -f1 || true)"
[[ -n "$verify_line" && "$verify_line" -gt "$restore_test_line" ]] || fail 'restored grants not verified'
if STUB_VERIFY_FAIL=1 run restore-test.sh production; then fail 'unsafe restored grants accepted'; fi
if STUB_RESTORED=20250101000000 run restore-test.sh production; then fail "older schema passed"; fi
if STUB_WORKSPACES=0 run restore-test.sh production; then fail "empty production passed"; fi
STUB_WORKSPACES=0 run restore-test.sh staging || fail "staging needs no workspace: $(cat "$t/out")"
for f in "$t/backups/staging"/*.dump; do touch -t 202001010000 "$f"; done
if run restore-test.sh staging; then fail "stale dump passed"; fi
grep -q "older than 26 h" "$t/out" || fail "stale reason missing"
mkdir -p "$t/backups/empty"
if GENESY_BACKUPS="$t/backups/empty" run restore-test.sh staging; then fail "no dump passed"; fi

# --- restore.sh ---
: > "$CALLS"
if echo "yes" | run restore.sh production "$dump"; then fail "production restore without the words"; fi
if grep -q "pg_restore -U genesy -d genesy --clean" "$CALLS"; then fail "restored anyway"; fi
if echo "restore production" | run restore.sh production /etc/hosts; then fail "dump outside backups taken"; fi
echo "restore production" | run restore.sh production "$dump" || fail "restore refused: $(cat "$t/out")"
grep -n "" "$CALLS" | grep -m1 "pg_dump" > /dev/null || fail "no pre-restore backup"
ls "$t/backups/production"/*-pre-restore.dump > /dev/null || fail "pre-restore dump missing"
stop_line="$(grep -n "stop api shop office till landing field flow" "$CALLS" | cut -d: -f1)"
restore_line="$(grep -n "pg_restore -U genesy -d genesy --clean --if-exists --single-transaction" "$CALLS" | cut -d: -f1)"
[[ -n "$stop_line" && -n "$restore_line" && "$stop_line" -lt "$restore_line" ]] || fail "apps not stopped before restore"

echo "backup.test.sh: all passed"
