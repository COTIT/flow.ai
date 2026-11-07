#!/usr/bin/env bash
# Offline snapshot integrity and redaction; never reads credentials or calls the provider.
set -Eeuo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$HERE" <<'PY'
import hashlib,json,pathlib,re,sys
root=pathlib.Path(sys.argv[1]); raw=(root/'assistant-agent.template.json').read_bytes()
template=json.loads(raw); evidence=json.loads((root/'assistant-validation.json').read_text())
assert evidence['provider']['templateSha256']==hashlib.sha256(raw).hexdigest(),'agent template changed without a new observed validation record'
assert evidence['releaseGate']=='pending' and evidence['releaseCommit'] is None
assert evidence['provider']['versionId'].startswith('agtvrsn_')
source=(root.parent/'apps/office/components/voice/provider.tsx').read_text()
assert len(template['clientTools'])==2
for tool in template['clientTools']:
    assert tool['type']=='client' and tool['name']+': (' in source,'snapshot tool not implemented by Office'
    assert tool['expects_response'] is True
    assert tool['parameters']['type']=='object'
assert not any(value in raw.decode() for value in ['https://','http://','xi-api-key','shareable_token'])
assert not re.search(r'\b(?:agent_|tool_|voice_|conv_)[A-Za-z0-9]{10,}',raw.decode()),'private provider binding copied into template'
assert set(template)=={'formatVersion','configuration','clientTools'}
config=template['configuration']; agent=config['conversation_config']['agent']
assert not agent['prompt'].get('custom_llm')
assert not {'tool_ids','knowledge_base'} & agent['prompt'].keys()
prompt=agent['prompt']['prompt']
assert 'assistant_batch_prepare' in prompt and 'assistant_batch_status' in prompt,'agent prompt missing fixed-batch preparation/status contract'
assert 'two through ten' in prompt and 'operationId' in prompt and 'arguments_json' in prompt,'agent batch arguments are unspecified'
assert 'paused' in prompt and 'never resume' in prompt,'agent must stop paused batches without replay'
assert set(config['conversation_config']['language_presets'])=={'nl','sw'}
assert config['conversation_config']['conversation']['file_input']=={'enabled':True,'max_files_in_memory':10,'max_files_per_conversation':10}
privacy=config['platform_settings']['privacy']
assert privacy['record_voice'] is False and privacy['retention_days']==0
assert privacy['delete_audio'] is True and privacy['delete_transcript_and_pii'] is True
assert config['platform_settings']['auth']['enable_auth'] is True
def inspect(value):
    if isinstance(value,dict):
        for key,child in value.items():
            assert key not in {'agent_id','branch_id','version_id','voice_id','tool_id','shareable_token','api_key','secret','webhook','knowledge_base'} or isinstance(child,bool),'private binding copied into template'
            inspect(child)
    elif isinstance(value,list):
        for child in value: inspect(child)
inspect(template)
print('assistant-agent.test.sh: sanitized snapshot and pending release evidence passed')
PY
