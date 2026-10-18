"""Sequential original/native CLI replay over one synthetic store; never select a runtime."""
from __future__ import annotations

import json
import math
import sys


def exact_json(value):
    return json.dumps(value, sort_keys=True, allow_nan=False)


def require_profile(observed, expected):
    timestamp = observed.get('updated_at')
    if (isinstance(timestamp, bool) or not isinstance(timestamp, (int, float))
            or not math.isfinite(timestamp) or timestamp < expected['created_at']):
        raise AssertionError('Cross-runtime profile timestamp is invalid')
    if (exact_json({key: value for key, value in observed.items() if key != 'updated_at'}) !=
            exact_json({key: value for key, value in expected.items() if key != 'updated_at'})):
        raise AssertionError('Cross-runtime profile changed identity, fields or secret')


def protected_bytes(root):
    found = {}
    for name in ('config.json', 'dashboard-config.json', 'proxies.json', 'profiles',
                 'dashboard', 'sessions', 'user-data'):
        path = root / name
        paths = [path]
        for item in paths:
            if item.is_symlink() or (item.exists()
                    and getattr(item.lstat(), 'st_file_attributes', 0) & 0x400):
                raise ValueError('Replay fixture contains a linked protected path')
            if item.is_dir():
                paths.extend(item.iterdir())
            elif item.is_file():
                found[item.relative_to(root).as_posix()] = item.read_bytes()
    return found


def run(candidate, original, parent, environment, execute, logger):
    root = parent / 'cross-runtime data with spaces'
    root.mkdir()
    env = dict(environment, PYTHONPATH=str(original / 'engine'))
    env.pop('NUMERA_PASSWORD', None)
    executables = {'original': [sys.executable, '-m', 'numera'], 'native': [str(candidate)]}

    def command(runtime, *arguments, password=None, expected_exit=0):
        child_env = dict(env)
        if password is not None:
            child_env['NUMERA_PASSWORD'] = password
        code, stdout, stderr = execute(
            [*executables[runtime], '--data-dir', str(root), *arguments, '--json'], child_env)
        if code != expected_exit or stderr or len(stdout.encode('utf-8')) > 131072:
            raise AssertionError(f'Cross-runtime command refused runtime={runtime} operation={arguments[0]} exit={code}')
        payload = json.loads(stdout)
        logger.info('Cross-runtime command runtime=%s operation=%s exit=%s', runtime, arguments[0], code)
        return payload

    secret = 'replay-fixture-secret'
    first_password, second_password = 'replay-fixture-master-one', 'replay-fixture-master-two'
    created = command('original', 'create', 'Rollback fixture α', '--seed', '42',
                      '--engine', 'playwright', '--notes', 'source-created',
                      '--proxy', f'http://fixture:{secret}@127.0.0.1:8080')['profile']
    identifier = created['id']
    assert created['proxy']['password'] == secret
    require_profile(command('native', 'show', identifier), created)
    expected = {**created, 'notes': 'native-edited', 'tags': ['native']}
    require_profile(command('native', 'edit', identifier, '--notes', expected['notes'],
                            '--add-tag', 'native')['profile'], expected)
    require_profile(command('original', 'show', identifier), expected)

    config_path = root / 'config.json'
    config = json.loads(config_path.read_text(encoding='utf-8'))
    sentinel = {'text': 'retained α', 'items': [42, True]}
    config['rollbackSentinel'] = sentinel
    config_path.write_text(json.dumps(config), encoding='utf-8')
    dashboard_path = root / 'dashboard' / (identifier + '.json')
    dashboard = json.loads(dashboard_path.read_text(encoding='utf-8'))
    dashboard['futureRollback'] = sentinel
    dashboard_path.write_text(json.dumps(dashboard), encoding='utf-8')
    session = root / 'user-data' / identifier
    marker = session / 'Default' / 'replay-fixture.bin'
    marker.parent.mkdir(parents=True)
    session_bytes = b'\x00\xffrepresentative session fixture\r\n'
    marker.write_bytes(session_bytes)

    enabled = command('native', 'vault', 'enable', password=first_password)
    assert enabled == {'ok': True, 'enabled': True, 'encrypted': 2}
    require_profile(command('original', 'show', identifier, password=first_password), expected)
    expected = {**expected, 'notes': 'source-after-native-vault', 'tags': ['native', 'original']}
    require_profile(command('original', 'edit', identifier, '--notes', expected['notes'],
                            '--add-tag', 'original', password=first_password)['profile'], expected)
    require_profile(command('native', 'show', identifier, password=first_password), expected)

    before = protected_bytes(root)
    errors = []
    for runtime in executables:
        destination = parent / f'{runtime}-locked-export.json'
        failure = command(runtime, 'export', identifier, str(destination), expected_exit=1)
        assert failure['ok'] is False and 'vault is locked' in failure['error']
        assert not destination.exists(), 'Locked export created an incomplete archive'
        assert protected_bytes(root) == before, 'Locked refusal changed protected fixture bytes'
        errors.append(failure)
    assert errors[0] == errors[1], 'Locked export verdict differs across runtimes'

    for writer, reader in (('native', 'original'), ('original', 'native')):
        archive = parent / f'{writer}-backup.zip'
        backed = command(writer, 'backup', identifier, str(archive), password=first_password)
        assert backed == {'ok': True, 'id': identifier, 'name': expected['name'], 'file': str(archive)}
        marker.write_bytes(b'changed after backup')
        stale = session / 'stale.bin'
        stale.write_bytes(b'must not merge')
        restored = command(reader, 'restore', str(archive), password=first_password)
        assert restored['ok'] is True
        require_profile(restored['profile'], expected)
        require_profile(command(writer, 'show', identifier, password=first_password), expected)
        assert marker.read_bytes() == session_bytes and not stale.exists()

    rotated = command('original', 'vault', 'change', '--new-password', second_password,
                      password=first_password)
    assert rotated == {'ok': True, 'rotated': 2}
    require_profile(command('native', 'show', identifier, password=second_password), expected)
    expected = {**expected, 'notes': 'native-after-source-rotation'}
    require_profile(command('native', 'edit', identifier, '--notes', expected['notes'],
                            password=second_password)['profile'], expected)
    require_profile(command('original', 'show', identifier, password=second_password), expected)
    for runtime in executables:
        status = command(runtime, 'vault', 'status', password=first_password)
        assert status == {'enabled': True, 'unlocked': False, 'backend': True}
    assert exact_json(json.loads(config_path.read_text(encoding='utf-8'))['rollbackSentinel']) == exact_json(sentinel)
    assert exact_json(json.loads(dashboard_path.read_text(encoding='utf-8'))['futureRollback']) == exact_json(sentinel)
    for relative, field in ((f'profiles/{identifier}.json', 'password'),
                            (f'dashboard/{identifier}.json', 'pass')):
        raw = (root / relative).read_bytes()
        value = json.loads(raw)['proxy'][field]
        assert value.startswith('enc:v2:') and secret.encode('utf-8') not in raw
    assert marker.read_bytes() == session_bytes
    assert not (root / 'vault-migration.json').exists()
    assert not list(root.glob('numera-restore-*'))
    assert not list(session.parent.glob(identifier + '.replaced-*'))
    assert not list((root / 'sessions').glob('*.json'))
    logger.info('Cross-runtime original/native replay, vault rotation, archive interoperability and source rollback passed')
