#!/usr/bin/env bash
# Tests for watch.sh with stub curl/openssl/docker/df on PATH:  bash deploy/ops/watch.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

mkdir -p "$t/bin" "$t/conf" "$t/backups/production" "$t/root/deploy/production"
cp "$REPO/deploy/production/docker-compose.production.yml" "$t/root/deploy/production/"
echo "GENESY_REF=abc" > "$t/root/deploy/production/.env.production"
touch "$t/backups/production/genesy-production-20261102T000000Z.dump"

# curl: STUB_DOWN host answers 502; pings are logged.
cat > "$t/bin/curl" << 'STUB'
#!/usr/bin/env bash
echo "curl $*" >> "$CALLS"
[[ -n "${STUB_DOWN:-}" && "$*" == *"https://$STUB_DOWN/"* ]] && { printf 502; exit 0; }
printf 200
STUB
# openssl: s_client prints the host it was asked for; x509 fails for STUB_EXPIRING.
cat > "$t/bin/openssl" << 'STUB'
#!/usr/bin/env bash
if [[ "$1" == s_client ]]; then
  while (( $# )); do [[ "$1" == -servername ]] && echo "cert-for $2"; shift; done
  exit 0
fi
! grep -qx "cert-for ${STUB_EXPIRING:-none}"
STUB
# docker: one prod container; inspect reads its line from $T/inspect.
cat > "$t/bin/docker" << 'STUB'
#!/usr/bin/env bash
case "$1" in
  ps) [[ "$*" == *project=genesy-prod* ]] && echo c1 ;;
  inspect) cat "$T/inspect" ;;
  compose) [[ "$*" == *"config --services"* ]] && printf 'api\ndb\n'
    [[ "$*" == *" ps "* ]] && cat "$T/ps" ;;
esac
STUB
cat > "$t/bin/df" << 'STUB'
#!/usr/bin/env bash
echo "Filesystem 1024-blocks Used Available Capacity Mounted"
echo "/dev/x 100 1 1 ${STUB_DISK:-40}% /"
STUB
chmod +x "$t/bin/"*

export CALLS="$t/calls" T="$t" PATH="$t/bin:$PATH" GENESY_OPS_CONF="$t/conf" \
  GENESY_BACKUPS="$t/backups" GENESY_OPS_STATE="$t/state" GENESY_ROOT="$t/root"
echo "https://hc.test/k" > "$t/conf/ping_base"
echo "/genesy-prod-api-1 0 running healthy" > "$t/inspect"
run() { : > "$CALLS"; bash "$HERE/watch.sh" > "$t/out" 2>&1; }

# All good: ok ping, staging (no checkout here) skipped.
run || fail "healthy box alerted: $(cat "$t/out")"
grep -q "curl .*https://hc.test/k/vps-watch$" "$CALLS" || fail "no ok ping"
if grep -q "staging" "$t/out"; then fail "missing staging checkout reported"; fi

# Each problem lands in the one fail ping, prefixed with its env.
expect() { # expect <text>: last run failed, and the fail ping carries <text>
  grep -q "curl .*--data-raw .*$1.*https://hc.test/k/vps-watch/fail" "$CALLS" || fail "no fail ping with '$1': $(cat "$t/out")"
}
if STUB_DOWN=office.genesy.app run; then fail "down host passed"; fi
expect "production: down: office.genesy.app=502"
if STUB_EXPIRING=till.genesy.app run; then fail "expiring cert passed"; fi
expect "production: certificate of till.genesy.app"
if STUB_DISK=91 run; then fail "full disk passed"; fi
expect "disk: / at 91%"
echo "/genesy-prod-api-1 4 running healthy" > "$t/inspect"
if run; then fail "restart loop passed"; fi
expect "genesy-prod-api-1 restarted 4 times"
run || fail "same count twice still alerted: $(cat "$t/out")"
echo "/genesy-prod-api-1 4 running unhealthy" > "$t/inspect"
if run; then fail "unhealthy passed"; fi
expect "genesy-prod-api-1 is unhealthy"
echo "/genesy-prod-api-1 4 running healthy" > "$t/inspect"

# No DNS yet (nodns overlay): Traefik serves no route and no certificate, so hosts and
# certificates are not checked; one info line instead of an alert every 5 minutes. A stopped or
# unhealthy service still alerts.
touch "$t/root/deploy/production/docker-compose.nodns.yml"
printf 'api running healthy\ndb running \n' > "$t/ps"
STUB_DOWN=office.genesy.app STUB_EXPIRING=till.genesy.app run || fail "routes off alerted: $(cat "$t/out")"
[[ "$(grep -c "production: Traefik routes off (no DNS yet); host and certificate checks skipped" "$t/out")" == 1 ]] \
  || fail "no single routes-off line: $(cat "$t/out")"
if grep -q "curl .*--resolve\|curl .*https://.*genesy.app" "$CALLS"; then fail "probed hosts under nodns"; fi
printf 'api exited \ndb running healthy\n' > "$t/ps"
if run; then fail "stopped service passed under nodns"; fi
expect "production: down: api=exited"
rm "$t/root/deploy/production/docker-compose.nodns.yml"

touch -t 202001010000 "$t/backups/production/genesy-production-20261102T000000Z.dump"
if run; then fail "stale backup passed"; fi
expect "production: no dump in the last 26 h"

# No ping configured: still exits 1 and logs, never calls out.
rm "$t/conf/ping_base"
if run; then fail "stale backup passed without ping"; fi
if grep -q "hc.test" "$CALLS"; then fail "pinged without a ping_base"; fi
grep -q "notify: vps-watch fail" "$t/out" || fail "not logged"

echo "watch.test.sh: all passed"
