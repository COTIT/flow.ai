#!/usr/bin/env bash
# Tests for assistant-agent.sh against a fake ElevenLabs on 127.0.0.1; no live calls, fake keys only.
#   bash deploy/assistant-agent-publish.test.sh
set -Eeuo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
server=""
trap '[[ -z "$server" ]] || kill "$server" 2> /dev/null || true; rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; [[ ! -f "$tmp/out" ]] || sed 's/^/  | /' "$tmp/out" >&2; exit 1; }

KEY=xi_fake_KEY0123456789
AGENT=agent_FAKEDEV000000001
printf 'OTHER=1\nELEVENLABS_API_KEY=%s\nELEVENLABS_AGENT_ID=%s\n' "$KEY" "$AGENT" > "$tmp/env.local"
printf '%s\n' "$KEY" > "$tmp/raw_key"

# Fake provider: one agent and its two client tools, state in files, every request logged.
cat > "$tmp/fake.py" <<'PY'
import json, os, pathlib, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
state = pathlib.Path(sys.argv[1]); key = sys.argv[2]
def load(name): return json.loads((state / name).read_text())
def save(name, value): (state / name).write_text(json.dumps(value))
def merge(into, patch):
    for k, v in patch.items():
        if isinstance(v, dict) and isinstance(into.get(k), dict): merge(into[k], v)
        else: into[k] = v
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def reply(self, code, body):
        raw = json.dumps(body).encode(); self.send_response(code)
        self.send_header('Content-Type', 'application/json'); self.send_header('Content-Length', str(len(raw)))
        self.end_headers(); self.wfile.write(raw)
    def handle_any(self, method):
        body = None
        if method == 'PATCH':
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        with open(state / 'log', 'a') as log:
            log.write(json.dumps({'method': method, 'path': self.path, 'body': body}) + '\n')
        if self.headers.get('xi-api-key') != key: return self.reply(401, {'detail': 'bad key'})
        parts = self.path.strip('/').split('/')
        mode = (state / 'mode').read_text().strip() if (state / 'mode').exists() else ''
        if parts[:3] == ['v1', 'convai', 'agents'] and len(parts) == 4:
            agent = load('agent.json')
            if method == 'PATCH':
                if mode != 'ignore_patch': merge(agent, body)
                if mode == 'drop_voice': agent['conversation_config'].pop('tts', None)
                n = int((state / 'n').read_text()) + 1 if (state / 'n').exists() else 1
                (state / 'n').write_text(str(n)); agent['version_id'] = 'agtvrsn_new%04d' % n
                save('agent.json', agent)
            return self.reply(200, agent)
        if parts[:3] == ['v1', 'convai', 'tools'] and len(parts) == 5 and parts[4] == 'dependent-agents':
            return self.reply(200, load('deps_%s.json' % parts[3]))
        if parts[:3] == ['v1', 'convai', 'tools'] and len(parts) == 4:
            if not (state / ('tool_%s.json' % parts[3])).exists(): return self.reply(404, {})
            tool = load('tool_%s.json' % parts[3])
            if method == 'PATCH':
                tool['tool_config'] = body['tool_config']; save('tool_%s.json' % parts[3], tool)
            return self.reply(200, tool)
        self.reply(404, {})
    def do_GET(self): self.handle_any('GET')
    def do_PATCH(self): self.handle_any('PATCH')
srv = HTTPServer(('127.0.0.1', 0), H)
(state / 'port').write_text(str(srv.server_port))
srv.serve_forever()
PY

