#!/usr/bin/python3
import json, os, sys
from pathlib import Path
with open(os.environ['HATCH_TEST_RESOLVER_LOG'],'a') as f:
    f.write(json.dumps(sys.argv[1:])+'\n')
mode=os.getenv('HATCH_TEST_RESOLVER_MODE','')
if sys.argv[1] == 'up':
    mode=os.getenv('HATCH_TEST_UP_MODE','')
    if mode == 'error':
        print('fixture build failed', file=sys.stderr)
        sys.exit(9)
    if mode == 'invalid':
        print('secret-invalid-result')
        sys.exit(0)
    if mode == 'unsuccessful':
        print(json.dumps({'outcome': 'error', 'containerId': 'a'*64}))
        sys.exit(0)
    fixture=Path(os.environ['HATCH_TEST_FIXTURE'])
    data=json.loads(fixture.read_text())
    labels=data[0]['Config']['Labels']
    if mode != 'wrong-target':
        labels['devcontainer.local_folder']=sys.argv[sys.argv.index('--workspace-folder')+1]
        labels['devcontainer.config_file']=sys.argv[sys.argv.index('--config')+1]
    data[0]['State']['Running']=True
    fixture.write_text(json.dumps(data))
    print(json.dumps({'outcome':'success','containerId':'a'*64}))
    sys.exit(0)
if mode=='error':
    print('super-secret-config-value',file=sys.stderr)
    sys.exit(1)
if mode=='invalid':
    print('super-secret-config-value')
    sys.exit(0)
value={"mergedConfiguration":{"workspaceFolder":"/custom/project", "remoteUser":"2000",
    "userEnvProbe":"none", "remoteEnv":{"HATCH_TOKEN":"super-secret-config-value", "HATCH_REMOVED":None}}}
if mode=='compose': value['mergedConfiguration']['service']='web'
if mode=='unresolved': value['mergedConfiguration']['remoteEnv']['HATCH_TOKEN']='${unknown:value}'
print(json.dumps(value))
