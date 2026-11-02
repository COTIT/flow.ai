#!/usr/bin/env bash
# Parses every compose chain deploy.sh can build, with the example env files, and checks what
# the optional overlays switch on. Needs the docker CLI only (no daemon, no build):
#   bash deploy/compose.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
command -v docker > /dev/null || { echo "compose.test.sh: skipped (no docker CLI)"; exit 0; }

p="$HERE/production" s="$HERE/staging"
cp "$p/.env.production.example" "$t/prod.env" && echo "GENESY_REF=test" >> "$t/prod.env"
cp "$s/.env.staging.example" "$t/staging.env" && echo "GENESY_REF=test" >> "$t/staging.env"
# value <env file> <files…> -- <service> <var>: one resolved environment value, empty if unset.
value() {
  local env_file="$1" args=()
  shift
  while [[ "$1" != -- ]]; do args+=(-f "$1"); shift; done
  docker compose "${args[@]}" --env-file "$env_file" --profile cli config --format json \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['services']['$2'].get('environment',{}).get('$3') or '')"
}

for chain in "$p/docker-compose.production.yml" \
  "$p/docker-compose.production.yml $p/docker-compose.stripe.yml"; do
  # shellcheck disable=SC2046,SC2086  # paths hold no spaces
  docker compose $(printf -- '-f %s ' $chain) --env-file "$t/prod.env" config -q || fail "production chain: $chain"
done
for chain in "$s/docker-compose.staging.yml" "$s/docker-compose.staging.yml $s/docker-compose.stripe.yml" \
  "$s/docker-compose.staging.yml $s/docker-compose.local.yml"; do
  # shellcheck disable=SC2046,SC2086  # paths hold no spaces
  docker compose $(printf -- '-f %s ' $chain) --env-file "$t/staging.env" config -q || fail "staging chain: $chain"
done

# Without the uploads overlay: no bucket, no key file, no CSP origin.
[[ -z "$(value "$t/prod.env" "$p/docker-compose.production.yml" -- api B2_UPLOADS_BUCKET)" ]] || fail "uploads on by default"
[[ -z "$(value "$t/prod.env" "$p/docker-compose.production.yml" -- shop FILES_ORIGIN)" ]] || fail "FILES_ORIGIN set by default"
[[ "$(value "$t/prod.env" "$p/docker-compose.production.yml" -- api SKIP_BOOT_MIGRATIONS)" == true ]] || fail "boot migrations not skipped"

# With it, but the bucket not set: refuses with the variable's name.
if docker compose -f "$p/docker-compose.production.yml" -f "$p/docker-compose.uploads.yml" \
  --env-file "$t/prod.env" config -q 2> "$t/err"; then fail "uploads without a bucket parsed"; fi
grep -q "set B2_UPLOADS_BUCKET in .env.production" "$t/err" || fail "unclear error: $(cat "$t/err")"

# With it and the bucket set: key from the secret file, CSP origin = endpoint, stripe still stacks.
printf 'B2_UPLOADS_BUCKET=genesy-prod\nB2_UPLOADS_KEY_ID=005abc\n' >> "$t/prod.env"
on=("$p/docker-compose.production.yml" "$p/docker-compose.stripe.yml" "$p/docker-compose.uploads.yml")
[[ "$(value "$t/prod.env" "${on[@]}" -- api B2_UPLOADS_BUCKET)" == genesy-prod ]] || fail "bucket not passed"
[[ "$(value "$t/prod.env" "${on[@]}" -- api B2_UPLOADS_APPLICATION_KEY_FILE)" == /run/secrets/b2_uploads_key ]] || fail "key file not passed"
[[ "$(value "$t/prod.env" "${on[@]}" -- cli B2_UPLOADS_KEY_ID)" == 005abc ]] || fail "cli has no uploads"
[[ "$(value "$t/prod.env" "${on[@]}" -- api PAYMENTS_PROVIDER)" == stripe ]] || fail "uploads overlay dropped stripe"
for app in shop office; do
  [[ "$(value "$t/prod.env" "${on[@]}" -- $app FILES_ORIGIN)" == https://s3.eu-central-003.backblazeb2.com ]] || fail "$app FILES_ORIGIN"
done
docker compose -f "${on[0]}" -f "${on[1]}" -f "${on[2]}" --env-file "$t/prod.env" config --format json \
  | python3 -c "import json,sys; s=json.load(sys.stdin)['services']['api']['secrets']; n={x['source'] for x in s}; sys.exit(0 if {'b2_uploads_key','stripe_secret_key','database_url'} <= n else 1)" \
  || fail "api secrets lost in the merge"

echo "compose.test.sh: all passed"