# fixture <shared:0|1>: live agent = template with older values plus environment-only fields.
fixture() {
  rm -rf "$tmp/state" && mkdir -p "$tmp/state"
  cp "$HERE/assistant-validation.json" "$tmp/validation.json"
  python3 - "$HERE" "$tmp/state" "$AGENT" "$1" <<'PY'
import copy, json, pathlib, sys
here, state, agent_id, shared = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3], sys.argv[4] == '1'
t = json.loads((here / 'assistant-agent.template.json').read_text())
live = copy.deepcopy(t['configuration'])
cc = live['conversation_config']
cc['conversation']['max_duration_seconds'] = 600
cc['turn']['silence_end_call_timeout'] = 60
cc['agent']['prompt']['prompt'] = 'Old prompt.\nSecond line.'
cc['agent']['prompt']['tool_ids'] = ['tool_DISCOVERFAKE0001', 'tool_EXECUTEFAKE00001']
cc['tts'] = {'voice_id': 'voice_FAKEVOICE000001', 'stability': 0.5}
cc['language_presets']['nl']['first_message_translation'] = {'source_hash': 'x'}
ps = live['platform_settings']
ps['auth']['require_origin_header'] = True
ps['auth']['allowlist'] = [{'hostname': 'office.fake-origin.test'}]
ps['call_limits'] = {'daily_limit': 150}
live.update({'agent_id': agent_id, 'name': 'Genesy Development', 'version_id': 'agtvrsn_old0001'})
(state / 'agent.json').write_text(json.dumps(live))
tools = {tool['name']: copy.deepcopy(tool) for tool in t['clientTools']}
tools['execute_genesy_tool'].update({'force_pre_tool_speech': True, 'pre_tool_speech': 'force'})
tools['execute_genesy_tool']['extra_server_field'] = 'kept'
for tid, name in [('tool_DISCOVERFAKE0001', 'discover_genesy_tools'), ('tool_EXECUTEFAKE00001', 'execute_genesy_tool')]:
    (state / ('tool_%s.json' % tid)).write_text(json.dumps({'id': tid, 'tool_config': tools[name]}))
    agents = [{'type': 'available', 'id': agent_id, 'name': 'Genesy Development'}]
    if shared and name == 'execute_genesy_tool':
        agents.append({'type': 'available', 'id': 'agent_FAKEPROD00000001', 'name': 'Genesy Production'})
    (state / ('deps_%s.json' % tid)).write_text(json.dumps({'agents': agents, 'has_more': False}))
PY
  : > "$tmp/state/log"
  [[ -z "$server" ]] || kill "$server" 2> /dev/null || true
  python3 "$tmp/fake.py" "$tmp/state" "$KEY" &
  server=$!
  for _ in $(seq 50); do [[ -s "$tmp/state/port" ]] && break; sleep 0.1; done
  [[ -s "$tmp/state/port" ]] || fail "fake provider did not start"
  BASE="http://127.0.0.1:$(cat "$tmp/state/port")"
}

# run <env> [--apply]: development reads .env.local-style key + id; the others a raw key file.
run() {
  local keyfile="$tmp/raw_key"
  [[ "$1" == development ]] && keyfile="$tmp/env.local"
  ELEVENLABS_API_BASE="$BASE" ASSISTANT_AGENT_KEY_FILE="$keyfile" ELEVENLABS_AGENT_ID="$AGENT" \
    ASSISTANT_VALIDATION_FILE="$tmp/validation.json" bash "$HERE/assistant-agent.sh" "$@" > "$tmp/out" 2>&1
}
rename() { python3 -c 'import json,sys; p=sys.argv[1]; a=json.load(open(p)); a["name"]=sys.argv[2]; json.dump(a,open(p,"w"))' "$tmp/state/agent.json" "$1"; }
patches() { grep -c '"method": "PATCH"' "$tmp/state/log" || true; }
no_secrets() {
  for secret in "$KEY" "$AGENT" voice_FAKEVOICE000001 tool_DISCOVERFAKE0001 tool_EXECUTEFAKE00001 \
    agent_FAKEPROD00000001 fake-origin.test 'Old prompt'; do
    if grep -qF "$secret" "$tmp/out"; then fail "printed a private value: $secret"; fi
  done
}

# 1. Dry run: plain diff of governed fields only, no writes, no private values.
fixture 0
run development || fail "dry run failed"
grep -qF "conversation_config.conversation.max_duration_seconds: 600 → 1800" "$tmp/out" || fail "duration diff missing"
grep -qF "conversation_config.turn.silence_end_call_timeout: 60 → 180" "$tmp/out" || fail "silence diff missing"
grep -qE "conversation_config.agent.prompt.prompt: <24 chars sha256 [0-9a-f]{12}> → <[0-9]+ chars sha256 [0-9a-f]{12}>" "$tmp/out" || fail "prompt diff not summarised"
grep -qF "tool execute_genesy_tool: force_pre_tool_speech: true → false" "$tmp/out" || fail "tool diff missing"
grep -qF "tool execute_genesy_tool: pre_tool_speech: \"force\" → \"auto\"" "$tmp/out" || fail "pre_tool_speech diff missing"
if grep -q "discover_genesy_tools:\|require_origin_header\|voice_id\|call_limits\|allowlist\|tool_ids" "$tmp/out"; then fail "non-governed field in diff"; fi
grep -qF "dry run" "$tmp/out" || fail "dry run not labelled"
no_secrets
[[ "$(patches)" == 0 ]] || fail "dry run sent a PATCH"
cmp -s "$HERE/assistant-validation.json" "$tmp/validation.json" || fail "dry run changed the validation record"

