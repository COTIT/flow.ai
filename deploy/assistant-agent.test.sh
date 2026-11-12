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
# Written by assistant-agent.sh --apply after a read-back; one entry per environment.
for env,entry in evidence['provider'].get('applied',{}).items():
    assert env in {'development','staging','production'} and entry['versionId'].startswith('agtvrsn_')
    assert re.fullmatch('[0-9a-f]{64}',entry['governedSha256']) and re.fullmatch('[0-9a-f]{64}',entry['templateSha256'])
source=(root.parent/'packages/voice/src/components/provider.tsx').read_text()
assert len(template['clientTools'])==3
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
assert 'two through 25' in prompt and 'operationId' in prompt and 'arguments_json' in prompt,'agent batch arguments are unspecified'
assert 'paused' in prompt and 'never resume' in prompt,'agent must stop paused batches without replay'
execute=next(tool for tool in template['clientTools'] if tool['name']=='execute_genesy_tool')
assert execute['force_pre_tool_speech'] is False,'one acknowledgement per user turn comes from the prompt, not a forced line before every tool call'
assert 'once per user turn' in prompt and 'nothing between tool calls' in prompt,'agent must acknowledge once per turn, then stay quiet between tool calls'
assert "language of the user's current message" in prompt,'narration and answer follow the language of the current message, not an earlier turn'
assert '[Review honesty]' in prompt and '500 cents or $500?' in prompt,'review honesty block missing'
assert 'I cancelled the card because you sent a new message' in prompt,'cancel-on-new-message line missing'
assert 'Never answer a question about an open card by reading out its confirmation phrase' in prompt,'phrase echo guard missing'
assert '[Discoverability]' in prompt and '[/Discoverability]' in prompt,'discoverability block missing'
assert 'Each account is private' in prompt and 'supply link with consent' in prompt,'account privacy line missing'
assert '[Opening screens]' in prompt and 'open_flow_view' in prompt and 'never changes anything' in prompt,'P4 opening-screens block missing'
open_view=next(tool for tool in template['clientTools'] if tool['name']=='open_flow_view')
assert list(open_view['parameters']['properties'])==['view'] and open_view['parameters']['required']==['view'],'open_flow_view takes only a view'
assert 'Till refunds are done on the till' in prompt and 'Kassaretouren gaan via de kassa' in prompt and 'Marejesho ya mauzo ya kaunta' in prompt,'till refund wording missing'
assert 'meta.open_totals' in prompt and 'never add amounts yourself' in prompt,'owe rule must use backend totals'
assert 'up to 25 exact actions' in prompt and '"remaining"' in prompt,'bulk continuation sentence missing'
assert 'use stock_transfer' in prompt,'in-account stock transfer hint missing'
assert 'prepare scheduled_reports_create' in prompt,'recurring request hint missing'
assert 'workspace rate as of' in prompt and 'currency_rates_convert' in prompt,'currency conversion hint missing'
assert 'Never ask the user for an ID, UUID or workspace id' in prompt,'agent must never ask the user for ids'
assert 'Say field names as plain words' in prompt and 'never in backticks' in prompt,'field names must be spoken as plain words'
assert 'granted by the farm co-op from its own Genesy account' in prompt and 'supply_links_accept' in prompt,'supply link explanation missing'
assert 'farm_crops_list' in prompt and 'farm_supply_options for that village and crop' in prompt,'farmer delivery lookup order missing'
assert 'If farm_supply_options lists no harvests, do not prepare farm_supply_create' in prompt and 'must be bought by another ops or admin member' in prompt,'empty harvest list must stop the delivery with a plain reason'
assert 'do not prepare it yet: convert that part with currency_rates_convert' in prompt,'mixed-currency payment must be converted and asked first'
assert 'end with one direct question' in prompt,'missing or mismatched file must end in a question'
assert 'never revert anything by yourself' in prompt,'undo must be a new review card'
assert 'meta.total for how many' in prompt,'counts must come from meta.total'
assert '[File import]' in prompt and 'never copy prices yourself' in prompt,'file import block missing'
# Talk-back is style guidance, not a script: fixed lines made the agent sound robotic.
for script in ['Ok, got that, let me check it for you.','Oké, begrepen, ik zoek het even voor je uit.','Sawa, nimeelewa, niruhusu nikuangalie.','Approved, just finalizing.','Here is what I\'ll do, please read and approve the card.']:
    assert script not in prompt,'talk-back must not be a fixed script: '+script
