#!/usr/bin/env bash
# Tests for deploy.sh: a real git checkout, stub docker/curl/sudo/flock/df and backup/restore
# scripts on PATH:  bash deploy/deploy.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# The checkout: two commits; the env file is untracked, as on the VPS.
root="$t/root" d="$t/root/deploy/production"
mkdir -p "$d" "$t/bin" "$t/opslib"
cp "$HERE/production/docker-compose.production.yml" "$HERE/production/docker-compose.stripe.yml" \
  "$HERE/production/docker-compose.uploads.yml" "$d/"
g() { git -C "$root" -c user.email=t@t -c user.name=t "$@"; }
g init -q && g add . && g commit -qm one && A="$(g rev-parse --short HEAD)"
echo "# two" >> "$d/docker-compose.production.yml" && g commit -qam two && B="$(g rev-parse --short HEAD)"
printf 'TRAEFIK_NETWORK=traefik\nGENESY_REF=\nMAIL_FROM=x\n' > "$d/.env.production"

echo 20261101000000 > "$t/schema"
cat > "$t/bin/docker" << 'EOF'
#!/usr/bin/env bash
echo "docker $*" >> "$CALLS"
if [[ "$1" == compose ]]; then
  while (( $# )) && [[ "$1" != config && "$1" != exec && "$1" != run ]]; do
    [[ "$1" == --env-file ]] && env_file="$2"
    shift
  done
  case "${1:-}" in
    config) ref="$(sed -n 's/^GENESY_REF=//p' "$env_file")"
      printf 'postgres:18@sha256:abc\ngenesy-prod/api:%s\ngenesy-prod/shop:%s\n' "$ref" "$ref" ;;
    exec) [[ "$*" == *to_regclass* ]] && echo t || cat "$T/schema" ;;
    run) [[ -n "${STUB_MIGRATE_TO:-}" ]] && echo "$STUB_MIGRATE_TO" > "$T/schema" ;;
  esac
  exit 0
fi
case "$1 $2" in
  "image ls") cat "$T/tags" 2> /dev/null ;;
esac
exit 0
EOF
cat > "$t/bin/curl" << 'EOF'
#!/usr/bin/env bash
echo "curl $*" >> "$CALLS"
printf '%s' "${STUB_CODE:-200}"
EOF
printf '#!/usr/bin/env bash\n[[ "$1" == -n ]] && shift\nexec "$@"\n' > "$t/bin/sudo"
printf '#!/usr/bin/env bash\n[[ -z "${STUB_LOCKED:-}" ]]\n' > "$t/bin/flock"
printf '#!/usr/bin/env bash\necho "Filesystem 1024-blocks Used Available"\necho "/dev/x 1 1 ${STUB_FREE_KB:-99999999} 1%% /"\n' > "$t/bin/df"
cat > "$t/opslib/backup.sh" << 'EOF'
#!/usr/bin/env bash
echo "backup.sh $*" >> "$CALLS"
echo "backup: /srv/genesy-backups/$1/genesy-$1-X-$2.dump (1K, schema 1)"
EOF
printf '#!/usr/bin/env bash\necho "restore.sh $*" >> "$CALLS"\n' > "$t/opslib/restore.sh"
chmod +x "$t/bin/"* "$t/opslib/"*

export CALLS="$t/calls" T="$t" PATH="$t/bin:$PATH" GENESY_ROOT="$root" GENESY_OPS_LIB="$t/opslib" HEALTH_WAIT=0
run() { : > "$CALLS"; bash ${TRACE:-} "$HERE/deploy.sh" production "$@" > "$t/out" 2>&1; }
state() { sed -n "s/^$1=//p" "$d/.deploy-state"; }
line() { grep -n -m1 -- "$1" "$CALLS" | cut -d: -f1; }
before() { local a b; a="$(line "$1")" b="$(line "$2")"; [[ -n "$a" && -n "$b" && "$a" -lt "$b" ]]; }

# First deploy: images → backup → migrate → up → health, in that order.
run "$A" || fail "deploy A: $(cat "$t/out")"
before "compose .* build" "backup.sh production pre-$A" || fail "build not before backup"
before "backup.sh production pre-$A" "run --rm cli migrate" || fail "backup not before migrate"
before "run --rm cli migrate" "up -d --wait --wait-timeout 300" || fail "migrate not before up"
before "up -d --wait --wait-timeout 300" "curl " || fail "health not after up"
grep -q "curl .*--resolve api.genesy.app:443:127.0.0.1 https://api.genesy.app/healthz" "$CALLS" || fail "api health not checked"
[[ "$(grep -c '^curl' "$CALLS")" == 6 ]] || fail "not every host checked"
grep -qx "GENESY_REF=$A" "$d/.env.production" || fail "GENESY_REF not set"
[[ "$(state live)" == "$A" && -z "$(state previous)" ]] || fail "state after A: $(cat "$d/.deploy-state")"
if grep -q "stripe.yml" "$CALLS"; then fail "stripe overlay without its key"; fi
grep -q "$A deploy\|deploy $A ok" "$d/deploys.log" || fail "not logged"

# Second deploy with a migration: previous recorded with its schema and the pre-deploy dump.
mkdir -p "$d/secrets" && touch "$d/secrets/stripe_secret_key"
STUB_MIGRATE_TO=20261102000000 run "$B" || fail "deploy B: $(cat "$t/out")"
grep -q -- "-f $d/docker-compose.stripe.yml" "$CALLS" || fail "stripe overlay missing with its key"
[[ "$(state live)" == "$B" && "$(state previous)" == "$A" ]] || fail "state after B: $(cat "$d/.deploy-state")"
[[ "$(state previous_schema)" == 20261101000000 && "$(state live_schema)" == 20261102000000 ]] || fail "schemas: $(cat "$d/.deploy-state")"
[[ "$(state pre_deploy_dump)" == "/srv/genesy-backups/production/genesy-production-X-pre-$B.dump" ]] || fail "dump: $(state pre_deploy_dump)"

