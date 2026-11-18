#!/usr/bin/env bash
# Tests for set-user-password.sh with a fake deploy.sh:  bash deploy/set-user-password.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# Fake deploy.sh: records its argv and stdin, exits with FAKE_EXIT.
cat > "$tmp/deploy.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$FAKE_DIR/argv"
cat > "$FAKE_DIR/stdin"
echo "set-user-password afm: done"
exit "${FAKE_EXIT:-0}"
EOF
chmod +x "$tmp/deploy.sh"
# Synthetic passwords only: real ones never appear in tests.
run() {
  rm -f "$tmp/argv" "$tmp/stdin"
  DEPLOY_SH="$tmp/deploy.sh" FAKE_DIR="$tmp" bash "$HERE/set-user-password.sh" "$@" > "$tmp/out" 2>&1
}

# Typed twice and matching: piped once on stdin to the API CLI, never in argv or output.
printf 'Fake!Pass-AAA111\nFake!Pass-AAA111\n' | run production a@b.test --admin \
  || fail "matching passwords refused: $(cat "$tmp/out")"
[[ "$(cat "$tmp/stdin")" == "Fake!Pass-AAA111" ]] || fail "stdin: $(cat "$tmp/stdin")"
[[ "$(wc -l < "$tmp/stdin" | tr -d ' ')" == 1 ]] || fail "stdin is not exactly one line"
expected="production compose --profile cli run --rm -T cli set-user-password --workspace afm --email a@b.test --admin"
[[ "$(tr '\n' ' ' < "$tmp/argv" | sed 's/ $//')" == "$expected" ]] || fail "argv: $(cat "$tmp/argv")"
if grep -q "AAA111" "$tmp/argv" "$tmp/out"; then fail "password in argv or output"; fi
grep -q "set-user-password afm: done" "$tmp/out" || fail "CLI result not shown"

# Names with spaces pass through as single arguments.
printf 'Fake!Pass-BBB222\nFake!Pass-BBB222\n' \
  | run production c@d.test --first-name Anna --last-name "van Dam" || fail "names refused"
[[ "$(sed -n 17p "$tmp/argv")" == "van Dam" ]] || fail "last name split: $(cat "$tmp/argv")"

# Spaces inside the password survive (read without trimming).
printf ' Fake! Pass-1 \n Fake! Pass-1 \n' | run production a@b.test || fail "spaced password refused"
[[ "$(cat "$tmp/stdin")" == " Fake! Pass-1 " ]] || fail "spaces trimmed: [$(cat "$tmp/stdin")]"

# Mismatch and empty: refused before the server is touched.
if printf 'Fake!Pass-CCC333\nFake!Pass-DDD444\n' | run production a@b.test; then fail "mismatch taken"; fi
[[ ! -e "$tmp/argv" ]] || fail "mismatch still called deploy.sh"
if grep -q "CCC333\|DDD444" "$tmp/out"; then fail "mismatched password echoed"; fi
if printf '\n\n' | run production a@b.test; then fail "empty password taken"; fi
[[ ! -e "$tmp/argv" ]] || fail "empty still called deploy.sh"

# The CLI's refusal is the script's failure.
if printf 'Fake!Pass-EEE555\nFake!Pass-EEE555\n' | FAKE_EXIT=1 run production a@b.test; then
  fail "CLI refusal reported as success"
fi

# Usage errors: production only, email required.
if run staging a@b.test < /dev/null; then fail "staging taken"; fi
if run production < /dev/null; then fail "missing email taken"; fi
if run production --admin < /dev/null; then fail "flag taken as email"; fi
echo "set-user-password.sh: all tests passed"
