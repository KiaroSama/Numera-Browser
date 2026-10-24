"""Observe original cookie exports using non-sensitive synthetic rows only."""
from __future__ import annotations

from baseline_source import activate
activate()

from numera.cookies import dumps, to_netscape


def cases():
    base = {'name': 'fixture', 'value': 'synthetic', 'domain': '.fixture.test',
            'path': '/', 'expires': -1.0, 'secure': False, 'httpOnly': False}
    inputs = [
        ('empty', []),
        ('ordinary', [base]),
        ('flags', [dict(base, secure=True, httpOnly=True),
                   dict(base, domain='fixture.test', path='/account')]),
        ('unicode-controls', [dict(base, name='α فارسی 😀',
                                  value='tab\tline\ncarriage\r', path='/space path')]),
        ('duplicates', [base, base]),
    ]
    for expiry in (0, None, -0.0, 1.75, -1.75, 1893456000.5,
                   9223372036854775808, 10**40, '00123', '-00123', True):
        inputs.append((f'expiry-{len(inputs)}', [dict(base, expires=expiry)]))
    inputs.append(('defaults', [{'name': 'fixture', 'domain': 'fixture.test'}]))
    for invalid in ({'name': 'fixture'}, dict(base, domain=None),
                    dict(base, value=None), dict(base, expires='bad'),
                    dict(base, expires=[]), dict(base, expires={}),
                    dict(base, path=None)):
        inputs.append((f'invalid-{len(inputs)}', [invalid]))
    output = []
    for name, rows in inputs:
        try:
            expected = {'text': to_netscape(rows)}
        except (AttributeError, KeyError, TypeError, ValueError, OverflowError):
            expected = {'failed': True}
        output.append({'name': name, 'cookies': rows, 'expected': expected,
                       'json': dumps(rows)})
    return output