for rule in ['only when a lookup will take a moment','never the same opener twice in a row','say nothing before an instant answer','illustrations only, never repeat them verbatim']:
    assert rule in prompt,'talk-back style rule missing: '+rule
assert prompt.count('never repeat them verbatim')>=3,'acknowledgement, progress and review examples must all be marked as illustrations'
assert 'never read an approval code or phrase aloud and never approve yourself' in prompt,'talk-back must keep the approval rules'
agent_overrides=config['platform_settings']['overrides']['conversation_config_override']['agent']
assert agent_overrides['first_message'] is True,"Office's contextual greeting needs the first_message override"
assert agent_overrides['prompt']['prompt'] is False and agent_overrides['language'] is True,'only the greeting and language may be overridden'
firsts=[agent['first_message']]+[preset['overrides']['agent']['first_message'] for preset in config['conversation_config']['language_presets'].values()]
assert all(0<len(text)<=60 for text in firsts),'the fallback greeting stays short and neutral'
assert 'call the same wait:true again' in prompt,'agent must keep waiting after a running progress result'
assert 'you may call execute_genesy_tool directly' in prompt and 'Effects still require discovering' in prompt,'direct lookup contract missing'
assert 'You cannot approve actions yourself.' in prompt and 'Approval codes and exact confirmation phrases are private' in prompt,'approval rules changed'
assert 'The top search result includes its full input schema' in prompt,'agent must execute after one search when the top match fits'
assert '[Results on screen]' in prompt and 'never read the points or items one by one' in prompt,'summary rule for pictures on screen missing'
assert 'say why in one sentence and offer the closest kind' in prompt,'refused chart kind must be explained with the closest fit'
assert '[Flow context]' in prompt and '[/Flow context]' in prompt and 'UNTRUSTED UI state' in prompt,'Flow context block missing'
assert 'saved period' in prompt,'Flow context period caveat missing'
assert 'replaces' in prompt and 'presentation' in prompt and 'Reads stay reads' in prompt,'change-in-place contract missing'
assert 'Never invent data to fit a chart' in prompt,'charts must never fake data'
kinds=['line','sparkline','bar','hbar','pie','donut','kpi','delta','meter','table','list','timeline','map']
for shape,allowed in {'series':'line, sparkline, bar, table','totals':'hbar, bar, kpi, table','comparison':'delta, bar, hbar, kpi, table','breakdown':'hbar, bar, pie, donut, table, list','share':'list, bar, hbar, pie, donut, meter, table','counts':'list, bar, hbar, pie, donut, kpi, table','rows':'list, kpi, table','events':'timeline, list, table'}.items():
    assert f'{shape}: {allowed}' in prompt,'chart kinds per data shape missing: '+shape
assert 'flow_scenes_update' in prompt and 'review card' in prompt,'scene chart changes must stay a reviewed write'
params=execute['parameters']
assert params['required']==['operation_id','arguments_json'],'presentation and replaces stay optional'
assert params['properties']['presentation']['type']=='string' and params['properties']['presentation']['enum']==kinds,'presentation must offer every chart kind'
assert params['properties']['replaces']['type']=='string','replaces names the picture to redraw'
assert 'presentation, replaces' in source,'Office must forward presentation and replaces'
assert 'superseded or cancelled has no answer' in prompt and 'never retry it' in prompt,'a superseded or cancelled tool result must be silent, never retried'
turn=config['conversation_config']['turn']
assert turn['turn_timeout']==20 and turn['silence_end_call_timeout']==180,'idle: one check-in after 20 s silence, end after 180 s'
assert config['conversation_config']['conversation']['max_duration_seconds']==1800,'long tool chains need a 30-minute conversation limit'
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
