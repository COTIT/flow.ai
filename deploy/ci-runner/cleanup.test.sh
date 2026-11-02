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
# CI disk 40% full: under the 70% line, so the daily prune keeps unused images.
cat > "$stubs/df" << 'EOF'
#!/usr/bin/env bash
printf 'Use%%\n 40%%\n'
EOF
chmod +x "$stubs/docker" "$stubs/du" "$stubs/df"
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
grep -qx "docker volume inspect -f {{.Mountpoint}} genesy-ci-trivy-cache" "$log" || fail "trivy cache not capped"

# Daily prune: keeps a capped build cache (image builds stay warm) instead of wiping it.
: > "$log"
DOCKER_HOST=unix:///run/user/1001/docker.sock CALLS="$log" PATH="$stubs:$PATH" bash "$HERE/cleanup.sh" prune > /dev/null
grep -qx "docker builder prune -f --max-used-space 12GB" "$log" || fail "build cache not capped"

echo "cleanup.test.sh: all passed"
