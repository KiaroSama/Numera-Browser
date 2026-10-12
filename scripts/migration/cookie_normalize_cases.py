"""Independent original cookie normalization and input-format observations."""
from __future__ import annotations

import json
from numera import cookies


def cases():
    base = {'name': 'fixture', 'value': 'synthetic', 'domain': '.fixture.test'}
    rows = [None, 'bad', [], {}, {'name': 'missing-domain'},
            dict(base, host='fallback.test', domain=''),
            dict(base, name=1, value=None, path=''),
            dict(base, name=True, value=['α', True, None]),
            dict(base, value={'a': 'synthetic', 'b': [1, False]}),
            dict(base, secure=True, httpOnly='yes', sameSite='no_restriction'),
            dict(base, secure=False, sameSite='None'),
            dict(base, sameSite='STRICT'), dict(base, sameSite='unknown')]
    for expiry in (None, '', 0, False, True, -0.0, -1, 1.25, '١٢٣.٥',
                   '1_234.5', '1__2', 'bad', [], {}, '2027-01-01T00:00:00Z',
                   '2027-01-01T03:00:00+03:00', '2027-01-01',
                   '20270101T000000', '2027-W01-1T00:00:00+00:00',
                   '2027-01-01🐈00:00:00.1234567+00:00',
                   '2027-01-01T00:00:00+00:00:30.5', '2027-02-30T00:00:00Z',
                   '2027-01-01T00:00:00+01:60', '2027-01-01T00:00:00-01:60',
                   '2027-01-01T00:00:00+00:00:99', '2027-01-01T00:00:00+00:00:00.5',
                   '2027-01-01T00:00:00-00:00:00.5', '2027-01-01T00:00:00+24:00',
                   '2027-01-01T00:00:00-24:00', '2027-01-01T24:00:00+00:00'):
        rows.append(dict(base, expires=expiry))
    rows.extend([dict(base, expirationDate='2027-01-01T00:00:00Z'),
                 dict(base, expires=None, expirationDate=123),
                 dict(base, expires='bad', session=True)])
    normalized = [{'input': row, 'expected': cookies.normalize(row)} for row in rows]
    texts = ['', '   ', '{}', 'null', 'true', '12', '"plain"',
             json.dumps(base), json.dumps([base, None, {'name': 'missing'}]),
             json.dumps({'cookies': [base], 'origins': []}),
             json.dumps({'cookies': None}), json.dumps({'cookies': {}}),
             json.dumps({'cookies': 'bad'}), 'not JSON']
    return {'normalize': normalized,
            'parse': [{'text': text, 'expected': cookies.parse(text)} for text in texts]}
