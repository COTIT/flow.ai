#!/usr/bin/env bash
# Temporary git history and database evidence; no deployment or database access.
set -Eeuo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$HERE" <<'PY'
import hashlib,json,pathlib,subprocess,sys,tempfile
script=pathlib.Path(sys.argv[1])/'assistant-compatibility.py'
reviewed=json.loads((script.parent/'assistant-compatibility.json').read_text())
assert len(reviewed['migrations'])==10,'review each new additive migration explicitly'
assert reviewed['invoiceDeliveryVersion']==1
assert reviewed['batchVersion']==1
assert reviewed['workflowVersion']==2
for path,checksum in reviewed['migrations'].items():
    assert hashlib.sha384((script.parent.parent/path).read_bytes()).hexdigest()==checksum,('reviewed migration changed',path)
with tempfile.TemporaryDirectory() as temporary:
    root=pathlib.Path(temporary)
    def git(*args):
        return subprocess.check_output(['git','-C',str(root),'-c','user.name=test','-c','user.email=test@test',*args],stderr=subprocess.DEVNULL).decode().strip()
    git('init','-q'); (root/'old').touch(); git('add','.'); git('commit','-qm','old'); unsupported=git('rev-parse','HEAD')
    for path in reviewed['migrations']:
        target=root/path; target.parent.mkdir(parents=True,exist_ok=True)
        target.write_bytes((script.parent.parent/path).read_bytes())
    module=root/'apps/api/src/modules/invoices/delivery.rs'; module.parent.mkdir(parents=True); module.touch()
    jobs=root/'apps/api/src/jobs/kinds.rs'; jobs.parent.mkdir(parents=True)
    jobs.write_text('INVOICE_DELIVERY => return crate::modules::invoices::delivery::run(ctx, &job).await,\nINVOICE_DELIVERY_SWEEP => return crate::modules::invoices::delivery::sweep(ctx).await,\n')
    for name in ("batches", "batch_repo", "batch_handlers", "batch_dto"):
        target=root/f'apps/api/src/modules/assistant/{name}.rs'; target.parent.mkdir(parents=True,exist_ok=True); target.touch()
    guard=root/'apps/api/src/http/assistant.rs'; guard.parent.mkdir(parents=True,exist_ok=True); guard.write_text('Assistant-Batch-Token batch_repo::claim(')
    (root/'apps/api/src/modules/assistant/repo.rs').write_text('batch_repo::expire( batch_repo::prune(')
    marker=root/'deploy/assistant-compatibility.json'; marker.parent.mkdir(); marker.write_text(json.dumps(reviewed))
    git('add','.'); git('commit','-qm','supported assistant'); previous=git('rev-parse','HEAD')
    (root/'old').write_text('new application release'); git('add','.'); git('commit','-qm','new app'); live=git('rev-parse','HEAD')
    before='20261106190652'; current='20261107000925'
    evidence=''.join(path.split('/')[-1][:14]+'|'+checksum+'\n' for path,checksum in reviewed['migrations'].items() if int(before)<int(path.split('/')[-1][:14])<=int(current))
    def check(evidence=evidence,saver='5',prev=previous,now=live,before=before):
        return subprocess.run(['python3',str(script),str(root),now,prev,before,current,saver],input=evidence,text=True,capture_output=True)
    result=check(); assert result.returncode==0,result.stderr
    assert result.stdout.strip()=='office-assistant-v1/2/1.1.2/assistant_checkpoint/5/1/1'
    # Phase 5 window (staging dd6678bfe at 20261109191001 -> gap demand): its one reviewed function migration.
    gap='apps/api/migrations/20261110120000_assistant_gap_demand.up.sql'
    window=subprocess.run(['python3',str(script),str(root),live,previous,'20261109191001','20261110120000','5'],input='20261110120000|'+reviewed['migrations'][gap]+'\n',text=True,capture_output=True)
    assert window.returncode==0,window.stderr
    assert check(prev=unsupported).returncode!=0,'pre-assistant target without invoice worker accepted'
    assert check(prev=unsupported,before=current,evidence='').returncode!=0,'same-schema pre-assistant target accepted'
    assert check(before=current,evidence='\n').returncode==0,'supported same-schema app rollback rejected'
    assert check(evidence=evidence+'20261106204413|bad\n').returncode!=0,'changed or duplicate checksum accepted'
    assert check(evidence=evidence+'20261105123456|unknown\n').returncode!=0,'unreviewed business migration accepted'
    assert check(evidence='').returncode!=0,'missing database evidence accepted'
    assert check(saver='6').returncode!=0,'unknown saver schema accepted'
    def revision(change):
        git('checkout','-q',live); change(); git('add','.'); git('commit','-qm','incompatible'); return git('rev-parse','HEAD')
    missing_module=revision(lambda: module.unlink())
    assert check(prev=missing_module).returncode!=0,'marker-only target without invoice delivery module accepted'
    assert check(now=missing_module).returncode!=0,'live release without invoice delivery module accepted'
    missing_dispatch=revision(lambda: jobs.write_text('// worker not registered\n'))
    assert check(prev=missing_dispatch).returncode!=0,'target without invoice worker dispatch accepted'
    assert check(prev=missing_dispatch,before=current,evidence='').returncode!=0,'same-schema target without worker accepted'
    def change_marker(key,value):
        changed=dict(reviewed); changed[key]=value; marker.write_text(json.dumps(changed))
    assert check(prev=revision(lambda: change_marker('invoiceDeliveryVersion',None))).returncode!=0,'unversioned invoice worker accepted'
    assert check(prev=revision(lambda: change_marker('batchVersion',None))).returncode!=0,'unversioned batch admission accepted'
    assert check(prev=revision(lambda: change_marker('workflowVersion',1))).returncode!=0,'different graph fingerprint accepted'
    assert check(prev=revision(lambda: guard.write_text('// missing controller'))).returncode!=0,'batch target without guard accepted'
    assert check(prev=revision(lambda: (root/'apps/api/src/modules/assistant/repo.rs').write_text('// missing cleanup')),before=current,evidence='').returncode!=0,'same-schema batch target without cleanup accepted'
    assert check(prev=revision(lambda: (root/'apps/api/src/modules/assistant/batch_repo.rs').unlink())).returncode!=0,'batch target without repository accepted'
    first=root/next(iter(reviewed['migrations']))
    changed_source=revision(lambda: first.write_text(first.read_text()+'-- changed\n'))
    assert check(now=changed_source).returncode!=0,'changed reviewed migration source accepted'
print('assistant-compatibility.test.sh: all passed')
PY