# Rollback across a schema change needs --restore-db; nothing changes without it.
if run rollback; then fail "rollback across a migration without --restore-db"; fi
grep -q "needs --restore-db" "$t/out" || fail "no reason given"
grep -qx "GENESY_REF=$B" "$d/.env.production" || fail "refused rollback changed GENESY_REF"
echo 20261101000000 > "$t/schema.after"
run rollback --restore-db || fail "rollback --restore-db: $(cat "$t/out")"
before "restore.sh production /srv/genesy-backups/production/genesy-production-X-pre-$B.dump" "up -d --wait" || fail "restore not before up"
grep -qx "GENESY_REF=$A" "$d/.env.production" || fail "GENESY_REF not back to A"
[[ "$(g rev-parse --short HEAD)" == "$A" ]] || fail "checkout not back to A"
[[ "$(state live)" == "$A" && "$(state previous)" == "$B" && -z "$(state pre_deploy_dump)" ]] || fail "state after rollback: $(cat "$d/.deploy-state")"

# Same schema: plain rollback, no restore. A redeploy of the live commit keeps previous.
echo 20261101000000 > "$t/schema"
run "$B" && run "$B" || fail "redeploy B: $(cat "$t/out")"
[[ "$(state previous)" == "$A" ]] || fail "redeploy lost previous: $(cat "$d/.deploy-state")"
run rollback || fail "plain rollback: $(cat "$t/out")"
if grep -q "restore.sh" "$CALLS"; then fail "restored without a schema change"; fi
[[ "$(state live)" == "$A" ]] || fail "plain rollback state"

# Unhealthy: exits 1 with the rollback command, state already points at the new version.
if STUB_CODE=502 run "$B"; then fail "unhealthy deploy passed"; fi
grep -q "deploy/deploy.sh production rollback" "$t/out" || fail "no rollback hint"
[[ "$(state live)" == "$B" ]] || fail "unhealthy deploy not recorded"
grep -q "deploy $B unhealthy" "$d/deploys.log" || fail "unhealthy not logged"
# The shop may answer 503 until its app key is set; nothing else may.
cat > "$t/bin/curl" << 'EOF'
#!/usr/bin/env bash
echo "curl $*" >> "$CALLS"
[[ "$*" == *shop.genesy.app* ]] && printf 503 || printf 200
EOF
run "$B" || fail "shop 503 failed the deploy: $(cat "$t/out")"

# Pull mode pulls instead of building.
echo "IMAGE_SOURCE=pull" >> "$d/.env.production"
run "$B" || fail "pull deploy: $(cat "$t/out")"
grep -q "compose .* pull" "$CALLS" || fail "pull mode did not pull"
if grep -q "compose .* build" "$CALLS"; then fail "pull mode built"; fi
sed -i.bak '/^IMAGE_SOURCE=/d' "$d/.env.production"

# Prune keeps live and previous only, never pinned images.
printf '%s\n' "$A" "$B" old1 > "$t/tags"
run prune || fail "prune: $(cat "$t/out")"
grep -q "image rm genesy-prod/api:old1" "$CALLS" || fail "old tag kept"
if grep -q "image rm genesy-prod/api:$A\|image rm genesy-prod/api:$B\|image rm postgres" "$CALLS"; then fail "pruned a kept image"; fi
grep -q "builder prune -f --max-used-space 10GB" "$CALLS" || fail "build cache not capped"

# A commit only in a bundle: refused without it, deployed with it.
other="$t/other" && git clone -q "$root" "$other"
echo "# three" >> "$other/deploy/production/docker-compose.production.yml"
git -C "$other" -c user.email=t@t -c user.name=t commit -qam three
C="$(git -C "$other" rev-parse --short HEAD)"
git -C "$other" bundle create -q "$t/c.bundle" HEAD 2> /dev/null || git -C "$other" bundle create "$t/c.bundle" HEAD 2> /dev/null
if run "$C"; then fail "unknown commit deployed"; fi
grep -q "pass --bundle" "$t/out" || fail "no bundle hint"
run "$C" --bundle "$t/c.bundle" || fail "bundle deploy: $(cat "$t/out")"
[[ "$(state live)" == "$C" ]] || fail "bundle commit not live"

# Guards.
if run nothing-like-a-sha; then fail "bad sha taken"; fi
if STUB_FREE_KB=1000 run "$B"; then fail "deployed on a full disk"; fi
if STUB_LOCKED=1 run "$B"; then fail "deployed while locked"; fi
if bash "$HERE/deploy.sh" nowhere "$B" > /dev/null 2>&1; then fail "unknown env taken"; fi
run compose ps || fail "compose pass-through: $(cat "$t/out")"
grep -q "^docker compose -f $d/docker-compose.production.yml -f $d/docker-compose.stripe.yml --env-file $d/.env.production ps$" "$CALLS" || fail "pass-through: $(cat "$CALLS")"
touch "$d/secrets/b2_uploads_key"
run compose ps || fail "compose with uploads: $(cat "$t/out")"
grep -q "^docker compose -f $d/docker-compose.production.yml -f $d/docker-compose.stripe.yml -f $d/docker-compose.uploads.yml --env-file" "$CALLS" \
  || fail "uploads overlay not added with its key: $(cat "$CALLS")"

echo "deploy.test.sh: all passed"
