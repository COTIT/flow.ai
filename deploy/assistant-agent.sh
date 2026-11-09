#!/usr/bin/env bash
# Publishes the reviewed ElevenLabs agent config (deploy/assistant-agent.template.json) to one environment's
# agent. Default is a dry run (GET only) that prints the diff of the governed fields; --apply PATCHes only
# those fields, reads back, and records the new version in assistant-validation.json (ASSISTANT.md).
#   bash deploy/assistant-agent.sh <development|staging|production> [--apply]
# Key: a file, never argv: ASSISTANT_AGENT_KEY_FILE, else development apps/office/.env.local
# (ELEVENLABS_API_KEY=), staging/production $GENESY_ROOT/deploy/<env>/secrets/elevenlabs_api_key.
# Agent id: ELEVENLABS_AGENT_ID, else ELEVENLABS_AGENT_ID= in ASSISTANT_AGENT_ENV_FILE or the env file.
# Production --apply, and any change to a tool other agents share, need ASSISTANT_AGENT_CONFIRM=production.
set -Eeuo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  echo "usage: $0 <development|staging|production> [--apply]" >&2
  exit 2
}
die() {
  echo "refused: $*" >&2
  exit 1
}

env="${1:-}"
mode="${2:-}"
(($# <= 2)) || usage
case "$env" in
  development) env_file="$HERE/../apps/office/.env.local" key_file="$env_file" ;;
  staging | production)
    root="${GENESY_ROOT:-/srv/genesy-$([[ "$env" == production ]] && echo prod || echo staging)}"
    env_file="$root/deploy/$env/.env.$env" key_file="$root/deploy/$env/secrets/elevenlabs_api_key" ;;
  *) usage ;;
esac
[[ -z "$mode" || "$mode" == --apply ]] || usage
if [[ "$mode" == --apply && "$env" == production && "${ASSISTANT_AGENT_CONFIRM:-}" != production ]]; then
  die "production --apply needs ASSISTANT_AGENT_CONFIRM=production on every run"
fi
key_file="${ASSISTANT_AGENT_KEY_FILE:-$key_file}"
env_file="${ASSISTANT_AGENT_ENV_FILE:-$env_file}"
# -e, not -s: a process-substituted pipe (<(ssh …)) must not be read twice.
[[ -e "$key_file" ]] || die "no key file at $key_file; set ASSISTANT_AGENT_KEY_FILE (ASSISTANT.md)"

python3 - "$HERE" "$env" "$mode" "$key_file" "$env_file" <<'PY'
import copy, datetime, hashlib, json, os, pathlib, re, sys, urllib.error, urllib.parse, urllib.request

here, env, mode, key_file, env_file = pathlib.Path(sys.argv[1]), *sys.argv[2:]
apply = mode == '--apply'
confirmed = os.environ.get('ASSISTANT_AGENT_CONFIRM') == 'production'
ENVS = ['development', 'staging', 'production']
MISSING = object()
# Environment-specific: never in the PATCH, and must read back byte-identical after it.
PROTECTED = [('conversation_config', 'tts', 'voice_id'), ('conversation_config', 'agent', 'prompt', 'tool_ids'),
             ('platform_settings', 'auth', 'allowlist'), ('platform_settings', 'auth', 'require_origin_header'),
             ('platform_settings', 'call_limits'), ('name',)]
EXCLUDED = {('platform_settings', 'auth', 'require_origin_header')}
VOLATILE = {'version_id', 'metadata', 'branch_id', 'main_branch_id'}

def die(message):
    print('refused: ' + message, file=sys.stderr); sys.exit(1)

def env_value(text, name):
    found = [line.split('=', 1)[1].strip().strip('"\'') for line in text.splitlines() if line.startswith(name + '=')]
    return found[-1] if found else ''

base = os.environ.get('ELEVENLABS_API_BASE', 'https://api.elevenlabs.io')
if base != 'https://api.elevenlabs.io':
    parsed = urllib.parse.urlsplit(base)  # test override only: the key never leaves this machine
    if parsed.scheme != 'http' or parsed.hostname != '127.0.0.1': die('ELEVENLABS_API_BASE may only point at 127.0.0.1')
raw = pathlib.Path(key_file).read_text()
key = env_value(raw, 'ELEVENLABS_API_KEY') if 'ELEVENLABS_API_KEY=' in raw else raw.strip()
if not re.fullmatch(r'[A-Za-z0-9_-]{8,200}', key): die('the key file holds no usable ElevenLabs key')
agent_id = os.environ.get('ELEVENLABS_AGENT_ID', '')
if not agent_id and pathlib.Path(env_file).exists():
    agent_id = env_value(pathlib.Path(env_file).read_text(), 'ELEVENLABS_AGENT_ID')
