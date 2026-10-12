"""Observe original proxy archive/assignment routes with isolated real stores."""
from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app
from numera.store import ProfileStore
from numera.models import Profile, Proxy
from numera.fingerprint import generate


def address_cases():
    import ipaddress
    prefixes = ['0.0.0.0/8', '10.0.0.0/8', '100.64.0.0/10', '127.0.0.0/8',
                '169.254.0.0/16', '172.16.0.0/12', '192.0.0.0/24', '192.0.2.0/24',
                '192.168.0.0/16', '198.18.0.0/15', '198.51.100.0/24',
                '203.0.113.0/24', '240.0.0.0/4', '64:ff9b:1::/48', '100::/64',
                '2001::/23', '2001:db8::/32', '2002::/16', '3fff::/20', 'fc00::/7',
                'fe80::/10', '2001:3::/32', '2001:4:112::/48', '2001:20::/28',
                '2001:30::/28']
    addresses = {'8.8.8.8', '192.0.0.9', '192.0.0.10', '224.0.0.1', '::', '::1',
                 '2001:1::1', '2001:1::2', '2001:1::3', 'ff02::1'}
    for text in prefixes:
        network = ipaddress.ip_network(text)
        first, last = int(network.network_address), int(network.broadcast_address)
        for number in (first - 1, first, last, last + 1):
            if 0 <= number < 2 ** network.max_prefixlen:
                addresses.add(str(network.network_address.__class__(number)))
    for text in tuple(addresses):
        if ':' not in text:
            addresses.add('::ffff:' + text)
    return [{'ip': text, 'global': ipaddress.ip_address(text).is_global}
            for text in sorted(addresses)]


def relay_cases():
    from numera.relay import _socks4_connect, _socks5_connect
    class Socket:
        def __init__(self, reply):
            self.reply = reply
            self.sent = bytearray()
        def sendall(self, data): self.sent.extend(data)
        def recv(self, count):
            part, self.reply = self.reply[:count], self.reply[count:]
            return part
    rows = []
    for kind, denied in [('socks4', False), ('socks5', False), ('socks4', True), ('socks5', True)]:
        reply = ((b'\x00' + bytes([91 if denied else 90]) + b'\x00' * 6)
                 if kind == 'socks4' else
                 b'\x05\x02\x01\x00\x05' + bytes([5 if denied else 0]) + b'\x00\x01' + b'\x00' * 6)
        sock = Socket(reply)
        error = None
        try:
            if kind == 'socks4': _socks4_connect(sock, 'destination.invalid', 8080, 'tenant-7')
            else: _socks5_connect(sock, 'destination.invalid', 8080, 'tenant-7', 'synthetic')
        except Exception as failure:
            error = str(failure)
        if denied != (error is not None):
            raise AssertionError('Source SOCKS refusal fixture did not exercise its verdict')
        rows.append({'kind': kind, 'denied': denied, 'error': error, 'wire': list(sock.sent)})
    return rows


def snapshot(root):
    return {kind: {path.name: json.loads(path.read_text(encoding='utf-8'))
                   for path in sorted((root / kind).glob('*.json'))}
            for kind in ('profiles', 'dashboard')}


