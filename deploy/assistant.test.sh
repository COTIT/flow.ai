#!/usr/bin/env bash
# Assistant deployment contracts; parses Compose without starting containers.
set -Eeuo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
for env in staging production; do
  d="$HERE/$env"
  [[ -f "$d/docker-compose.assistant.yml" ]] || fail "$env assistant overlay missing"
  command -v docker > /dev/null || continue
  cp "$d/.env.$env.example" "$t/$env.env"
  printf '\nGENESY_REF=test\nELEVENLABS_AGENT_ID=agent-test\nASSISTANT_ENABLED=true\nASSISTANT_WORKSPACE_ALLOWLIST=01950000-0000-7000-8000-000000000009\n' >> "$t/$env.env"
  docker compose -f "$d/docker-compose.$env.yml" -f "$d/docker-compose.assistant.yml" \
    --env-file "$t/$env.env" config --format json > "$t/$env.json"
  python3 - "$t/$env.json" <<'PY'
import json,sys
c=json.load(open(sys.argv[1])); api=c['services']['api']; office=c['services']['office']
assert office['environment']['ASSISTANT_ENABLED']=='true'
assert office['environment']['ASSISTANT_WORKSPACE_ALLOWLIST']=='01950000-0000-7000-8000-000000000009'
assert api['environment']['ASSISTANT_ENABLED']=='true'
assert office['environment']['ELEVENLABS_AGENT_ID']=='agent-test'
for key in ['ELEVENLABS_API_KEY','ASSISTANT_BRIDGE_SECRET','ASSISTANT_CHECKPOINT_URL','LANGSMITH_API_KEY']:
    assert office['environment'][key+'_FILE'].startswith('/run/secrets/')
    assert key not in office['environment']
for key in ['ELEVENLABS_API_KEY','ASSISTANT_BRIDGE_SECRET']:
    assert api['environment'][key+'_FILE']==office['environment'][key+'_FILE']
assert office['environment']['LANGSMITH_HIDE_INPUTS']=='true'
assert office['environment']['LANGSMITH_HIDE_OUTPUTS']=='true'
assert office['environment']['LANGCHAIN_CALLBACKS_BACKGROUND']=='false'
assert 'assistant_checkpoint_url' not in {x['source'] for x in api['secrets']}
assert 'database_url' in {x['source'] for x in api['secrets']}
PY
done

# Selector stays off without opt-in, preserves uploads, and rejects misspelled booleans.
mkdir -p "$t/root/deploy/staging/secrets"
touch "$t/root/deploy/staging/.env.staging" "$t/root/deploy/staging/docker-compose.uploads.yml" "$t/root/deploy/staging/secrets/b2_uploads_key"
chain() { GENESY_ROOT="$t/root" bash -c '. "$1"; env_setup staging; printf "%s\n" "${COMPOSE[@]}"' _ "$HERE/ops/lib.sh"; }
[[ "$(chain)" != *docker-compose.assistant.yml* ]] || fail 'assistant enabled implicitly'
printf 'ASSISTANT_ENABLED=true\n' > "$t/root/deploy/staging/.env.staging"
[[ "$(chain)" == *docker-compose.assistant.yml* && "$(chain)" == *docker-compose.uploads.yml* ]] || fail 'overlay selector dropped assistant or uploads'
touch "$t/root/deploy/staging/secrets/elevenlabs_api_key" "$t/root/deploy/staging/docker-compose.assistant.yml"
printf 'ASSISTANT_ENABLED=false\n' > "$t/root/deploy/staging/.env.staging"
[[ "$(chain)" == *docker-compose.assistant.yml* ]] || fail 'paused assistant lost cleanup credentials'
rm "$t/root/deploy/staging/docker-compose.assistant.yml"
[[ "$(chain)" != *docker-compose.assistant.yml* ]] || fail 'pre-assistant checkout requires missing overlay'
printf 'ASSISTANT_ENABLED=yes\n' > "$t/root/deploy/staging/.env.staging"
if chain > /dev/null 2>&1; then fail 'invalid assistant flag accepted'; fi
echo 'assistant.test.sh: all passed'
