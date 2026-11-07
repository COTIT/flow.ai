#!/usr/bin/env bash
# Exercise the Docker probe without a server or network.
set -Eeuo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$HERE/next.Dockerfile" <<'PY'
import json,os,pathlib,re,subprocess,sys
source=pathlib.Path(sys.argv[1]).read_text()
command=json.loads(re.search(r'CMD (\["node", "-e", .*\])',source)[1])
for app,status,expected in [('office',200,0),('office',404,1),('office',500,1),('shop',404,0),('shop',500,1)]:
    script='globalThis.fetch=async()=>({status:'+str(status)+'});'+command[2]
    result=subprocess.run(['node','-e',script],env={**os.environ,'GENESY_APP':app})
    assert result.returncode==expected,(app,status,result.returncode)
assert 'GENESY_APP=${APP}' in source
print('next-health.test.sh: all passed')
PY
