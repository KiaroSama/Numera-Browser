"""Observe original Netscape parsing with synthetic, non-sensitive text."""
from __future__ import annotations

from baseline_source import activate
activate()

from numera.cookies import parse_netscape


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
