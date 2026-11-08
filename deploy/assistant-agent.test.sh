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
execute=next(tool for tool in template['clientTools'] if tool['name']=='execute_genesy_tool')
assert execute['force_pre_tool_speech'] is True,'agent must acknowledge before running a business tool'
for line in ['Ok, got that','just finalizing','please read and approve the card','Oké, begrepen','lees de kaart en keur hem goed','Sawa, nimeelewa','tafadhali soma kadi na uidhinishe']:
    assert line in prompt,'four-beat talk-back script missing: '+line
assert 'call the same wait:true again' in prompt,'agent must keep waiting after a running progress result'
assert 'you may call execute_genesy_tool directly' in prompt and 'Effects still require discovering' in prompt,'direct lookup contract missing'
assert 'You cannot approve actions yourself.' in prompt and 'Approval codes and exact confirmation phrases are private' in prompt,'approval rules changed'
assert 'The top search result includes its full input schema' in prompt,'agent must execute after one search when the top match fits'
turn=config['conversation_config']['turn']
assert turn['turn_timeout']==20 and turn['silence_end_call_timeout']==60,'idle: one check-in after 20 s silence, end after 60 s'
assert 'never the same line twice' in prompt and 'after the third check-in' in prompt,'idle check-in rule missing'
assert 'Do not use audio tags' not in prompt and 'never read tags' not in prompt,'expressive audio tags stay allowed'
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
