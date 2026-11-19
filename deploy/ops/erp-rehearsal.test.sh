#!/usr/bin/env bash
# Refusal tests for erp-rehearsal.sh (no database needed): bash deploy/ops/erp-rehearsal.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fail() { echo "FAIL: $*" >&2; exit 1; }
refuses() {
  local want="$1"; shift
  local out
  if out="$(bash "$HERE/erp-rehearsal.sh" "$@" 2>&1)"; then fail "accepted: $*"; fi
  [[ "$out" == *"$want"* ]] || fail "expected '$want' for $*, got: $out"
}

refuses "usage" 
refuses "not a postgres URL" --database-url "mysql://x@127.0.0.1/postgres"
refuses "refusing host" --database-url "postgres://u:p@db.example.com:5432/postgres"
refuses "refusing host" --database-url "postgres://u:p@10.0.0.5:5432/postgres"
refuses "refusing maintenance database genesy360" --database-url "postgres://u:p@127.0.0.1:54320/genesy360"
refuses "no API binary" --database-url "postgres://u:p@127.0.0.1:54320/postgres" --api-bin /nonexistent
refuses "unknown argument" --force
echo "erp-rehearsal.test.sh: ok"
