"""Observe original cookie write policy with only the SDK session replaced."""
from contextlib import contextmanager
from copy import deepcopy
from unittest.mock import patch

from numera import cookies, drivers


def cases():
    result = []
    row = {'name': 'fixture', 'value': 'synthetic', 'domain': '.fixture.test',
           'path': '/', 'expires': -1.0, 'httpOnly': False,
           'secure': False, 'sameSite': 'Lax'}
    for jar, clear, failed in [([], False, None), ([], True, None),
                               ([row], False, None), ([row], True, None),
                               ([row], True, 'clear'), ([row], True, 'add')]:
        seen = []
        class Context:
            def clear_cookies(self):
                seen.append({'operation': 'clear'})
                if failed == 'clear':
                    raise RuntimeError('Controlled cookie clear failure')
            def add_cookies(self, values):
                seen.append({'operation': 'add', 'cookies': deepcopy(values)})
                if failed == 'add':
                    raise RuntimeError('Controlled cookie add failure')
        @contextmanager
        def session(*args, **kwargs):
            seen.append({'operation': 'open'})
            try:
                yield Context()
            finally:
                seen.append({'operation': 'close'})
        with patch.object(drivers, 'session', session):
            try:
                count = cookies.write(None, None, jar, clear=clear)
                outcome = {'count': count}
            except RuntimeError:
                outcome = {'failed': True}
        result.append({'name': f'cookie-write-{len(result)}', 'cookies': deepcopy(jar),
                       'clear': clear, 'failure': failed, 'expected': seen,
                       'outcome': outcome})
    return result
