#!/usr/bin/env bash
# erp-rehearsal.sh --database-url postgres://USER:PASS@127.0.0.1:PORT/postgres [--api-bin PATH]
#                  [--pg-container NAME] [--keep]
# --pg-container: run pg_dump/pg_restore inside that local container (its server on 5432) when the
# local client tools are older than the server (e.g. genesy-dev-pg).
#
# Flow ERP M8 release rehearsal, LOCAL ONLY. Proves that a populated pre-ERP database migrates
# forward and that the ERP read routes answer on it, and that a dump of it restores:
#   1. scratch DB "src": every migration, then `seed-demo` (populated: AFM + RUAHA demo data)
#   2. revert src to the last pre-ERP migration (20261114020000): the populated pre-ERP shape
#   3. pg_dump src → pg_restore into scratch DB "dst" (the restore rehearsal)
#   4. `verzo-360-api migrate` on dst twice: forward to the newest, then a no-op
#   5. pre-ERP row counts survive; every workspace has its book row (state setup)
#   6. the API on dst (127.0.0.1:18490): log in as a seeded owner, ERP read routes answer 200
#   7. drop both scratch DBs (--keep leaves them for inspection)
# Refuses any non-loopback host and any maintenance DB other than "postgres": the scratch DBs
# are created and dropped on that local server only. It never activates a book and never
# reads deploy/<env>/secrets or reaches staging/production.
set -Eeuo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PRE_ERP=20261114020000
PORT=18490
die() { echo "erp-rehearsal: $*" >&2; exit 1; }

url="" api_bin="" container="" keep=0
while (($#)); do
  case "$1" in
    --database-url) url="${2:-}"; shift 2 ;;
    --api-bin) api_bin="${2:-}"; shift 2 ;;
    --pg-container) container="${2:-}"; shift 2 ;;
    --keep) keep=1; shift ;;
    *) die "unknown argument: $1" ;;
  esac
done
[[ -n "$url" ]] || die "usage: --database-url postgres://USER:PASS@127.0.0.1:PORT/postgres"
# postgres://user:pass@host:port/db
re='^postgres(ql)?://([^@/]+@)?([^:/?]+)(:[0-9]+)?/([^/?]+)$'
[[ "$url" =~ $re ]] || die "not a postgres URL with a database name"
host="${BASH_REMATCH[3]}" db="${BASH_REMATCH[5]}"
[[ "$host" == 127.0.0.1 || "$host" == localhost || "$host" == "[::1]" ]] \
  || die "refusing host $host: the rehearsal runs on a local server only"
[[ "$db" == postgres ]] || die "refusing maintenance database $db: pass .../postgres"
server="${url%/postgres}"

api_bin="${api_bin:-${CARGO_TARGET_DIR:-$REPO/apps/api/target}/debug/verzo-360-api}"
[[ -x "$api_bin" ]] || die "no API binary at $api_bin (cargo build first, or --api-bin)"
for tool in psql sqlx python3 curl; do
  command -v "$tool" > /dev/null || die "$tool not found"
done

admin() { psql "$url" -v ON_ERROR_STOP=1 -Atqc "$1"; }
server_major="$(admin "SHOW server_version_num")"; server_major=$((server_major / 10000))
# pg_dump/pg_restore: local, or inside the container (same server, its own port 5432).
inner() { sed -E 's#@[^/@]+/#@127.0.0.1:5432/#' <<< "$1"; }
if [[ -n "$container" ]]; then
  [[ "$(docker inspect -f '{{.State.Running}}' "$container" 2> /dev/null)" == true ]] \
    || die "container $container is not running"
  dump_to() { docker exec "$container" pg_dump -Fc -d "$(inner "$1")" > "$2"; }
  restore_from() { docker exec -i "$container" pg_restore --no-owner --exit-on-error -d "$(inner "$1")" < "$2"; }
  dump_major="$(docker exec "$container" pg_dump --version | sed -E 's/[^0-9]*([0-9]+).*/\1/')"
else
  dump_to() { pg_dump -Fc -d "$1" -f "$2"; }
  restore_from() { pg_restore --no-owner --exit-on-error -d "$1" "$2"; }
  dump_major="$(pg_dump --version | sed -E 's/[^0-9]*([0-9]+).*/\1/')"
fi
((dump_major >= server_major)) \
  || die "pg_dump $dump_major is older than the server ($server_major): pass --pg-container"

tag="erp_rehearsal_$$"
src="$server/${tag}_src" dst="$server/${tag}_dst"
dump="$(mktemp -d)/src.dump"
api_pid=""
cleanup() {
  [[ -z "$api_pid" ]] || kill "$api_pid" 2> /dev/null || true
  rm -rf "$(dirname "$dump")"
  if ((keep)); then echo "erp-rehearsal: kept ${tag}_src and ${tag}_dst"; return; fi
  admin "DROP DATABASE IF EXISTS ${tag}_src WITH (FORCE)" || true
  admin "DROP DATABASE IF EXISTS ${tag}_dst WITH (FORCE)" || true
}
trap cleanup EXIT

