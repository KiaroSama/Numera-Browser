"""Original probe CLI narration, substituting only the browser result boundary."""
from __future__ import annotations

import io
from types import SimpleNamespace
from unittest.mock import patch

from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.cli import browser as handlers
from numera.models import Profile, Proxy
from numera.fingerprint import generate


def cases(probes, alignment):
    class Stream(io.StringIO):
        def __init__(self, tty):
            super().__init__()
            self.tty = tty

        def isatty(self):
            return self.tty

    parser = build_parser()
    rows = []
    fingerprint = generate(seed=42)
    inputs = [(row['operation'], row['expected'], None) for row in probes['cases']]
    inputs.extend(('proxy', row['expected'], row['profile']) for row in probes['network']['cases'])
    inputs.extend(('trust', row['expected'], row['profile']) for row in probes['trust']['cases'])
    inputs.extend(('align', row['expected'], row['profile']) for row in alignment['cases'])
    for operation in ['tls', 'dns', 'proxy']:
        inputs.append((operation, {'ok': False, 'error': 'Controlled probe failure'}, None))
    functions = {'tls': 'check_tls', 'dns': 'check_dns', 'proxy': 'check_proxy',
                 'trust': 'trust_report', 'align': 'align_to_reality'}
    for tty in [False, True]:
        for operation, result, record in inputs:
            profile = Profile(name='Fixture', id='fixture', fingerprint=fingerprint)
            if record and record.get('proxy'):
                profile.proxy = Proxy(**record['proxy'])
            argv = [operation, 'test', 'fixture'] if operation == 'proxy' else [operation, 'fixture']
            args = parser.parse_args(argv)
            stdout = Stream(tty)
            payload = []
            set_json_mode(False)

            def emit(value, code=0):
                payload.append(value)
                return code

            with patch('sys.stdout', stdout), patch.object(handlers, 'emit', side_effect=emit), \
                    patch(f'numera.probe.{functions[operation]}', return_value=result):
                code = args.func(args, SimpleNamespace(resolve=lambda ref: profile))
            rows.append({'argv': argv, 'tty': tty, 'name': profile.name,
                         'proxy': profile.proxy.server if profile.proxy else None,
                         'payload': payload[0] if payload else result, 'exit': code,
                         'stdout': stdout.getvalue()})
        for operation in ['tls', 'dns', 'proxy', 'trust', 'align']:
            argv = [operation, 'test', 'missing'] if operation == 'proxy' else [operation, 'missing']
            args = parser.parse_args(argv)
            stdout = Stream(tty)
            payload = []
            with patch('sys.stdout', stdout), patch.object(handlers, 'emit', side_effect=emit):
                code = args.func(args, SimpleNamespace(resolve=lambda ref: None))
            rows.append({'argv': argv, 'tty': tty, 'name': None, 'proxy': None,
                         'payload': payload[0], 'exit': code, 'stdout': stdout.getvalue()})
    return rows
