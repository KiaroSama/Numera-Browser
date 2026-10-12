"""Observe original pool handlers against isolated shared stores, no network."""
from __future__ import annotations
import contextlib
import io
import json
from pathlib import Path
from unittest.mock import patch
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.models import Profile
from numera.fingerprint import generate
from numera.store import ProfileStore


def cases(parent, human=False, tty=False):
    root=Path(parent)/'store'
    store=ProfileStore(root)
    with patch('time.time',return_value=1000.0):
        store.save(Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42),created_at=1000.0,updated_at=1000.0))
    source=Path(parent)/'proxies.txt'
    source.write_text('https://user:synthetic@[2001:db8::1]:8080:US\nhttp://plain.test:3128:DE\n',encoding='utf-8')
    invalid=Path(parent)/'invalid.txt'
    invalid.write_text('http://invalid.test:70000\n',encoding='utf-8')
    ids=iter(('poolone00000','pooltwo00000'))
    class UUID:
        def __init__(self): self.hex=next(ids)
    operations=[['pool','list','--json'],['pool','add',str(invalid),'--json'],
                ['pool','add',str(source),'--json'],['pool','add','-','--json'],
                ['pool','list','--json'],['pool','edit','poolone00000','--json'],
                ['pool','edit','poolone00000','--port','70000','--json'],
                ['pool','edit','poolone00000','--label','chosen','--pass','','--json'],
                ['pool','archive','poolone00000','missing','--json'],
                ['pool','archive','poolone00000','--restore','--json'],
                ['pool','export','--json'],['pool','test','--all','--json'],['pool','test','missing','--json'],
                ['pool','rotate','missing','--json'],['pool','rotate','poolone00000','--json'],
                ['pool','assign','--all','--json'],
                ['pool','remove','missing','--json'],['pool','remove','poolone00000','missing','--json'],
                ['pool','list','--json']]
    class Stream(io.StringIO):
        def isatty(self): return tty
    results=[]
    for argv in operations:
        if human: argv=[value for value in argv if value!='--json']
        args=build_parser().parse_args(argv)
        output=Stream();set_json_mode(not human);payload=[]
        def emit(value,code=0): payload.append(value);return code
        def check(proxy):
            return {'ok': True, 'ip': '192.0.2.1', 'country': 'US', 'city': None, 'latencyMs': 10} if proxy['host'] == '2001:db8::1' else {'ok': False, 'error': 'fixture network refusal'}
        with contextlib.ExitStack() as stack:
            stack.enter_context(contextlib.redirect_stdout(output))
            stack.enter_context(patch('numera.proxypool.check',side_effect=check))
            stack.enter_context(patch('sys.stdin',io.StringIO(source.read_text(encoding='utf-8'))))
            stack.enter_context(patch('uuid.uuid4',side_effect=UUID))
            stack.enter_context(patch('time.time',return_value=1000.0))
            if human: stack.enter_context(patch('numera.cli.pool.emit',side_effect=emit))
            code=args.func(args,store)
        row=payload[0] if human else json.loads(output.getvalue())
        fixture=store.get('fixture').to_dict()
        persisted={kind:json.loads((root/kind/'fixture.json').read_text(encoding='utf-8'))['proxy'] for kind in ('profiles','dashboard')}
        pool_path = root / 'proxies.json'
        pool = json.loads(pool_path.read_text(encoding='utf-8')) if pool_path.exists() else []
        from numera.devices import COUNTRY_TO_LOCALE
        assignment_locale=COUNTRY_TO_LOCALE.get((fixture.get('proxy') or {}).get('country','').upper())
        results.append({'argv':[Path(v).name if v in (str(source),str(invalid)) else v for v in argv],
                        'exit':code,'payload':row,'profile':fixture,'persisted_proxy':persisted,'pool':pool,
                        'stdout':output.getvalue() if human else None,'tty':tty,'assignmentLocale':assignment_locale})
    return results
