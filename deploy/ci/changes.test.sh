#!/usr/bin/env bash
# Tests for changes.sh on a throwaway git repo:  bash deploy/ci/changes.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

g() { git -C "$t" -c user.email=t@t -c user.name=t "$@"; }
g init -q && g commit -q --allow-empty -m root && ROOT="$(g rev-parse HEAD)"

# commit <file…>: one commit on top of ROOT touching these files; prints its sha.
commit() {
  g checkout -q --detach "$ROOT"
  local f
  for f in "$@"; do mkdir -p "$t/$(dirname "$f")" && echo x >> "$t/$f"; done
  g add -A && g commit -qm change && g rev-parse HEAD
}
# expect "<want>" <base> <head>: want is "api web img_api img_web" as 0/1.
expect() {
  local want="$1" got
  got="$(cd "$t" && bash "$HERE/changes.sh" "$2" "$3" | sed 's/.*=true/1/; s/.*=false/0/' | tr '\n' ' ')"
  [[ "${got% }" == "$want" ]] || fail "$want wanted, got '${got% }' for: $(g diff --name-only "$2" "$3" 2> /dev/null | tr '\n' ' ')"
}

expect "1 0 1 0" "$ROOT" "$(commit apps/api/src/main.rs)"
expect "1 1 1 0" "$ROOT" "$(commit apps/api/openapi.json)"
expect "0 1 0 1" "$ROOT" "$(commit apps/shop/app/page.tsx)"
expect "0 1 0 1" "$ROOT" "$(commit packages/ui/button.tsx pnpm-lock.yaml)"
expect "0 0 0 0" "$ROOT" "$(commit deploy/deploy.sh deploy/ops/lib.sh)"
expect "0 0 0 0" "$ROOT" "$(commit docs/notes.md README.md .claude/x)"
expect "0 1 0 1" "$ROOT" "$(commit deploy/images/next.Dockerfile)"
expect "0 0 1 1" "$ROOT" "$(commit deploy/staging/docker-compose.staging.yml)"
expect "0 0 1 1" "$ROOT" "$(commit deploy/production/docker-compose.stripe.yml)"
expect "1 1 1 1" "$ROOT" "$(commit .github/workflows/ci.yml)"
expect "1 1 1 1" "$ROOT" "$(commit deploy/ci/changes.sh)"
expect "1 1 1 1" "$ROOT" "$(commit apps/api/src/lib.rs apps/office/app/page.tsx)"
expect "0 1 0 1" "$ROOT" "$(commit apps/field/README.md)"
expect "0 1 0 1" "$ROOT" "$(commit apps/flow/app/page.tsx)"

# Unknown base: everything runs.
HEAD_SHA="$(commit apps/shop/x.ts)"
expect "1 1 1 1" "" "$HEAD_SHA"
expect "1 1 1 1" 0000000000000000000000000000000000000000 "$HEAD_SHA"
expect "1 1 1 1" deadbeefdeadbeefdeadbeefdeadbeefdeadbeef "$HEAD_SHA"
# Not an ancestor (a sibling commit, as after a force push): everything runs.
expect "1 1 1 1" "$(commit apps/till/y.ts)" "$HEAD_SHA"
# Nothing changed (a re-run of the green commit itself): nothing to do.
expect "0 0 0 0" "$HEAD_SHA" "$HEAD_SHA"

echo "changes.test.sh: all passed"
