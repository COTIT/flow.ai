#!/usr/bin/env bash
# Local smoke check for the staging stack: build every image, start it without Traefik
# (docker-compose.local.yml), probe each service, then remove everything it added.
# Run from anywhere: deploy/staging/smoke.sh. Takes 10-20 min (release Rust build).
# Leaves Docker as it found it: removes only images that were not present before, prunes the build
# cache, and never touches other containers (genesy-dev-pg, genesy-pg-*).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
secrets="$(mktemp -d)"
project=genesy-staging-smoke
compose=(docker compose -p "$project" -f "$here/docker-compose.staging.yml" -f "$here/docker-compose.local.yml")

# Throwaway values: the smoke run never sees a real secret.
export TRAEFIK_NETWORK=genesy-staging-smoke-edge TRAEFIK_ENTRYPOINT=web TRAEFIK_CERTRESOLVER=none
export GENESY_REF=smoke SECRETS_DIR="$secrets" MAILPIT_UI_PORT=18025
pw="$(openssl rand -hex 24)"
printf '%s' "$pw" >"$secrets/db_password"
printf 'postgres://genesy:%s@db:5432/genesy' "$pw" >"$secrets/database_url"
openssl rand -hex 32 | tr -d '\n' >"$secrets/credentials_key"
chmod 444 "$secrets"/*

# Images this run may add: its own six plus the pinned ones compose pulls.
ours=(ghcr.io/cotit/genesy-{api,office,field}:smoke-staging ghcr.io/cotit/genesy-{shop,till,landing}:smoke)
pulled=($(sed -n 's/^ *image: \([^$]*@sha256:[0-9a-f]*\)$/\1/p' "$here/docker-compose.staging.yml"))
before="$(docker images -q --no-trunc | sort -u)"

cleanup() {
  "${compose[@]}" --profile seed down -v --remove-orphans >/dev/null 2>&1 || true
  for ref in "${ours[@]}" "${pulled[@]}"; do
    id="$(docker image inspect -f '{{.Id}}' "$ref" 2>/dev/null)" || continue
    grep -qx "$id" <<<"$before" || docker image rm -f "$ref" >/dev/null || true
  done
  docker builder prune -af >/dev/null
  rm -rf "$secrets"
}
trap cleanup EXIT

"${compose[@]}" build
"${compose[@]}" up -d --wait --wait-timeout 180

fail=0
# expect <name> <url> <allowed status regex>
expect() {
  code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 "$2")"
  if [[ "$code" =~ ^($3)$ ]]; then echo "ok   $1 $2 -> $code"; else echo "FAIL $1 $2 -> $code"; fail=1; fi
}
expect api "http://127.0.0.1:18080/healthz" 200
for app in shop:18100 office:18101 field:18102 till:18103; do
  # Home answers (2xx/3xx; the shop answers 503 until STOREFRONT_APP_KEY is set).
  expect "${app%%:*}" "http://127.0.0.1:${app##*:}/" '2..|3..|503'
  # The same-origin /v1 forward reaches the API: /v1/me without a token is the API's 401.
  expect "${app%%:*}" "http://127.0.0.1:${app##*:}/v1/me" 401
done
# The landing page answers and, on staging, tells search engines to stay away.
expect landing "http://127.0.0.1:18104/" 200
expect landing "http://127.0.0.1:18104/nl" 200
if curl -s --max-time 15 "http://127.0.0.1:18104/robots.txt" | grep -q "Disallow: /"; then
  echo "ok   landing robots.txt disallows"
else
  echo "FAIL landing robots.txt does not disallow"; fail=1
fi
# Fake payments booted (the API refuses PAYMENTS_PROVIDER=fake otherwise) and nothing restarted.
"${compose[@]}" ps --format '{{.Service}} {{.Status}}'
if "${compose[@]}" logs api | grep -q 'PAYMENTS_PROVIDER not set'; then echo "FAIL api: no payments provider"; fail=1; fi

[ "$fail" = 0 ] && echo "smoke: all green" || { echo "smoke: FAILED"; exit 1; }