if not re.fullmatch(r'[A-Za-z0-9_-]{1,200}', agent_id):
    die('no agent id: set ELEVENLABS_AGENT_ID or ASSISTANT_AGENT_ENV_FILE (ASSISTANT.md)')

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args): return None
opener = urllib.request.build_opener(NoRedirect)

def call(method, path, label, body=None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(base + path, data=data, method=method,
                                     headers={'xi-api-key': key, 'Content-Type': 'application/json'})
    try:
        with opener.open(request, timeout=30) as response:
            raw = response.read(5_000_001)
    except urllib.error.HTTPError as error:
        die('%s %s answered HTTP %s' % (method, label, error.code))
    except (urllib.error.URLError, OSError) as error:
        die('%s %s failed: %s' % (method, label, type(error).__name__))
    if len(raw) > 5_000_000: die('%s %s answered too much data' % (method, label))
    return json.loads(raw or b'{}')

def leaves(value, prefix=()):
    if isinstance(value, dict) and value:
        for k, child in value.items(): yield from leaves(child, prefix + (k,))
    else:
        yield prefix, value

def get(value, path):
    for k in path:
        if not isinstance(value, dict) or k not in value: return MISSING
        value = value[k]
    return value

def put(value, path, leaf):
    for k in path[:-1]: value = value.setdefault(k, {})
    value[path[-1]] = leaf

def show(value):
    if value is MISSING: return '(unset)'
    text = json.dumps(value, ensure_ascii=False)
    if isinstance(value, str) and (len(value) > 80 or '\n' in value):
        return '<%d chars sha256 %s>' % (len(value), hashlib.sha256(value.encode()).hexdigest()[:12])
    return text if len(text) <= 120 else '<json sha256 %s>' % hashlib.sha256(text.encode()).hexdigest()[:12]

def canonical(value): return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)

template_raw = (here / 'assistant-agent.template.json').read_bytes()
template = json.loads(template_raw)
template_sha = hashlib.sha256(template_raw).hexdigest()
governed = [(p, v) for p, v in leaves(template['configuration']) if p not in EXCLUDED]
tool_governed = {t['name']: list(leaves(t)) for t in template['clientTools']}

def observe():
    agent = call('GET', '/v1/convai/agents/' + agent_id, 'agent')
    tools = {}
    for tool_id in get(agent, ('conversation_config', 'agent', 'prompt', 'tool_ids')) or []:
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,200}', str(tool_id)): die('agent lists a malformed tool id')
        tool = call('GET', '/v1/convai/tools/' + tool_id, 'tool')
        name = (tool.get('tool_config') or {}).get('name')
        if name in tool_governed:
            if name in tools: die('agent has two tools named %s' % name)
            tools[name] = (tool_id, tool['tool_config'])
    for name in tool_governed:
        if name not in tools: die('agent has no client tool named %s; attach it first (ASSISTANT.md)' % name)
    return agent, tools

def diff(agent, tools):
    changes = [(p, get(agent, p), v) for p, v in governed if get(agent, p) != v]
    tool_changes = {name: [(p, get(tools[name][1], p), v) for p, v in fields if get(tools[name][1], p) != v]
                    for name, fields in tool_governed.items()}
    return changes, {name: c for name, c in tool_changes.items() if c}

def projection(agent, tools):
    return {'agent': {'.'.join(p): get(agent, p) for p, _ in governed},
            'tools': {name: {'.'.join(p): get(tools[name][1], p) for p, _ in fields}
                      for name, fields in tool_governed.items()}}

before, before_tools = observe()
name = str(before.get('name', ''))
print('%s: agent "%s", version %s%s' % (env, name, before.get('version_id'), '' if apply else ' (dry run, GET only)'))
expected = {'development': r'develop|evaluation', 'staging': r'staging'}
wrong_env = (not re.search(expected[env], name, re.I)) if env in expected else re.search(r'develop|evaluation|staging', name, re.I)
if wrong_env:
    message = 'agent name "%s" does not look like the %s agent' % (name, env)
    if apply: die(message)
    print('warning: ' + message)
