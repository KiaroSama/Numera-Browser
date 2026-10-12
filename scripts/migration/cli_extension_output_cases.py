"""Original extension narration with real isolated profile/config stores."""
from __future__ import annotations

import io
import json
from pathlib import Path
from unittest.mock import patch
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.cli import setup as handlers
from numera.store import ProfileStore
from numera.models import Profile
from numera.fingerprint import generate


def cases(parent):
    class Stream(io.StringIO):
        def __init__(self,tty): super().__init__();self.tty=tty
        def isatty(self): return self.tty
    parent=Path(parent)
    extension=parent/'extension';extension.mkdir(parents=True,exist_ok=True)
    (extension/'manifest.json').write_text(json.dumps({'manifest_version':3,'name':'Fixture','version':'1'}),encoding='utf-8')
    missing=parent/'missing-extension'
    operations=[['ext','list','fixture'],['ext','list','missing'],
                ['ext','add','fixture',str(extension)],['ext','add','fixture',str(extension)],
                ['ext','list','fixture'],['ext','remove','fixture',str(missing)],
                ['ext','remove','fixture',str(extension)],['ext','add','fixture',str(missing)],
                ['ext','default-list'],['ext','default-add',str(extension)],['ext','default-add',str(extension)],
                ['ext','default-list'],['ext','default-remove',str(missing)],['ext','default-remove',str(extension)],
                ['ext','installed'],['ext','uninstall','abcdefghijklmnopabcdefghijklmnop']]
    result=[];parser=build_parser()
    for tty in [False,True]:
        store=ProfileStore(parent/f'store-{tty}')
        profile=Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42));store.save(profile)
        for argv in operations:
            args=parser.parse_args(argv);stdout=Stream(tty);payload=[];set_json_mode(False)
            def emit(value,code=0): payload.append(value);return code
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit):
                code=args.func(args,store)
            result.append({'argv':argv,'tty':tty,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code,'name':profile.name,'home':str(parent)})
    return result
