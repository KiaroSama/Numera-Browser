"""Original MCP method/result observations at the native-command execution seam."""
from __future__ import annotations
import json
from subprocess import CompletedProcess
from unittest.mock import patch
from baseline_source import activate
activate()

from numera import mcp


def cases(root):
    messages=[{'id':1,'method':'initialize'},
              {'id':'unicode','method':'initialize','params':{'protocolVersion':'2025-11-25'}},
              {'id':2,'method':'ping'}, {'method':'notifications/initialized'},
              {'id':3,'method':'unknown'}, {'method':'unknown'},
              {'id':4,'method':'tools/call','params':{'name':'absent'}},
              {'id':5,'method':'tools/call','params':{'name':'check','arguments':{'ref':'fixture'}}},
              {'id':6,'method':'tools/call','params':{'name':'list','arguments':{}}}]
    results=[]
    for message in messages:
        payload={'ok':False,'issues':['α فارسی 😀']} if message.get('id')==5 else {'total':0,'profiles':[]}
        exit=1 if message.get('id')==5 else 0
        with patch('subprocess.run',return_value=CompletedProcess([],exit,stdout=json.dumps(payload),stderr='')):
            response=mcp.handle(message)
        results.append({'message':message,'payload':payload,'exit':exit,'expected':response})
    import contextlib
    import io
    from numera.cli import main
    from numera.cli.out import set_json_mode
    calls=[]
    for message in ({'id':20,'method':'tools/call','params':{'name':'list'}},
                    {'id':21,'method':'tools/call','params':{'name':'show','arguments':{'ref':'missing'}}},
                    {'id':22,'method':'tools/call','params':{'name':'health'}}):
        def execute(cmd,**kwargs):
            index=cmd.index('--data-dir')
            output=io.StringIO()
            with contextlib.redirect_stdout(output):
                set_json_mode(True)
                code=main(cmd[index:])
            return CompletedProcess(cmd,code,stdout=output.getvalue(),stderr='')
        with patch('subprocess.run',side_effect=execute):
            expected=mcp.handle(message,data_dir=str(root))
        calls.append({'message':message,'expected':expected})
    return {'messages':results,'application':calls}