changes, tool_changes = diff(before, before_tools)
for path, old, new in changes: print('  %s: %s → %s' % ('.'.join(path), show(old), show(new)))
shared = []
for tool, fields in tool_changes.items():
    for path, old, new in fields: print('  tool %s: %s: %s → %s' % (tool, '.'.join(path), show(old), show(new)))
    deps = call('GET', '/v1/convai/tools/%s/dependent-agents' % before_tools[tool][0], 'tool dependents')
    others = [str(a.get('name') or 'an unnamed agent') for a in deps.get('agents', []) if a.get('id') != agent_id]
    if others or deps.get('has_more'):
        shared.append(tool)
        print('!!! SHARED TOOL: %s is also used by %s%s.' % (tool, ', '.join(others) or 'other agents', ' and more' if deps.get('has_more') else ''))
        print('!!! Changing it changes those agents at the same time (Production and Staging share one).')
if not changes and not tool_changes:
    print('  matches the template on %d governed fields and %d tools' % (len(governed), len(tool_governed)))
if not apply:
    if changes or tool_changes: print('dry run: nothing sent. Review the diff, then re-run with --apply.')
    sys.exit(0)
if shared and not confirmed:
    die('changing a shared tool needs ASSISTANT_AGENT_CONFIRM=production (it changes Production too)')

if changes:
    body = {}
    for path, _, new in changes: put(body, path, copy.deepcopy(new))
    call('PATCH', '/v1/convai/agents/' + agent_id, 'agent', body)
for tool, fields in tool_changes.items():
    tool_id, config = before_tools[tool]
    config = copy.deepcopy(config)  # PATCH needs the whole tool_config; only governed fields change
    for path, _, new in fields: put(config, path, copy.deepcopy(new))
    call('PATCH', '/v1/convai/tools/' + tool_id, 'tool', {'tool_config': config})

after, after_tools = observe()
lost = ['.'.join(p) for p in PROTECTED if get(before, p) != get(after, p)]
if lost: die('environment-specific fields changed by the update: %s. Restore them by hand now.' % ', '.join(lost))
left, left_tools = diff(after, after_tools)
if left or left_tools:
    paths = ['.'.join(p) for p, _, _ in left] + ['tool %s: %s' % (t, '.'.join(p)) for t, f in left_tools.items() for p, _, _ in f]
    die('read-back still differs from the template: %s' % ', '.join(paths))
skip = {p for p, _ in governed} | set(PROTECTED)
def outside(agent): return {p: v for p, v in leaves(agent) if p[0] not in VOLATILE and p not in skip}
old_outside, new_outside = outside(before), outside(after)
drift = sorted('.'.join(p) for p in set(old_outside) | set(new_outside) if old_outside.get(p, MISSING) != new_outside.get(p, MISSING))
for path in drift[:20]: print('warning: changed outside the template (provider default?): ' + path)
version = after.get('version_id')
if changes and version == before.get('version_id'):
    die('provider kept version %s after the update' % version)
print('%s: read-back matches the template; version %s' % (env, version))

record = pathlib.Path(os.environ.get('ASSISTANT_VALIDATION_FILE') or here / 'assistant-validation.json')
evidence = json.loads(record.read_text())
provider = evidence['provider']
now = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.000Z')
provider.setdefault('applied', {})[env] = {
    'versionId': version, 'previousVersionId': before.get('version_id'),
    'governedSha256': hashlib.sha256(canonical(projection(after, after_tools)).encode()).hexdigest(),
    'templateSha256': template_sha, 'observedAt': now, 'by': 'deploy/assistant-agent.sh'}
current, previous = {'development': ('versionId', 'previousVersionId'),
                     'staging': ('stagingVersionId', 'previousStagingVersionId'),
                     'production': ('productionVersionId', 'previousProductionVersionId')}[env]
if provider.get(current) != version:
    provider[previous], provider[current] = provider.get(current), version
if env != 'development': provider[env + 'Applied'] = True
evidence['observedAt'] = now
pending = [e for e in ENVS if provider['applied'].get(e, {}).get('templateSha256') != template_sha]
if pending:
    done = ['%s (%s)' % (e, provider['applied'][e]['versionId']) for e in ENVS if e not in pending]
    evidence['pendingGates']['agentTemplateAhead'] = (
        'Template %s is not yet published by deploy/assistant-agent.sh to: %s. Published and read back: %s.'
        % (template_sha[:12], ', '.join(pending), ', '.join(done) or 'none'))
else:
    evidence['pendingGates'].pop('agentTemplateAhead', None)
record.write_text(json.dumps(evidence, indent=2, ensure_ascii=False) + '\n')
print('recorded %s in %s; commit it' % (env, record.name))
PY
