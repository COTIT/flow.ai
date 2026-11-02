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
g init -q && g add . && g commit -qm one && A="$(g rev-parse HEAD)"
echo "# two" >> "$d/docker-compose.production.yml" && g commit -qam two && B="$(g rev-parse HEAD)"
printf 'TRAEFIK_NETWORK=traefik\nGENESY_REF=\nMAIL_FROM=x\n' > "$d/.env.production"

echo 20261101000000 > "$t/schema"
cat > "$t/bin/docker" << 'EOF'
#!/usr/bin/env bash
echo "docker $*" >> "$CALLS"
if [[ "$1" == compose ]]; then
  while (( $# )) && [[ "$1" != config && "$1" != exec && "$1" != run && "$1" != pull && "$1" != ps ]]; do
    [[ "$1" == --env-file ]] && env_file="$2"
    shift
  done
  case "${1:-}" in
    config) [[ "$2" == --services ]] && { printf 'api\ndb\nshop\n'; exit 0; }
      ref="$(sed -n 's/^GENESY_REF=//p' "$env_file")"
      printf 'postgres:18@sha256:abc\nghcr.io/cotit/genesy-api:%s\nghcr.io/cotit/genesy-shop:%s\n' "$ref" "$ref" ;;
    pull) [[ -z "${STUB_PULL_FAIL:-}" ]] || exit 1 ;;
    exec) [[ "$*" == *to_regclass* ]] && echo t || cat "$T/schema" ;;
    run) [[ -n "${STUB_MIGRATE_TO:-}" ]] && echo "$STUB_MIGRATE_TO" > "$T/schema" ;;
    ps) cat "$T/ps" 2> /dev/null ;;
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

# First deploy (given a short sha): pull → backup → migrate → up --no-build → health, in that order.
run "${A:0:7}" || fail "deploy A: $(cat "$t/out")"
before "compose .* pull" "backup.sh production pre-$A" || fail "pull not before backup"
if grep -q "compose .* build" "$CALLS"; then fail "pull mode built"; fi
grep -q "up -d --wait --wait-timeout 300 --no-build" "$CALLS" || fail "up may build after a pull"
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
[[ "$(g rev-parse HEAD)" == "$A" ]] || fail "checkout not back to A"
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

# A failed pull stops before the backup; nothing changes.
if STUB_PULL_FAIL=1 run "$B"; then fail "deployed after a failed pull"; fi
if grep -q "backup.sh" "$CALLS"; then fail "backup after a failed pull"; fi

# Build fallback builds instead of pulling, and up may then use the built images.
echo "IMAGE_SOURCE=build" >> "$d/.env.production"
run "$B" || fail "build deploy: $(cat "$t/out")"
grep -q "compose .* build" "$CALLS" || fail "build mode did not build"
if grep -q "compose .* pull" "$CALLS"; then fail "build mode pulled"; fi
if grep -q -- "--no-build" "$CALLS"; then fail "build mode passed --no-build"; fi
sed -i.bak '/^IMAGE_SOURCE=/d' "$d/.env.production"

# Prune keeps live and previous of BOTH envs (shared GHCR repos; staging's -staging tags too),
# never pinned images.
sd="$root/deploy/staging" && mkdir -p "$sd"
echo "GENESY_REF=s1" > "$sd/.env.staging"
printf 'live=s1\nprevious=\n' > "$sd/.deploy-state"
printf '%s\n' "$A" "$B" old1 s1 s1-staging old2-staging > "$t/tags"
run prune || fail "prune: $(cat "$t/out")"
grep -q "image rm ghcr.io/cotit/genesy-api:old1$" "$CALLS" || fail "old tag kept"
grep -q "image rm ghcr.io/cotit/genesy-api:old2-staging$" "$CALLS" || fail "old staging tag kept"
for kept in "$A" "$B" s1 s1-staging; do
  if grep -q "image rm ghcr.io/cotit/genesy-api:$kept$" "$CALLS"; then fail "pruned kept tag $kept"; fi
done
if grep -q "image rm postgres" "$CALLS"; then fail "pruned a pinned image"; fi
grep -q "builder prune -f --max-used-space 10GB" "$CALLS" || fail "build cache not capped"

# A commit only in a bundle: refused without it, deployed with it.
other="$t/other" && git clone -q "$root" "$other"
echo "# three" >> "$other/deploy/production/docker-compose.production.yml"
git -C "$other" -c user.email=t@t -c user.name=t commit -qam three
C="$(git -C "$other" rev-parse HEAD)"
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

# No DNS yet (VPS-only nodns overlay sets traefik.enable=false): every host would answer 404, so
# health is the containers' own state and no Traefik probe runs. Rollback uses the same check.
touch "$d/docker-compose.nodns.yml"
printf 'api running healthy\ndb running healthy\nshop running \n' > "$t/ps"
run "$B" || fail "nodns deploy: $(cat "$t/out")"
grep -q "health: Traefik routes off (no DNS yet); checked container health only" "$t/out" || fail "no routes-off line: $(cat "$t/out")"
if grep -q "^curl" "$CALLS"; then fail "probed Traefik under nodns"; fi
printf 'api running unhealthy\ndb exited \n' > "$t/ps"
if run "$B"; then fail "unhealthy containers passed under nodns"; fi
grep -q "health: failing: api=running/unhealthy db=exited shop=missing" "$t/out" || fail "nodns failure: $(cat "$t/out")"
if run rollback; then fail "unhealthy rollback passed under nodns"; fi
printf 'api running healthy\ndb running healthy\nshop running healthy\n' > "$t/ps"
run rollback || fail "nodns rollback: $(cat "$t/out")"
grep -q "routes off" "$t/out" || fail "rollback probed Traefik under nodns"
if grep -q "^curl" "$CALLS"; then fail "rollback probed Traefik under nodns"; fi

echo "deploy.test.sh: all passed"
