#!/usr/bin/env bash
# Full local gate, mirroring .github/workflows/ci.yml (api, web + fake-API e2e, full-stack e2e),
# retaining build caches for the next run. Run from any checkout or worktree:
#   scripts/local-gate.sh            # everything
#   SKIP_API=1 scripts/local-gate.sh # skip a part: SKIP_API, SKIP_WEB, SKIP_E2E, SKIP_STACK
# Prints "GATE RESULT: GREEN" or "RED" and exits non-zero on red.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PG=genesy-gate-pg-$$
fail=0
stack_started=0
helpers_pid=
step() { echo "=== $1"; }
run() { "$@" || { echo "FAILED: $*"; fail=1; }; }

cleanup() {
  step cleanup
  if [[ -n "$helpers_pid" ]]; then wait "$helpers_pid"; fi
  if [[ "$stack_started" == 1 ]]; then
    (cd "$ROOT/packages/e2e" && pnpm stack:down >/dev/null 2>&1)
  fi
  docker rm -f -v "$PG" >/dev/null 2>&1
  df -h / | tail -1
}
trap cleanup EXIT

# The deployment fixtures use their own temporary resources and can run alongside the API.
deployment_tests() (
  step "deployment helper tests"
  for t in "$ROOT"/deploy/*.test.sh "$ROOT"/deploy/*/*.test.sh; do run bash "$t"; done
  run python3 -B -m unittest discover -s "$ROOT/deploy/ci" -p 'test_*.py'
  exit "$fail"
)
deployment_tests &
helpers_pid=$!

if [[ -z "${SKIP_API:-}" ]]; then
  step "api"
  docker run -d --name "$PG" -p 127.0.0.1:54339:5432 --tmpfs /var/lib/postgresql \
    -e POSTGRES_PASSWORD=gate-only postgres:18-alpine -c fsync=off -c synchronous_commit=off -c full_page_writes=off -c max_connections=300 >/dev/null || exit 1
  until docker exec "$PG" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; do sleep 1; done
  export DATABASE_URL=postgres://postgres:gate-only@127.0.0.1:54339/postgres
  export SQLX_OFFLINE=true
  unset UPDATE_OPENAPI
  cd "$ROOT/apps/api"
  run cargo fmt --check
  run cargo clippy --all-targets -- -D warnings
  run cargo test --tests -- migrations_fill_an_empty_database migrate_fills_an_empty_database
  run env DATABASE_URL="${DATABASE_URL%/*}/template1" "${CARGO_TARGET_DIR:-target}/debug/verzo-360-api" migrate
  run cargo test --no-fail-fast -- --skip migrations_fill_an_empty_database --skip migrate_fills_an_empty_database
  [ "$(grep -ciE 'laravel|vimdesk' openapi.json)" = 0 ] || { echo "FAILED: internal names in spec"; fail=1; }
  docker rm -f -v "$PG" >/dev/null 2>&1
  unset DATABASE_URL
fi

wait "$helpers_pid" || fail=1
helpers_pid=

if [[ -z "${SKIP_WEB:-}" ]]; then
  step "web"
  cd "$ROOT"
  run pnpm install --frozen-lockfile
  run pnpm turbo run generate:check --filter=@verzo/api-client --only --output-logs=errors-only
  run pnpm --filter @verzo/office voice:check
  run pnpm --filter @verzo/office exec node scripts/check-voice-schema.mjs
  run pnpm turbo run typecheck lint test build --filter='!@verzo/api' --output-logs=errors-only
fi

if [[ -z "${SKIP_E2E:-}" ]]; then
  step "e2e (fake API)"
  cd "$ROOT"
  run env E2E_PORT_OFFSET=95 VITE_FEATURE_RESOURCES=on NEXT_PUBLIC_SHOP_URL=https://shop.genesy.app pnpm turbo run e2e --output-logs=errors-only -- --workers=3
  run env E2E_PORT_OFFSET=95 pnpm --filter @verzo/e2e exec playwright test -c storefront-design.config.ts
  run pnpm --filter @verzo/e2e exec playwright test -c voice-dev.config.ts --workers=1
fi

if [[ -z "${SKIP_STACK:-}" ]]; then
  step "stack e2e"
  cd "$ROOT/packages/e2e"
  stack_started=1
  run pnpm stack:up
  run pnpm stack:test
fi

echo "GATE RESULT: $([ $fail = 0 ] && echo GREEN || echo RED)"
exit $fail
