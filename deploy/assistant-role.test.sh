#!/usr/bin/env bash
# Role provisioning stays on stdin; these tests never connect to PostgreSQL.
set -Eeuo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
mkdir -p "$t/root/deploy/staging/secrets" "$t/bin"
touch "$t/root/deploy/staging/.env.staging"
printf "role-password-'\\\\-sentinel\n" > "$t/root/deploy/staging/secrets/assistant_checkpoint_password"
cat > "$t/bin/docker" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$CALLS"
cat > "$SQL"
SH
chmod +x "$t/bin/docker"
export GENESY_ROOT="$t/root" CALLS="$t/calls" SQL="$t/sql" PATH="$t/bin:$PATH"
bash "$HERE/assistant-role.sh" staging bootstrap > "$t/out" 2>&1 || fail "bootstrap: $(cat "$t/out")"
grep -q 'assistant_checkpoint_staging' "$SQL" || fail 'wrong environment role'
# Two overlapping Office instances each need four shared-pool slots and one readiness slot.
grep -q 'CONNECTION LIMIT 10' "$SQL" || fail 'role cannot hold blue/green pool and readiness connections'
grep -q 'PASSWORD' "$SQL" || fail 'password never applied'
! grep -q 'sentinel' "$CALLS" "$t/out" || fail 'password leaked to arguments or output'
grep -q 'ON_ERROR_STOP=1' "$CALLS" || fail 'SQL errors ignored'
grep -q 'COMMIT;' "$SQL" || fail 'bootstrap not committed'
bash "$HERE/assistant-role.sh" staging verify > "$t/out" 2>&1 || fail 'verify refused'
! grep -q 'sentinel' "$SQL" || fail 'verify read password'
printf 'first\nsecond\n' > "$t/root/deploy/staging/secrets/assistant_checkpoint_password"
if bash "$HERE/assistant-role.sh" staging bootstrap > "$t/out" 2>&1; then fail 'multiline password accepted'; fi
! grep -q 'COMMIT;' "$SQL" || fail 'invalid password committed role changes'
rm "$t/root/deploy/staging/secrets/assistant_checkpoint_password"
if bash "$HERE/assistant-role.sh" staging bootstrap > "$t/out" 2>&1; then fail 'missing password accepted'; fi
if bash "$HERE/assistant-role.sh" staging wrong > "$t/out" 2>&1; then fail 'unknown mode accepted'; fi
echo 'assistant-role.test.sh: all passed'
