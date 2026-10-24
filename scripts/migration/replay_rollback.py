"""Sequential original/native CLI replay over one synthetic store; never select a runtime."""
from __future__ import annotations

import json
import math
import sys


def exact_json(value):
    return json.dumps(value, sort_keys=True, allow_nan=False)


def require_equal(observed, expected, label):
    if exact_json(observed) != exact_json(expected):
        raise AssertionError(f'Cross-runtime {label} differs')


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

    def paired(*arguments, password=None):
        before = protected_bytes(root)
        responses = [command(runtime, *arguments, password=password) for runtime in executables]
        require_equal(responses[0], responses[1], arguments[0] + ' read')
        assert protected_bytes(root) == before, 'Cross-runtime read changed protected bytes'
        return responses[0]

    def refused(*arguments, password=None):
        before = protected_bytes(root)
        responses = [command(runtime, *arguments, password=password, expected_exit=1)
                     for runtime in executables]
        require_equal(responses[0], responses[1], arguments[0] + ' refusal')
        assert protected_bytes(root) == before, 'Cross-runtime refusal changed protected bytes'
        return responses[0]

    settings = paired('config')
    extension = parent / 'unpacked extension with spaces'
    extension.mkdir()
    manifest = extension / 'manifest.json'
    manifest_bytes = b'{"manifest_version":3,"name":"Replay fixture","version":"1.0"}\n'
    manifest.write_bytes(manifest_bytes)
    settings = {**settings, 'default_engine': 'playwright', 'headless_launch': True,
                'default_extensions': [str(extension)]}
    require_equal(command('native', 'config', '--default-engine', 'playwright',
                          '--headless-launch', 'on', '--default-extension', str(extension)),
                  settings, 'native settings write')
    require_equal(paired('config'), settings, 'settings reopen')
    refused('config', '--headless-launch', 'off', '--default-extension', str(parent / 'absent extension'))
    settings = {**settings, 'headless_launch': False}
    require_equal(command('original', 'config', '--headless-launch', 'off'), settings, 'source settings write')
    require_equal(paired('config'), settings, 'source settings reopen')
    assert manifest.read_bytes() == manifest_bytes

    folder = dashboard['folder']
    renamed = 'Replay renamed α'
    final_folder = 'Replay final 日本語'
    dashboard['folder'] = 'Replay source α'
    dashboard_path.write_text(json.dumps(dashboard), encoding='utf-8')
    metadata_path = root / 'dashboard-config.json'
    metadata = json.loads(metadata_path.read_text(encoding='utf-8')) if metadata_path.exists() else {}
    metadata['rollbackSentinel'] = sentinel
    metadata_path.write_text(json.dumps(metadata), encoding='utf-8')
    initial_folders = paired('folder', 'list')['folders']
    names = list(dict.fromkeys([*initial_folders, 'Replay source α']))
    require_equal(command('original', 'folder', 'create', 'Replay source α'),
                  {'folders': names, 'created': 'Replay source α'}, 'source folder create')
    for runtime, source, destination in (('native', 'Replay source α', renamed),
                                          ('original', renamed, final_folder)):
        names = list(dict.fromkeys(destination if value == source else value for value in names))
        require_equal(command(runtime, 'folder', 'rename', source, destination),
                      {'folders': names, 'from': source, 'to': destination}, 'folder rename')
        require_equal(paired('folder', 'list'), {'folders': names}, 'folder rename reopen')
        assert json.loads(dashboard_path.read_text(encoding='utf-8'))['folder'] == destination
    names = list(dict.fromkeys([*names, 'Replay source α']))
    require_equal(command('native', 'folder', 'create', 'Replay source α'),
                  {'folders': names, 'created': 'Replay source α'}, 'retired folder recreation')
    assert 'Replay source α' not in json.loads(metadata_path.read_text(encoding='utf-8')).get('renamed', {})
    refused('folder', 'rename', '', 'invalid')
    names = [value for value in names if value != final_folder]
    if folder not in names:
        names.append(folder)
    require_equal(command('native', 'folder', 'delete', final_folder, '--reassign-to', folder),
                  {'folders': names, 'deleted': final_folder, 'reassignedTo': folder}, 'folder reassignment')
    require_equal(paired('folder', 'list'), {'folders': names}, 'folder deletion reopen')
    assert json.loads(dashboard_path.read_text(encoding='utf-8'))['folder'] == folder

    pool_secret = 'replay-pool-secret'
    pool_file = parent / 'pool input with spaces.txt'
    pool_file.write_text(f'http://pool-user:{pool_secret}@127.0.0.1:8181:US\n', encoding='utf-8')
    added = command('original', 'pool', 'add', str(pool_file))
    assert (added['added'], added['parsed'], added['total']) == (1, 1, 1)
    pool_row = added['proxies'][0]
    imported_pool_row = dict(pool_row)
    pool_id = pool_row['id']
    assert isinstance(pool_id, str) and len(pool_id) == 12 and pool_row['pass'] == pool_secret
    pool_path = root / 'proxies.json'
    raw_pool = json.loads(pool_path.read_text(encoding='utf-8'))
    raw_pool[0]['futureRollback'] = sentinel
    pool_path.write_text(json.dumps(raw_pool), encoding='utf-8')
    pool_row = {**pool_row, 'futureRollback': sentinel}
    require_equal(paired('pool', 'list'), {'total': 1, 'proxies': [pool_row]}, 'pool source import')
    require_equal(command('native', 'pool', 'add', str(pool_file)),
                  {'added': 0, 'parsed': 1, 'total': 1, 'proxies': []}, 'pool cross-runtime deduplication')
    require_equal(paired('pool', 'list'), {'total': 1, 'proxies': [pool_row]}, 'pool deduplication reopen')
    pool_row = {**pool_row, 'label': 'native pool α'}
    require_equal(command('native', 'pool', 'edit', pool_id, '--label', pool_row['label']),
                  {'ok': True, 'proxy': pool_row}, 'native pool edit')
    require_equal(paired('pool', 'list'), {'total': 1, 'proxies': [pool_row]}, 'pool edit reopen')
    refused('pool', 'edit', pool_id, '--port', '70000')
    for runtime, restore in (('original', False), ('native', True)):
        require_equal(command(runtime, 'pool', 'archive', pool_id, *(['--restore'] if restore else [])),
                      {'restored' if restore else 'archived': 1, 'ids': [pool_id]}, 'pool archive transition')
        pool_row = {**pool_row, 'archived': not restore}
        require_equal(paired('pool', 'list'), {'total': 1, 'proxies': [pool_row]}, 'pool archive reopen')

    enabled = command('native', 'vault', 'enable', password=first_password)
    assert enabled == {'ok': True, 'enabled': True, 'encrypted': 3}
    require_equal(paired('pool', 'list', password=first_password),
                  {'total': 1, 'proxies': [pool_row]}, 'pool migration reopen')
    sealed_pool = json.loads(pool_path.read_text(encoding='utf-8'))[0]['pass']
    assert sealed_pool.startswith('enc:v2:') and pool_secret.encode('utf-8') not in pool_path.read_bytes()
    refused('pool', 'export', pool_id)
    refused('pool', 'assign', identifier, '--proxy-id', pool_id)
    # Metadata-only changes while locked must retain the exact unreadable secret.
    pool_row = {**pool_row, 'label': 'source locked metadata α'}
    locked_edit = command('original', 'pool', 'edit', pool_id, '--label', pool_row['label'])
    require_equal(locked_edit, {'ok': True, 'proxy': {**pool_row, 'pass': None}}, 'locked pool metadata edit')
    assert json.loads(pool_path.read_text(encoding='utf-8'))[0]['pass'] == sealed_pool
    require_equal(paired('pool', 'list', password=first_password),
                  {'total': 1, 'proxies': [pool_row]}, 'locked pool preservation')
    pool_row = {**pool_row, 'label': 'native unlocked metadata 日本語'}
    require_equal(command('native', 'pool', 'edit', pool_id, '--label', pool_row['label'], password=first_password),
                  {'ok': True, 'proxy': pool_row}, 'unlocked pool edit')
    require_equal(paired('pool', 'export', pool_id, password=first_password),
                  {'exported': 1, 'file': None,
                   'text': f'http://pool-user:{pool_secret}@127.0.0.1:8181:{pool_row["label"]}\n'},
                  'pool export')
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
    assert rotated == {'ok': True, 'rotated': 3}
    require_equal(paired('config', password=second_password), settings, 'settings after rotation')
    require_equal(paired('folder', 'list', password=second_password),
                  {'folders': names}, 'folders after rotation')
    require_equal(paired('pool', 'list', password=second_password),
                  {'total': 1, 'proxies': [pool_row]}, 'pool after rotation')
    assert json.loads(pool_path.read_text(encoding='utf-8'))[0]['pass'].startswith('enc:v2:')
    assert pool_secret.encode('utf-8') not in pool_path.read_bytes()
    refused('pool', 'export', pool_id, password=first_password)
    # Both writers must preserve a sealed credential through an explicit clear/replacement.
    pool_row = {**pool_row, 'pass': '', 'status': 'untested', 'lastCheck': None, 'checkedAt': None}
    require_equal(command('original', 'pool', 'edit', pool_id, '--pass', '', password=second_password),
                  {'ok': True, 'proxy': pool_row}, 'pool credential clear')
    require_equal(paired('pool', 'list', password=second_password),
                  {'total': 1, 'proxies': [pool_row]}, 'pool clear reopen')
    # The frozen CLI shares --pass and the master-password destination. A nonempty
    # entry password shadows the environment key; preserve its refusal, not fake success.
    refused('pool', 'edit', pool_id, '--pass', pool_secret, password=second_password)
    require_equal(command('original', 'pool', 'remove', pool_id, password=second_password),
                  {'removed': 1, 'ids': [pool_id], 'missing': [], 'total': 0}, 'cleared pool removal')
    replacement = command('native', 'pool', 'add', str(pool_file), password=second_password)
    assert (replacement['added'], replacement['parsed'], replacement['total']) == (1, 1, 1)
    pool_row = replacement['proxies'][0]
    assert isinstance(pool_row['id'], str) and len(pool_row['id']) == 12 and pool_row['id'] != pool_id
    pool_id = pool_row['id']
    require_equal(pool_row, {**imported_pool_row, 'id': pool_id}, 'encrypted native pool import')
    require_equal(paired('pool', 'list', password=second_password),
                  {'total': 1, 'proxies': [pool_row]}, 'pool replacement reopen')
    assert json.loads(pool_path.read_text(encoding='utf-8'))[0]['pass'].startswith('enc:v2:')
    assert pool_secret.encode('utf-8') not in pool_path.read_bytes()
    require_profile(command('native', 'show', identifier, password=second_password), expected)
    expected = {**expected, 'notes': 'native-after-source-rotation'}
    require_profile(command('native', 'edit', identifier, '--notes', expected['notes'],
                            password=second_password)['profile'], expected)
    require_profile(command('original', 'show', identifier, password=second_password), expected)
    for runtime in executables:
        status = command(runtime, 'vault', 'status', password=first_password)
        assert status == {'enabled': True, 'unlocked': False, 'backend': True}
    pool_row = {**pool_row, 'label': 'source rollback pool α'}
    require_equal(command('original', 'pool', 'edit', pool_id, '--label', pool_row['label'], password=second_password),
                  {'ok': True, 'proxy': pool_row}, 'source rollback pool write')
    require_equal(paired('pool', 'list', password=second_password),
                  {'total': 1, 'proxies': [pool_row]}, 'source rollback pool reopen')
    assert json.loads(pool_path.read_text(encoding='utf-8'))[0]['pass'].startswith('enc:v2:')
    assert pool_secret.encode('utf-8') not in pool_path.read_bytes()
    require_equal(command('native', 'pool', 'remove', pool_id, 'missing', password=second_password),
                  {'removed': 1, 'ids': [pool_id], 'missing': ['missing'], 'total': 0}, 'pool removal')
    require_equal(paired('pool', 'list', password=second_password),
                  {'total': 0, 'proxies': []}, 'pool removal reopen')
    refused('pool', 'remove', 'missing', password=second_password)
    assert exact_json(json.loads(metadata_path.read_text(encoding='utf-8'))['rollbackSentinel']) == exact_json(sentinel)
    assert manifest.read_bytes() == manifest_bytes
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
    logger.info('Shared settings, folder chains and encrypted proxy-pool generations passed')