# 2. Production apply needs the confirmation every time; refused before any request.
: > "$tmp/state/log"
run production --apply && fail "production applied without ASSISTANT_AGENT_CONFIRM"
grep -qF "ASSISTANT_AGENT_CONFIRM=production" "$tmp/out" || fail "refusal does not name the confirmation"
[[ ! -s "$tmp/state/log" ]] || fail "refused production run still called the provider"

# 3. Guards on inputs: non-loopback base, missing key, bad usage.
ELEVENLABS_API_BASE=http://example.com ASSISTANT_AGENT_KEY_FILE="$tmp/raw_key" ELEVENLABS_AGENT_ID="$AGENT" \
  bash "$HERE/assistant-agent.sh" staging > "$tmp/out" 2>&1 && fail "accepted a non-loopback API base"
ELEVENLABS_API_BASE="$BASE" ASSISTANT_AGENT_KEY_FILE="$tmp/missing" ELEVENLABS_AGENT_ID="$AGENT" \
  bash "$HERE/assistant-agent.sh" staging > "$tmp/out" 2>&1 && fail "ran without a key file"
grep -qF "ASSISTANT_AGENT_KEY_FILE" "$tmp/out" || fail "missing key message unhelpful"
bash "$HERE/assistant-agent.sh" nowhere > "$tmp/out" 2>&1 && fail "accepted an unknown environment"
bash "$HERE/assistant-agent.sh" staging --force > "$tmp/out" 2>&1 && fail "accepted an unknown flag"
[[ ! -s "$tmp/state/log" ]] || fail "guard failures still called the provider"

# 4. Development apply: PATCH carries only changed governed leaves; read-back passes; record written.
run development --apply || fail "development apply failed"
python3 - "$tmp/state" <<'PY' || fail "apply touched more than the governed diff"
import json, pathlib, sys
log = [json.loads(l) for l in (pathlib.Path(sys.argv[1]) / 'log').read_text().splitlines()]
patches = [e for e in log if e['method'] == 'PATCH']
agent = [e for e in patches if e['path'].startswith('/v1/convai/agents/')]
tools = [e for e in patches if e['path'].startswith('/v1/convai/tools/')]
assert len(agent) == 1 and len(tools) == 1, patches
assert tools[0]['path'] == '/v1/convai/tools/tool_EXECUTEFAKE00001'
body = agent[0]['body']
assert set(body) == {'conversation_config'}, body.keys()
cc = body['conversation_config']
assert cc == {'agent': {'prompt': {'prompt': cc['agent']['prompt']['prompt']}},
              'conversation': {'max_duration_seconds': 1800}, 'turn': {'silence_end_call_timeout': 180}}, cc
tool = tools[0]['body']['tool_config']
assert tool['force_pre_tool_speech'] is False and tool['pre_tool_speech'] == 'auto'
assert tool['extra_server_field'] == 'kept' and tool['name'] == 'execute_genesy_tool'
live = json.loads((pathlib.Path(sys.argv[1]) / 'agent.json').read_text())
assert live['conversation_config']['tts']['voice_id'] == 'voice_FAKEVOICE000001'
assert live['platform_settings']['call_limits'] == {'daily_limit': 150}
assert live['platform_settings']['auth']['require_origin_header'] is True
PY
grep -qF "read-back matches the template" "$tmp/out" || fail "no read-back confirmation"
no_secrets
python3 - "$HERE" "$tmp/validation.json" <<'PY' || fail "validation record not updated"
import hashlib, json, pathlib, re, sys
here = pathlib.Path(sys.argv[1]); ev = json.loads(pathlib.Path(sys.argv[2]).read_text())
tsha = hashlib.sha256((here / 'assistant-agent.template.json').read_bytes()).hexdigest()
dev = ev['provider']['applied']['development']
assert dev['versionId'] == 'agtvrsn_new0001' and dev['previousVersionId'] == 'agtvrsn_old0001', dev
assert dev['templateSha256'] == tsha and re.fullmatch('[0-9a-f]{64}', dev['governedSha256'])
assert ev['provider']['versionId'] == 'agtvrsn_new0001'
gate = ev['pendingGates']['agentTemplateAhead']
assert 'to: staging, production.' in gate and 'development (agtvrsn_new0001)' in gate, gate
PY

