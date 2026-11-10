#!/usr/bin/env bash
# restore.sh <staging|production> <dump file>: replaces the LIVE database with a dump.
# Takes a fresh backup first (label pre-restore), stops every app container, restores in one
# transaction, and leaves the apps stopped: the caller starts the version it wants
# (deploy.sh rollback, or `deploy.sh compose <env> up -d --wait`).
# Production asks for the typed words "restore production". Runs as root (dumps are root-only).
set -Eeuo pipefail
here="$(dirname "$(readlink -f "$0")")"
. "$here/lib.sh"

env_setup "${1:-}"
dump="${2:-}"
[[ -f "$dump" && "$(readlink -f "$dump")" == "$(readlink -f "$BACKUPS/$GENESY_ENV")"/* ]] \
  || die "dump must be a file in $BACKUPS/$GENESY_ENV, got '$dump'"
if [[ "$GENESY_ENV" == production ]]; then
  echo "This replaces the production database with $(basename "$dump")."
  echo "Everything written after that dump is lost (a pre-restore backup is taken first)."
  read -r -p 'Type "restore production" to go on: ' answer
  [[ "$answer" == "restore production" ]] || die "not confirmed, nothing changed"
fi

"$here/backup.sh" "$GENESY_ENV" pre-restore
"${COMPOSE[@]}" stop api shop office till landing field flow
checkpoint_roles identities "${COMPOSE[@]}" exec -T db
"${COMPOSE[@]}" exec -T db pg_restore -U genesy -d genesy --clean --if-exists \
  --single-transaction --exit-on-error < "$dump"
if [[ "$("${COMPOSE[@]}" exec -T db psql -U genesy -d genesy -Atc "SELECT to_regnamespace('assistant_checkpoint') IS NOT NULL")" == t ]]; then
  checkpoint_roles verify "${COMPOSE[@]}" exec -T db
fi
echo "restore: $(basename "$dump") is now the $GENESY_ENV database; apps are stopped"
