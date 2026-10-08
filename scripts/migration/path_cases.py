"""Filename contract expectations executed by the retained source."""
from numera.paths import safe_id


def cases() -> list[dict]:
    values = ['', 'alpha', 'a' * 64, 'a' * 65, 'CON', 'con', 'COM0', 'LPT9',
              'com10', 'LPT10', 'NUL', 'aux', 'PRN', 'a_b-9', '..', '../outside',
              'a/b', 'a\\b', 'C:alpha', 'space name', 'Unicode α', 'فارسی', '\n',
              'alpha\n', 'alpha.json']
    found = []
    for value in values:
        try:
            expected, error = safe_id(value), None
        except ValueError as exc:
            expected, error = None, str(exc)
        found.append({'input': value, 'expected': expected, 'error': error})
    return found