step() { echo "erp-rehearsal: $*"; }
# Throwaway per run: seals the demo's webhook/mail secrets in the scratch DBs only.
creds="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
apienv() {
  env DATABASE_URL="$1" BIND_ADDR="127.0.0.1:$PORT" RUST_LOG=error \
    FRONTEND_URL="http://127.0.0.1:$PORT" API_PUBLIC_URL="http://127.0.0.1:$PORT" \
    SMTP_URL="smtp://127.0.0.1:9" SMTP_ALLOW_INSECURE=true MAIL_FROM="Rehearsal <no-reply@rehearsal.test>" \
    PAYMENTS_PROVIDER=fake INTEGRATIONS_PROVIDERS=fake GENESY_ALLOW_DEMO_SEED=1 \
    SIGNUP_ENABLED=true PRODUCT_BULK_ENABLED=true SKIP_BOOT_MIGRATIONS=true \
    RATE_LIMIT_BURST=10000 RATE_LIMIT_REPLENISH_SECS=1 PRIVATE_RATE_LIMIT_BURST=10000 \
    PRIVATE_IP_RATE_LIMIT_BURST=100000 INVITES_PER_HOUR=10000 CREDENTIALS_KEY="$creds" "${@:2}"
}
counts() {
  psql "$1" -Atqc "SELECT (SELECT count(*) FROM workspaces) || ' ' || (SELECT count(*) FROM products)
                       || ' ' || (SELECT count(*) FROM invoices) || ' ' || (SELECT count(*) FROM orders)
                       || ' ' || (SELECT count(*) FROM stocks)"
}

step "1/7 populated source: migrate + seed-demo"
admin "CREATE DATABASE ${tag}_src"
apienv "$src" "$api_bin" migrate > /dev/null
seed="$(apienv "$src" "$api_bin" seed-demo)"
# First owner login the seed printed: "app | role | login | secret | path".
read -r login secret < <(printf '%s\n' "$seed" \
  | awk -F'|' '$1 ~ /^office/ && $2 ~ /owner/ {gsub(/ /,"",$3); gsub(/ /,"",$4); print $3, $4; exit}') || true
[[ -n "${login:-}" ]] || die "seed-demo printed no office owner"

step "2/7 revert to the pre-ERP schema ($PRE_ERP)"
sqlx migrate revert --target-version "$PRE_ERP" --source "$REPO/apps/api/migrations" \
  --database-url "$src" > /dev/null
[[ "$(psql "$src" -Atqc "SELECT to_regclass('accounting_settings') IS NULL")" == t ]] \
  || die "revert left accounting_settings in place"
before="$(counts "$src")"

step "3/7 dump and restore into a second scratch database"
dump_to "$src" "$dump"
admin "CREATE DATABASE ${tag}_dst"
restore_from "$dst" "$dump"

step "4/7 migrate forward, then again (no-op)"
apienv "$dst" "$api_bin" migrate > /dev/null
first="$(psql "$dst" -Atqc "SELECT count(*) || ':' || max(version) FROM _sqlx_migrations WHERE success")"
apienv "$dst" "$api_bin" migrate > /dev/null
again="$(psql "$dst" -Atqc "SELECT count(*) || ':' || max(version) FROM _sqlx_migrations WHERE success")"
[[ "$first" == "$again" ]] || die "second migrate changed the schema: $first → $again"
newest="$(ls "$REPO/apps/api/migrations" | sed -n 's/^\([0-9]*\)_.*\.up\.sql$/\1/p' | sort | tail -1)"
[[ "${first#*:}" == "$newest" ]] || die "dst is at ${first#*:}, newest migration is $newest"

step "5/7 pre-ERP data kept, one setup book per workspace"
after="$(counts "$dst")"
[[ "$before" == "$after" ]] || die "row counts changed (workspaces products invoices orders stocks): $before → $after"
books="$(psql "$dst" -Atqc "SELECT count(*) FILTER (WHERE s.state = 'setup') || '/' || count(*)
                            FROM workspaces w LEFT JOIN accounting_settings s ON s.workspace_id = w.id")"
[[ "${books%/*}" == "${books#*/}" ]] || die "workspaces without a setup book: $books"

step "6/7 ERP read routes on the migrated copy"
apienv "$dst" "$api_bin" > /dev/null 2>&1 &
api_pid=$!
for _ in $(seq 60); do curl -fs "http://127.0.0.1:$PORT/healthz" > /dev/null && break; sleep 1; done
curl -fs "http://127.0.0.1:$PORT/healthz" > /dev/null || die "API did not start"
session="$(LOGIN="$login" SECRET="$secret" python3 -c '
import json, os
print(json.dumps({"email": os.environ["LOGIN"], "password": os.environ["SECRET"]}))' \
  | curl -fs -H 'content-type: application/json' --data-binary @- "http://127.0.0.1:$PORT/v1/login")" \
  || die "seeded owner could not log in"
read -r token ws < <(printf '%s' "$session" | python3 -c '
import json, sys
d = json.load(sys.stdin)["data"]; print(d["access_token"], d["workspace_ids"][0])')
for route in setup accounts journals fiscal-years periods exceptions opening-baselines \
  bank-accounts; do
  code="$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" \
    "http://127.0.0.1:$PORT/v1/workspace/$ws/accounting/$route")"
  [[ "$code" == 200 ]] || die "GET accounting/$route answered $code"
  echo "  accounting/$route 200"
done

step "7/7 ok: pre-ERP rows $before kept, migrations $first, ERP reads answer"
