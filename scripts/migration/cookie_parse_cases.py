"""Observe original Netscape parsing with synthetic, non-sensitive text."""
from __future__ import annotations

from baseline_source import activate
activate()

from numera.cookies import parse, parse_netscape


def cases():
    row = '.fixture.test\tTRUE\t/account\tTRUE\t1893456000.5\tfixture\tsynthetic'
    inputs = [('', ''), ('ordinary', row), ('http-only', '#HttpOnly_' + row),
              ('comment', '# ignored\n' + row), ('whitespace-fallback', row.replace('\t', ' ')),
              ('extra-columns', row + '\tignored'), ('empty-value', row.rsplit('\t', 1)[0] + '\t'),
              ('empty-name', row.replace('\tfixture\t', '\t\t')),
              ('negative-expiry', row.replace('1893456000.5', '-1')),
              ('bad-expiry', row.replace('1893456000.5', 'not-a-date')),
              ('arabic-digits', row.replace('1893456000.5', '١٢٣.٥')),
              ('superscript-refusal', row.replace('1893456000.5', '²')),
              ('dot-only', row.replace('1893456000.5', '.')),
              ('unicode', row.replace('fixture\t', 'α فارسی 😀\t'))]
    for boundary in ('\r\n', '\r', '\v', '\f', '\x1c', '\x1d', '\x1e', '\x85', ' ', ' '):
        inputs.append((f'line-{len(inputs)}', row + boundary + row))
    output = []
    for name, text in inputs:
        try:
            expected = {'cookies': parse_netscape(text)}
        except (TypeError, ValueError, OverflowError):
            expected = {'failed': True}
        output.append({'name': name, 'text': text, 'expected': expected})
    return output


def whitespace_cases():
    import json

    cookie = {'name': 'fixture', 'value': 'synthetic', 'domain': '.fixture.test'}
    row = '.fixture.test\tTRUE\t/account\tTRUE\t1893456000\tfixture\tsynthetic'
    document = json.dumps(cookie)
    inputs = []
    for space in ('\x1c', '\x1d', '\x1e', '\x1f'):
        inputs.extend([
            (f'json-padding-{ord(space):04x}', space + document + space),
            (f'netscape-padding-{ord(space):04x}', space + row + space),
        ])
    inputs.extend([
        ('unit-separator-fields', row.replace('\t', '\x1f')),
        ('repeated-unit-separator-fields', row.replace('\t', '\x1f \x1f')),
        ('tab-value-preserves-unit-separator', row + '\x1finside'),
        ('json-value-preserves-unit-separator', json.dumps(dict(cookie, value='\x1f'))),
    ])
    for name, control in [('escape', '\x1b'), ('zero-width-space', chr(0x200b))]:
        inputs.extend([
            (f'json-nonspace-{name}', control + document + control),
            (f'netscape-nonspace-{name}', control + row + control),
        ])
    return [{'name': name, 'text': text, 'parsed': parse(text),
             'netscape': parse_netscape(text)} for name, text in inputs]
