#!/usr/bin/env bash
# restore-test.sh <staging|production>: proves the newest dump restores. Restores it into a
# throwaway Postgres (same pinned image as the stack, no network, removed afterwards) and runs
# sanity queries. Fails when the newest dump is older than 26 h.
set -Eeuo pipefail
. "$(dirname "$(readlink -f "$0")")/lib.sh"

env_setup "${1:-}"
check="$GENESY_ENV-restore-test"
trap 'notify "$check" fail "line $LINENO"' ERR
notify "$check" start

dump="$(latest_dump "$GENESY_ENV")"
[[ -n "$dump" ]] || die "no dump in $BACKUPS/$GENESY_ENV"
[[ -n "$(find "$dump" -mmin -1560)" ]] || die "newest dump older than 26 h: $(basename "$dump")"
want="$(sed -n 's/^schema=//p' "$dump.meta" 2> /dev/null)"
image="$(sed -n 's/^ *image: \(postgres:[^$]*@sha256:[0-9a-f]*\)$/\1/p' "$DIR/docker-compose.$GENESY_ENV.yml" | head -1)"
[[ -n "$image" ]] || die "no pinned postgres image in the compose file"

name="genesy-restore-test-$GENESY_ENV-$$"
trap 'docker rm -fv "$name" > /dev/null 2>&1 || true' EXIT
docker run -d --name "$name" --network none --memory 1g --cpus 1 \
  -e POSTGRES_USER=genesy -e POSTGRES_DB=genesy -e POSTGRES_HOST_AUTH_METHOD=trust \
  "$image" > /dev/null
ready=0
for _ in $(seq 60); do
  # The image's init restarts the server once; ask twice in a row.
  if docker exec "$name" pg_isready -U genesy -d genesy -q && sleep 1 \
    && docker exec "$name" pg_isready -U genesy -d genesy -q; then ready=1; break; fi
  sleep 1
done
(( ready )) || die "throwaway postgres did not start"

docker exec -i "$name" pg_restore -U genesy -d genesy --no-owner --exit-on-error < "$dump"
sql() { docker exec "$name" psql -U genesy -d genesy -Atc "$1"; }
got="$(sql "SELECT coalesce(max(version), 0) FROM _sqlx_migrations WHERE success")"
(( got > 0 )) || die "restored database has no migrations"
# The dump is taken after the version is read, so it can only be at or past it.
[[ -z "$want" ]] || (( got >= want )) || die "restored schema $got is behind the recorded $want"
if [[ "$GENESY_ENV" == production ]]; then
  (( $(sql "SELECT count(*) FROM workspaces") >= 1 )) || die "restored production has no workspace"
fi
echo "restore-test: $(basename "$dump") restored, schema $got"
notify "$check" ok "$(basename "$dump") schema $got"
