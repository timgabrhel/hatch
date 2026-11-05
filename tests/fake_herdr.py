#!/usr/bin/python3
import json, os, sys
from pathlib import Path
args=sys.argv[1:]
log=Path(os.environ['HATCH_TEST_HERDR_LOG'])
with log.open('a') as f: f.write(json.dumps(args)+'\n')
state=Path(str(log)+'.state')
data=json.loads(state.read_text()) if state.exists() else {'workspaces':[], 'tabs':0}
mode=os.getenv('HATCH_TEST_HERDR_MODE','')
if mode=='unavailable': sys.exit(1)
if args[:2]==['workspace','list']:
    result={'workspaces':data['workspaces']}
elif args[:2]==['workspace','create']:
    identifier='w'+str(len(data['workspaces'])+1)
    data['workspaces'].append({'workspace_id':identifier,'label':'same label is not identity'})
    result={'workspace':{'workspace_id':identifier},'root_pane':{'pane_id':identifier+':p1'}}
elif args[:2]==['tab','create']:
    data['tabs']+=1
    identifier=args[args.index('--workspace')+1]
    result={'root_pane':{'pane_id':identifier+':p'+str(data['tabs']+1)}}
elif args[:2]==['pane','run']:
    if mode=='run-fails':
        print(json.dumps({'error':{'message':'fixture failure'}}))
        sys.exit(0)
    result={'accepted':True}
else:
    print('unexpected Herdr command',file=sys.stderr)
    sys.exit(2)
state.write_text(json.dumps(data))
if args[:2] != ['pane','run']:
    print(json.dumps({'result':result}))
