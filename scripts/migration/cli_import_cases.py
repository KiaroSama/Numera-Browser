"""Original vendor CLI import decisions with fixed nondeterministic seed and IDs."""
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


def cases(parent):
    root=Path(parent)/'store';store=ProfileStore(root)
    variants=[('vendor.json',[{'name':'Vendor','os':'Linux','language':'de-DE','tags':'one, two','notes':'imported','proxy':{'type':'https','host':'proxy.test','port':8080,'user':'fixture-user','pass':'synthetic','country':'DE'}}]),
              ('vendor.json',[{'name':'Vendor','os':'Linux','language':'de-DE','tags':'one, two','notes':'imported','proxy':{'type':'https','host':'proxy.test','port':8080,'user':'fixture-user','pass':'synthetic','country':'DE'}}]),
              ('empty.json',[]),('native.json',{'numeraProfiles':1,'profiles':[]}),
              ('future.json',{'numeraProfiles':2,'profiles':[]})]
    for invalid in ({'timezone':{'unexpected':'zone'}},
                    {'webGLMetadata':{'vendor':{'unexpected':'vendor'}}},
                    {'proxy':{'type':'https','host':'proxy.test','port':8080,'user':7}},
                    {'proxy':{'type':'https','host':'proxy.test','port':8080,'pass':7}},
                    {'proxy':{'type':'https','host':'proxy.test','port':8080,'country':7}}):
        variants.append(('invalid.json',[{'name':'Must not save'},{'name':'Invalid',**invalid}]))
    identifiers=iter(('vendorfixture','duplicatefixture',*(f'invalidfixture{i}' for i in range(20))))
    def model(**fields):
        fields.update(id=next(identifiers),created_at=1000.0,updated_at=1000.0)
        return Profile(**fields)
    results=[]
    for filename,data in variants:
        path=Path(parent)/filename;path.write_text(json.dumps(data),encoding='utf-8')
        argv=['import-from',str(path),'--json'];args=build_parser().parse_args(argv);set_json_mode(True);output=io.StringIO()
        with contextlib.redirect_stdout(output),patch('numera.fingerprint.random.randint',return_value=42),patch('numera.importers.Profile',side_effect=model),patch('time.time',return_value=1000.0):
            try:
                code=args.func(args,store)
            except (RuntimeError, ValueError) as error:
                from numera.cli.out import emit
                code=emit({'ok':False,'error':str(error)},1)
        payload=json.loads(output.getvalue())
        if payload.get('error','').startswith("couldn't read "):
            payload['error']=payload['error'].replace(str(path),filename,1)
        rows=[profile.to_dict() for profile in store.list()]
        results.append({'file':filename,'input':data,'payload':payload,'exit':code,'profiles':rows,
                        'persisted_proxy':{kind:json.loads((root/kind/'vendorfixture.json').read_text(encoding='utf-8'))['proxy'] for kind in ('profiles','dashboard')}})
    return results
