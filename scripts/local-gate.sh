#!/usr/bin/env bash
# Full local gate, mirroring .github/workflows/ci.yml (api, web + fake-API e2e, full-stack e2e),
# then gives the disk space back. Run from any checkout or worktree before pushing to main:
#   scripts/local-gate.sh            # everything
#   SKIP_API=1 scripts/local-gate.sh # skip a part: SKIP_API, SKIP_WEB, SKIP_E2E, SKIP_STACK
# Prints "GATE RESULT: GREEN" or "RED" and exits non-zero on red.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PG=genesy-gate-pg-$$
fail=0
step() { echo "=== $1"; }
run() { "$@" || { echo "FAILED: $*"; fail=1; }; }

cleanup() {
  step cleanup
  (cd "$ROOT/packages/e2e" && pnpm stack:down >/dev/null 2>&1)
  docker rm -f -v "$PG" >/dev/null 2>&1
  (cd "$ROOT/apps/api" && cargo clean >/dev/null 2>&1)
  docker builder prune -a -f >/dev/null 2>&1
  df -h / | tail -1
}
trap cleanup EXIT

step "deployment helper tests"
for t in "$ROOT"/deploy/*.test.sh "$ROOT"/deploy/*/*.test.sh; do run bash "$t"; done
run python3 -B -m unittest discover -s "$ROOT/deploy/ci" -p 'test_*.py'

if [[ -z "${SKIP_API:-}" ]]; then
  step "api"
  docker run -d --name "$PG" -p 127.0.0.1:54339:5432 --tmpfs /var/lib/postgresql \
    -e POSTGRES_PASSWORD=gate-only postgres:18-alpine -c fsync=off -c max_connections=300 >/dev/null
  until docker exec "$PG" pg_isready -U postgres >/dev/null 2>&1; do sleep 1; done
  export DATABASE_URL=postgres://postgres:gate-only@127.0.0.1:54339/postgres
  export SQLX_OFFLINE=true CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
  cd "$ROOT/apps/api"
  run cargo fmt --check
  run cargo clippy --all-targets -- -D warnings
  run cargo test --tests -- migrations_fill_an_empty_database migrate_fills_an_empty_database
  run env DATABASE_URL="${DATABASE_URL%/*}/template1" target/debug/genesy-360-api migrate
  run cargo test --no-fail-fast -- --skip migrations_fill_an_empty_database --skip migrate_fills_an_empty_database
  run cargo test --test openapi committed_spec
  [ "$(grep -ciE 'laravel|vimdesk' openapi.json)" = 0 ] || { echo "FAILED: internal names in spec"; fail=1; }
  docker rm -f -v "$PG" >/dev/null 2>&1
  unset DATABASE_URL
fi

if [[ -z "${SKIP_WEB:-}" ]]; then
  step "web"
  cd "$ROOT"
  run pnpm install --frozen-lockfile
  run pnpm turbo run generate:check --filter=@genesy/api-client --only --output-logs=errors-only
  run pnpm --filter @genesy/office voice:check
  run pnpm --filter @genesy/office exec node scripts/check-voice-schema.mjs
  run pnpm turbo run typecheck lint test build --filter='!@genesy/api' --output-logs=errors-only
fi

if [[ -z "${SKIP_E2E:-}" ]]; then
  step "e2e (fake API)"
  cd "$ROOT"
  run env E2E_PORT_OFFSET=95 VITE_FEATURE_RESOURCES=on pnpm turbo run e2e --output-logs=errors-only -- --workers=3
  run pnpm --filter @genesy/e2e exec playwright test -c voice-dev.config.ts --workers=1
fi

if [[ -z "${SKIP_STACK:-}" ]]; then
  step "stack e2e"
  cd "$ROOT/packages/e2e"
  run pnpm stack:up
  run pnpm stack:test
fi

echo "GATE RESULT: $([ $fail = 0 ] && echo GREEN || echo RED)"
exit $fail
