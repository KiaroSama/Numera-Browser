"""Original await_ready results without spawning a browser or user process."""
from __future__ import annotations
import json
import tempfile
from pathlib import Path
from numera.handshake import await_ready, StartupFailed

class Exited:
    def __init__(self,code): self.code=code
    def poll(self): return self.code


def cases(parent: Path)->list[dict]:
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    variants=[('ready',{'ok':True},None),('failed',{'ok':False,'error':'fixture failure','kind':'locked'},None),
              ('default-failure',{'ok':False},None),('exited',None,7),('exited-ready',{'ok':True},0),
              ('exited-failure',{'ok':False,'kind':'gone','error':'fixture missing'},1),
              ('timeout',None,None),('damaged','{',None),('wrong-container',[],None)]
    for name,payload,code in variants:
        with tempfile.TemporaryDirectory(prefix='ready-oracle-',dir=parent) as directory:
            path=Path(directory)/'ready.json'
            if payload is not None: path.write_text(payload if isinstance(payload,str) else json.dumps(payload),encoding='utf-8')
            try:
                await_ready(Exited(code),path,0)
                outcome={'ok':True}
            except StartupFailed as error:
                outcome={'ok':False,'kind':error.kind,'error':str(error)}
            found.append({'name':name,'payload':payload,'exit':code,'timeout':0,'expected':outcome})
    return found
