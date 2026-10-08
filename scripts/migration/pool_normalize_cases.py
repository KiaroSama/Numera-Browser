"""Original structured proxy validation, with no requests or DNS lookups."""
from __future__ import annotations

from numera import proxypool


def cases() -> list[dict]:
    base = {'host': 'host.test', 'port': '8080'}
    inputs = [None, [], {}, base, {**base, 'type':'socks5', 'country':' gb ', 'pass':'fixture-secret'},
              *({**base, 'port':value} for value in (None, '', True, 0, -1, 65535, 65536, ' 80 ', '+80', '08', 80.0, '٨٠', '²')),
              *({**base, 'pass':value} for value in (None, '', False, 42, [])),
              *({**base, 'host':value} for value in ('[2001:db8::1]', ' ', 'host test', 42, False)),
              *({**base, 'type':value} for value in ('SOCKS4', 'socks5h', ' ftp ', None)),
              *({**base, 'country':value} for value in ('US', 'usa', 42, 'éé')),
              *({**base, 'rotateUrl':value} for value in (None, '', ' https://host.test/change ', 'ftp://host.test/', 'https:///change', 'http://host.test:99999/', 'http://host.test:nope/', 'http://[::1]:8080/change'))]
    found = []
    for entry in inputs:
        try:
            value, error = proxypool.normalize(entry), None
        except ValueError as exc:
            value, error = None, str(exc)
        found.append({'input':entry, 'result':value, 'error':error})
    return found