def cases(parent):
    parent = Path(parent)
    parent.mkdir(parents=True, exist_ok=True)
    rows = [dict(id='one', type='HTTP', host='first.test', port='8080', user='user',
                 **{'pass': 'synthetic'}, country='US', archived=False),
            dict(id='two', type='SOCKS5', host='second.test', port='1080', user=None,
                 **{'pass': None}, country='DE', archived=True)]
    locked = [dict(rows[0], **{'pass': 'enc:v2:synthetic-unreadable-fixture'})]
    variants = [
        ('archive-empty', 'archive', {}, rows, True),
        ('archive-shape', 'archive', {'ids': 'one'}, rows, True),
        ('archive-selected', 'archive', {'ids': ['one', 'missing']}, rows, True),
        ('archive-mixed-ids', 'archive', {'ids': ['one', 123, None, {}]}, rows, True),
        ('assign-mixed-proxy-ids', 'assign', {'proxyIds': [123, None, {}, 'one']}, rows, True),
        ('archive-missing', 'archive', {'ids': ['missing']}, rows, True),
        ('restore', 'archive', {'ids': ['two'], 'archived': False}, rows, True),
        ('archive-unauthorized', 'archive', {'ids': ['one']}, rows, False),
        ('assign-locked', 'assign', {}, locked, True),
        ('test-locked-only', 'test', {}, locked, True),
        ('test-single-locked', 'one/test', {}, locked, True),
        ('test-single-missing', 'missing/test', {}, rows, True),
        ('rotate-single-missing', 'missing/rotate', {}, rows, True),
        ('rotate-single-locked', 'one/rotate', {}, locked, True),
        ('rotate-single-no-url', 'one/rotate', {}, rows, True),
        ('rotate-many-empty', 'rotate', {}, rows, True),
        ('rotate-many-locked', 'rotate', {'ids': ['one']}, locked, True),
        ('rotate-many-no-url', 'rotate', {'ids': ['two', 'one']}, rows, True),
        ('test-empty', 'test', {}, [], True),
        ('test-missing', 'test', {'ids': ['missing']}, rows, True),
        ('assign-empty', 'assign', {}, [], True),
        ('assign-archived', 'assign', {}, [dict(row, archived=True) for row in rows], True),
        ('assign-whole', 'assign', {}, rows, True),
        ('assign-explicit', 'assign', {'proxyIds': ['two', 'one']}, rows, True),
        ('assign-missing-proxy', 'assign', {'proxyIds': ['missing']}, rows, True),
        ('assign-missing-profile', 'assign', {'profileIds': ['missing']}, rows, True),
        ('assign-duplicate-profile', 'assign', {'proxyIds': ['one', 'two'], 'profileIds': ['a', 'a']}, rows, True),
        ('assign-malformed-second', 'assign', {}, rows, True),
    ]
    results = []
    for name, verb, body, proxies, authorized in variants:
        with tempfile.TemporaryDirectory(prefix='proxy-source-', dir=parent) as directory:
            root = Path(directory)
            with patch.dict(os.environ, {'NUMERA_API_TOKEN': 'fixture-api', 'NUMERA_ALLOWED_ORIGINS': '', 'NUMERA_PASSWORD': ''}), patch('time.time', return_value=1000.0):
                app = create_app(data_dir=directory, start_scheduler=False)
                store = ProfileStore(root)
                for id in ('a', 'b'):
                    store.save(Profile(id=id, name=f'Fixture {id}', fingerprint=generate(seed=42),
                                       proxy=Proxy(server='http://previous.test:9000', username='old-user', password='synthetic-old'),
                                       created_at=1000.0, updated_at=1000.0))
                pool_path = root / 'proxies.json'
                pool_path.write_text(json.dumps(proxies), encoding='utf-8')
                if name == 'assign-malformed-second':
                    path = root / 'dashboard' / 'b.json'
                    record = json.loads(path.read_text(encoding='utf-8'))
                    record['cores'] = 1.5
                    path.write_text(json.dumps(record), encoding='utf-8')
                before = snapshot(root)
                with TestClient(app, raise_server_exceptions=False) as client:
                    headers = {'authorization': 'Bearer fixture-api'} if authorized else {}
                    response = client.post(f'/api/proxies/{verb}', json=body, headers=headers)
                payload = response.json() if response.headers.get('content-type', '').startswith('application/json') else response.text
                results.append({'name': name, 'verb': verb, 'input': body, 'authorized': authorized,
                                'rows': proxies, 'before': before, 'status': response.status_code,
                                'body': payload, 'after': snapshot(root),
                                'pool_after': json.loads(pool_path.read_text(encoding='utf-8'))})
    return results
