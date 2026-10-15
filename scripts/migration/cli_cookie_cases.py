"""Original cookie CLI output/refusal policy; only browser SDK I/O is replaced."""
from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
from unittest.mock import patch
from baseline_source import activate
activate()

from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.models import Profile
from numera.store import ProfileStore
from numera.fingerprint import generate
from numera import cookies


def cases(parent, human=False, tty=False):
    parent=Path(parent);parent.mkdir(parents=True,exist_ok=True)
    root=parent/'store';store=ProfileStore(root)
    store.save(Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42)))
    row=cookies.normalize({'name':'fixture','value':'synthetic','domain':'.fixture.test'})
    (parent/'empty.json').write_text('[]',encoding='utf-8')
    (parent/'rows.json').write_text(json.dumps([row]),encoding='utf-8')
    operations=[['cookies','list','Fixture','--json'],
                ['cookies','import','Fixture','empty.json','--clear','--json'],
                ['cookies','import','Fixture','rows.json','--clear','--json'],
                ['cookies','export','Fixture','output.txt','--json'],
                ['cookies','export','Fixture','output.json','--json'],
                ['cookies','list','missing','--json'],
                ['cookies','import','missing','absent.json','--json']]
    from numera.cli import browser as handlers
    class Stream(io.StringIO):
        def isatty(self): return tty
    result=[];parser=build_parser()
    for original in operations:
        if human: original=[value for value in original if value!='--json']
        argv=list(original)
        if argv[1] in ('import','export'):
            argv[3]=str(parent/argv[3])
        args=parser.parse_args(argv);output=Stream();seen=[];set_json_mode(not human);captured=[]
        def emit(value,code=0): captured.append(value);return code
        def write(profile,store,jar,**kw):
            seen.append({'rows':jar,'clear':kw.get('clear'),'engine':kw.get('engine')})
            return len(jar)
        with contextlib.ExitStack() as stack:
            stack.enter_context(contextlib.redirect_stdout(output))
            stack.enter_context(patch.object(cookies,'read',return_value=[row]))
            stack.enter_context(patch.object(cookies,'write',side_effect=write))
            if human: stack.enter_context(patch.object(handlers,'emit',side_effect=emit))
            code=args.func(args,store)
        payload=captured[0] if human else json.loads(output.getvalue())
        if 'file' in payload:payload['file']=Path(payload['file']).name
        if 'error' in payload and payload['error'].startswith('no usable cookies found in '):
            payload['error']='no usable cookies found in '+Path(args.file).name
        produced=Path(args.file).read_text(encoding='utf-8') if argv[1]=='export' else None
        stdout=output.getvalue().replace(str(parent),'fixture-files') if human else None
        result.append({'argv':original,'exit':code,'payload':payload,'written':seen,'output':produced,'stdout':stdout,'tty':tty})
    return result
