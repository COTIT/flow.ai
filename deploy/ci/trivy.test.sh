#!/usr/bin/env bash
# Exercise the workflow's actual installer with a fake download/extractor and real SHA checking.
set -Eeuo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
python3 - "$here/../../.github/workflows/ci.yml" > "$t/install.sh" <<'PY'
from pathlib import Path
import sys
step = Path(sys.argv[1]).read_text().split("      - name: Install Trivy\n", 1)[1].split("\n      - ", 1)[0]
script = step.split("        run: |\n", 1)[1]
print("\n".join(line[10:] for line in script.splitlines()))
PY
mkdir "$t/bin" "$t/runner"
cat > "$t/bin/curl" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$CALLS"
[[ -z "${FAIL_DOWNLOAD:-}" ]] || exit 7
while (( $# )); do
  if [[ "$1" == --output ]]; then printf 'checked archive' > "$2"; exit 0; fi
  shift
done
exit 1
SH
printf '#!/usr/bin/env bash\necho extracted >> "$CALLS"\n' > "$t/bin/tar"
chmod +x "$t/bin/"*
export CALLS="$t/calls" RUNNER_TEMP="$t/runner" TRIVY_VERSION=0.76.0
export TRIVY_SHA256="$(printf 'checked archive' | sha256sum | cut -d' ' -f1)"
export PATH="$t/bin:$PATH"
bash -e "$t/install.sh" > "$t/out"
grep -q -- '--retry 3 --retry-all-errors' "$CALLS" || fail 'download has no bounded retry'
grep -q -- '--connect-timeout 10 --max-time 60 --retry-max-time 180' "$CALLS" || fail 'download has no time limits'
grep -q extracted "$CALLS" || fail 'verified archive was not extracted'
: > "$CALLS"
FAIL_DOWNLOAD=1 bash -e "$t/install.sh" > "$t/out"
[[ "$(cat "$CALLS")" == extracted ]] || fail 'cached archive was downloaded again'
printf corrupted > "$RUNNER_TEMP/trivy.tgz"
: > "$CALLS"
if bash -e "$t/install.sh" > "$t/out" 2>&1; then fail 'corrupt cache bypassed checksum'; fi
[[ ! -s "$CALLS" ]] || fail 'corrupt archive was extracted'
rm "$RUNNER_TEMP/trivy.tgz"
: > "$CALLS"
if FAIL_DOWNLOAD=1 bash -e "$t/install.sh" > "$t/out" 2>&1; then fail 'failed download passed'; fi
if grep -q extracted "$CALLS"; then fail 'failed download was extracted'; fi
echo 'trivy.test.sh: all passed'
