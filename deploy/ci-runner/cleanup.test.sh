#!/usr/bin/env bash
# Tests for cleanup.sh with stub docker/du on PATH:  bash deploy/ci-runner/cleanup.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
stubs="$(mktemp -d)"
trap 'rm -rf "$stubs"' EXIT
log="$stubs/calls"

cat > "$stubs/docker" << 'EOF'
#!/usr/bin/env bash
echo "docker $*" >> "$CALLS"
case "$1 $2" in
  "ps -aq") echo leftover1 ;;
  "volume inspect") echo "/vol/${*: -1}" ;;
esac
EOF
# Sizes in KB: target is 11 GB (over its 10 GB cap), everything else is small.
cat > "$stubs/du" << 'EOF'
#!/usr/bin/env bash
case "$2" in
  */genesy-ci-cargo-target) printf '11534336\t%s\n' "$2" ;;
  *) printf '1024\t%s\n' "$2" ;;
esac
EOF
chmod +x "$stubs/docker" "$stubs/du"
run() { CALLS="$log" PATH="$stubs:$PATH" bash "$HERE/cleanup.sh"; }
fail() { echo "FAIL: $*" >&2; exit 1; }

# Refuses prod's socket and touches nothing.
: > "$log"
if DOCKER_HOST=unix:///var/run/docker.sock run 2> /dev/null; then fail "accepted prod's socket"; fi
[[ ! -s "$log" ]] || fail "called docker with prod's socket"

# Job hook: wipes leftovers, removes only the volume over its cap.
: > "$log"
DOCKER_HOST=unix:///run/user/1001/docker.sock run > /dev/null
grep -qx "docker rm -f leftover1" "$log" || fail "leftover container not removed"
grep -qx "docker volume rm -f genesy-ci-cargo-target" "$log" || fail "oversized target volume kept"
if grep -q "volume rm -f genesy-ci-cargo-registry" "$log"; then fail "small volume removed"; fi

echo "cleanup.test.sh: all passed"
