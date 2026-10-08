"""Original pasted proxy forms, IPv6, credential separators and round trips."""
from __future__ import annotations

from numera import proxypool


def decimal_ranges() -> list[list[int]]:
    ranges = []
    for scalar in range(0x110000):
        if chr(scalar).isdecimal():
            if ranges and scalar == ranges[-1][1] + 1:
                ranges[-1][1] = scalar
            else:
                ranges.append([scalar, scalar])
    return ranges


def digit_ranges() -> list[list[int]]:
    ranges = []
    start = end = None
    for scalar in range(0x110000):
        if chr(scalar).isdigit():
            if start is None:
                start = end = scalar
            elif scalar == end + 1:
                end = scalar
            else:
                ranges.append([start, end])
                start = end = scalar
    if start is not None:
        ranges.append([start, end])
    return ranges


def cases() -> list[dict]:
    lines = ['', ' ', '# fixture comment', '127.0.0.1:8080', 'host.test:80:US',
             'host.test:8080:fixture-user:fixture-pass:IT 1',
             'fixture-user:fixture:pass@host.test:8080:GB',
             'socks5://fixture-user:fixture-pass@[2001:db8::1]:1080:US_east',
             '[2001:db8::1]:1080', 'socks5h://[::1]:1080',
             'socks://host.test:1080', 'SOCKS4://host.test:1080',
             'custom://host.test:1080', 'host.test:nope', ':8080',
             '[2001:db8::1:1080', 'host.test:0', 'host.test:99999',
             'host.test: 8080', 'host.test:٨٠', 'host.test:²',
             '@host.test:8080', 'fixture-user@host.test:8080',
             'host.test:8080:::CA', 'https://host.test:443:US-2']
    return [{'line': line, 'parsed': (record := proxypool.parse_line(line)),
             'rendered': proxypool.to_line(record) if record else None,
             'reparsed': proxypool.parse_line(proxypool.to_line(record)) if record else None}
            for line in lines]