# 5. Second run is a no-op: nothing to send, record says it still matches.
: > "$tmp/state/log"
run development --apply || fail "no-op apply failed"
[[ "$(patches)" == 0 ]] || fail "no-op apply sent a PATCH"
grep -qF "matches the template" "$tmp/out" || fail "no-op not reported"

# 6. Same live agent recorded as staging, then production (confirmed): the gate closes.
run staging --apply && fail "applied to an agent whose name says another environment"
grep -qF "does not look like the staging agent" "$tmp/out" || fail "wrong-environment name not named"
rename "Genesy Staging"
run staging --apply || fail "staging record failed"
rename "Genesy Production"
ASSISTANT_AGENT_CONFIRM=production run production --apply || fail "confirmed production failed"
python3 - "$tmp/validation.json" <<'PY' || fail "gate not cleared after all three environments"
import json, sys
ev = json.load(open(sys.argv[1]))
assert 'agentTemplateAhead' not in ev['pendingGates'], ev['pendingGates']
assert set(ev['provider']['applied']) == {'development', 'staging', 'production'}
assert ev['provider']['stagingVersionId'] == ev['provider']['productionVersionId'] == 'agtvrsn_new0001'
PY

# 7. Shared tool: loud banner; staging apply refused without the production confirmation.
fixture 1
rename "Genesy Staging"
run staging --apply && fail "changed the shared tool without confirmation"
grep -qF "SHARED TOOL" "$tmp/out" || fail "no shared-tool banner"
grep -qF "Genesy Production" "$tmp/out" || fail "banner does not name the other agent"
[[ "$(patches)" == 0 ]] || fail "refused shared change still sent a PATCH"
no_secrets
ASSISTANT_AGENT_CONFIRM=production run staging --apply || fail "confirmed shared change failed"
grep -q '"path": "/v1/convai/tools/tool_EXECUTEFAKE00001"' <(grep PATCH "$tmp/state/log") || fail "shared tool not patched"

# 8. Provider drops a protected field: apply fails non-zero, record untouched.
fixture 0
echo drop_voice > "$tmp/state/mode"
run development --apply && fail "accepted a read-back that lost the voice"
grep -qF "conversation_config.tts.voice_id" "$tmp/out" || fail "lost protected field not named"
no_secrets
cmp -s "$HERE/assistant-validation.json" "$tmp/validation.json" || fail "failed apply wrote the record"

# 9. Provider ignores the PATCH: read-back mismatch fails non-zero.
fixture 0
echo ignore_patch > "$tmp/state/mode"
run development --apply && fail "accepted a read-back that differs from the template"
grep -qF "still differs" "$tmp/out" || fail "mismatch not reported"
cmp -s "$HERE/assistant-validation.json" "$tmp/validation.json" || fail "failed apply wrote the record"

# 10. Key through process substitution (the ssh path) and default env-file layout via GENESY_ROOT.
fixture 0
mkdir -p "$tmp/root/deploy/staging/secrets"
printf 'X=1\nELEVENLABS_AGENT_ID=%s\n' "$AGENT" > "$tmp/root/deploy/staging/.env.staging"
cp "$tmp/raw_key" "$tmp/root/deploy/staging/secrets/elevenlabs_api_key"
GENESY_ROOT="$tmp/root" ELEVENLABS_API_BASE="$BASE" ASSISTANT_VALIDATION_FILE="$tmp/validation.json" \
  bash "$HERE/assistant-agent.sh" staging > "$tmp/out" 2>&1 || fail "default staging layout failed"
grep -qF "max_duration_seconds: 600 → 1800" "$tmp/out" || fail "default layout dry run wrong"
ELEVENLABS_API_BASE="$BASE" ASSISTANT_AGENT_KEY_FILE=<(cat "$tmp/raw_key") \
  ASSISTANT_AGENT_ENV_FILE=<(printf 'ELEVENLABS_AGENT_ID=%s\n' "$AGENT") \
  bash "$HERE/assistant-agent.sh" staging > "$tmp/out" 2>&1 || fail "process-substituted key failed"
no_secrets

echo "assistant-agent-publish.test.sh: all tests passed"
