#!/usr/bin/env bash
# Exercise the workflow installer with fake Docker and real checksum checking.
set -Eeuo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
python3 - "$here/../../.github/workflows/ci.yml" > "$t/install.sh" <<'SCRIPT'
from pathlib import Path
import re, sys
workflow = Path(sys.argv[1]).read_text()
assert re.search(r'TRIVY_IMAGE: aquasec/trivy:[^\n]+@sha256:[0-9a-f]{64}', workflow)
step = workflow.split("      - name: Install Trivy\n", 1)[1].split("\n      - ", 1)[0]
script = step.split("        run: |\n", 1)[1]
print("\n".join(line[10:] for line in script.splitlines()))
SCRIPT
mkdir "$t/bin" "$t/runner"
cat > "$t/bin/docker" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$CALLS"
case "$1" in
  pull) [[ -z "${FAIL_DOWNLOAD:-}" ]] || exit 7 ;;
  create) printf fixture-tool-container ;;
  cp) [[ -z "${FAIL_COPY:-}" ]] || exit 8
    printf 'checked binary' > "$3" ;;
  rm) ;;
  *) exit 1 ;;
esac
SH
printf '#!/usr/bin/env bash\necho "timeout $1" >> "$CALLS"\nshift\nexec "$@"\n' > "$t/bin/timeout"
printf '#!/usr/bin/env bash\nexit 0\n' > "$t/bin/sleep"
chmod +x "$t/bin/"*
export CALLS="$t/calls" RUNNER_TEMP="$t/runner" TRIVY_IMAGE=aquasec/trivy:pinned
export TRIVY_BINARY_SHA256="$(printf 'checked binary' | sha256sum | cut -d' ' -f1)"
export PATH="$t/bin:$PATH"
bash -e "$t/install.sh" > "$t/out"
grep -q '^timeout 60$' "$CALLS" || fail 'pull has no deadline'
grep -q '^pull --platform linux/amd64 aquasec/trivy:pinned$' "$CALLS" || fail 'wrong tool image/platform'
grep -q '^rm -f fixture-tool-container$' "$CALLS" || fail 'tool container was not removed'
: > "$CALLS"
FAIL_DOWNLOAD=1 bash -e "$t/install.sh" > "$t/out"
[[ ! -s "$CALLS" ]] || fail 'cached binary was fetched again'
printf corrupted > "$RUNNER_TEMP/trivy"
if bash -e "$t/install.sh" > "$t/out" 2>&1; then fail 'corrupt cache bypassed checksum'; fi
[[ ! -s "$CALLS" ]] || fail 'corrupt binary was replaced before verification'
rm "$RUNNER_TEMP/trivy"
: > "$CALLS"
if FAIL_DOWNLOAD=1 bash -e "$t/install.sh" > "$t/out" 2>&1; then fail 'failed pull passed'; fi
[[ "$(grep -c '^timeout ' "$CALLS")" = 3 ]] || fail 'pull retry count is not bounded at three'
if grep -q '^cp ' "$CALLS"; then fail 'failed pull was copied'; fi
: > "$CALLS"
if FAIL_COPY=1 bash -e "$t/install.sh" > "$t/out" 2>&1; then fail 'failed copy passed'; fi
grep -q '^rm -f fixture-tool-container$' "$CALLS" || fail 'copy failure leaked tool container'
echo 'trivy.test.sh: all passed'
