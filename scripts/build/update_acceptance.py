"""Exercise two actual fixture-compiled portable generations through the updater."""
from __future__ import annotations

import argparse
from collections import deque
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import threading
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import ProxyHandler, Request, build_opener
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from numera import proctree, unit_report  # noqa: E402
from run_logging import run_log  # noqa: E402

PRODUCT = 'Numera Browser'
VAULT_PASSWORD = 'Public updater acceptance vault fixture only'
PROXY_PASSWORD = 'public-acceptance-proxy-value'
SEED = bytes.fromhex('5d' * 32)  # Public synthetic fixture identity; never an owner signer.
OPENER = build_opener(ProxyHandler({}))


def key():
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    return Ed25519PrivateKey.from_private_bytes(SEED)


def prepare(directory: Path):
    from cryptography.hazmat.primitives import serialization
    directory.mkdir(parents=True, exist_ok=False)
    public = key().public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex()
    (directory / 'public-key.hex').write_text(public + '\n', encoding='utf-8')
    # No private production values. The test key is explicitly public and may be
    # reconstructed by CI; do not export repository secrets into this process.
    (directory / 'fixture-signing.pem').write_bytes(key().private_bytes(
        serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    run = int(os.environ.get('GITHUB_RUN_ID', '1'))
    value = {'schema': 1, 'acceptance': True, 'old_generation': run * 2,
             'new_generation': run * 2 + 1, 'public_key': public}
    (directory / 'fixture.json').write_text(json.dumps(value) + '\n', encoding='utf-8')
    return value


def digest(path: Path):
    checksum = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            checksum.update(block)
    return checksum.hexdigest()


def command(argv, env, seconds=30, data=None):
    process, tree = proctree.spawn([str(item) for item in argv], env=env,
                                  stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
    try:
        output, _ = process.communicate(input=data, timeout=seconds)
        if process.returncode:
            raise RuntimeError(f'Fixture command failed exit={process.returncode} tool={Path(argv[0]).name}')
        return output
    except subprocess.TimeoutExpired:
        raise RuntimeError(f'Fixture command timed out tool={Path(argv[0]).name}') from None
    finally:
        proctree.end_session(process, tree, grace=1, kill_grace=3)


def identity(stage: Path, env):
    shell = stage / ('numera.exe' if os.name == 'nt' else 'numera')
    engine = stage / 'engine' / ('numera-engine.exe' if os.name == 'nt' else 'numera-engine')
    shell_id = json.loads(command([shell, '--build-info'], env))
    engine_id = json.loads(command([engine, '--build-info'], env))
    assert shell_id.get('acceptance') is True, 'Acceptance requires a fixture-compiled shell'
    assert engine_id.get('acceptance') is True, 'Acceptance requires a fixture-frozen engine'
    for field in ('source_sha', 'generation', 'version', 'owner', 'product'):
        assert shell_id[field] == engine_id[field], f'Compiled shell/engine mismatch: {field}'
    assert shell_id['owner'] == 'KiaroSama' and shell_id['product'] == PRODUCT
    assert len(shell_id['source_sha']) == 40 and shell_id['generation'] > 0
    return shell_id


def clone(source: Path, destination: Path):
    # Copy, not a writable hard link: fault/rollback must never mutate another
    # fixture generation or the parent-owned release staging directory.
    shutil.copytree(source, destination, symlinks=True,
                    ignore=shutil.ignore_patterns('BUILD.json', 'BUILD.sig', 'logs', 'data', '.updates', '.numera-update-*'))


def sign_build(stage: Path, info, platform):
    from distribution import dependencies
    files = []
    for path in sorted(stage.rglob('*')):
        name = path.relative_to(stage).as_posix()
        if path.is_symlink():
            target = os.readlink(path)
            assert not Path(target).is_absolute() and path.resolve().is_relative_to(stage.resolve())
            files.append({'path': name, 'link': target})
        elif path.is_file():
            files.append({'path': name, 'size': path.stat().st_size,
                          'sha256': digest(path), 'executable': bool(path.stat().st_mode & 0o111)})
    assert files
    browser = stage / 'browsers/manifest.json'
    build = {'schema': 1, 'owner': 'KiaroSama', 'product': PRODUCT,
             'version': info['version'], 'source_sha': info['source_sha'],
             'generation': info['generation'], 'platform': platform, 'minimum_protocol': 1,
             'components': {'sdk': dependencies(), 'browsers': json.loads(browser.read_text(encoding='utf-8')) if browser.is_file() else {}},
             'files': files}
    content = (json.dumps(build, separators=(',', ':'), ensure_ascii=False) + '\n').encode('utf-8')
    assert len(content) <= 8 * 1024 * 1024
    (stage / 'BUILD.json').write_bytes(content)
    (stage / 'BUILD.sig').write_bytes(key().sign(content))
    return build


class Controller:
    def __init__(self, asset_directory: Path, token: str):
        self.assets = asset_directory
        self.tamper = False
        self.discovery = 'available'
        self.token = token
        self.events = deque()
        self.condition = threading.Condition()
        self.release = False
        self.requests = []
        self.checks = 0
        self.reported_pids = {}
        controller = self

        class Handler(BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)

            def log_message(self, *_):
                pass

            def reply(self, status, body=b''):
                self.send_response(status)
                self.send_header('Content-Length', str(len(body)))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(body)

            def authorized(self):
                return secrets.compare_digest(self.headers.get('Authorization', ''), 'Bearer ' + controller.token)

            def do_POST(self):
                if self.path != '/report' or not self.authorized():
                    self.reply(403)
                    return
                size = int(self.headers.get('Content-Length', '0'))
                if not 0 < size <= 16384:
                    self.reply(413)
                    return
                value = json.loads(self.rfile.read(size).decode('utf-8'))
                with controller.condition:
                    controller.events.append(value)
                    if isinstance(value.get('pid'), int) and value['pid'] > 0:
                        controller.reported_pids[value['pid']] = proctree.started_at(value['pid'])
                    controller.condition.notify_all()
                self.reply(200, b'{}')

            def do_GET(self):
                if self.path == '/checkpoint/release':
                    self.reply(200 if controller.release and self.authorized() else 409)
                    return
                route = urlsplit(self.path).path
                if route == '/releases':
                    page = urlsplit(self.path).query
                    with controller.condition:
                        controller.checks += 1
                        discovery = controller.discovery
                    rows = ([{'draft': False, 'tag_name': 'build-' + controller.source}]
                            if page == 'page=1' and discovery != 'empty' else [])
                    self.reply(200, json.dumps(rows).encode('utf-8'))
                    return
                prefix = '/build-' + controller.source + '/'
                if not route.startswith(prefix):
                    self.reply(404)
                    return
                name = route[len(prefix):]
                if '/' in name or '\\' in name or name in {'.', '..'}:
                    self.reply(403)
                    return
                path = controller.assets / name
                if not path.is_file():
                    self.reply(404)
                    return
                with controller.condition:
                    controller.requests.append(name)
                    discovery = controller.discovery
                if name == 'update-manifest.json' and discovery == 'metadata-offline':
                    self.reply(429, b'private fixture transport detail')
                    return
                if name == 'update-manifest.sig' and discovery == 'signature-offline':
                    self.reply(503, b'private fixture transport detail')
                    return
                if name == 'update-manifest.sig' and discovery == 'untrusted':
                    self.reply(200, bytes(64))
                    return
                self.send_response(200)
                self.send_header('Content-Length', str(path.stat().st_size))
                self.send_header('Connection', 'close')
                self.end_headers()
                with path.open('rb') as stream:
                    if controller.tamper and name.startswith('numera-acceptance.'):
                        first = stream.read(1)
                        self.wfile.write(bytes([first[0] ^ 1]))
                    shutil.copyfileobj(stream, self.wfile, 65536)

        self.server = HTTPServer(('127.0.0.1', 0), Handler)
        self.server.timeout = 0.2
        self.server.socket.settimeout(5)
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.serve, name='update-acceptance-controller')

    def serve(self):
        while not self.stop.is_set():
            self.server.handle_request()

    def start(self, source):
        self.source = source
        self.thread.start()
        return f'http://127.0.0.1:{self.server.server_port}'

    def event(self, kind, root, seconds=60, checkpoint=None, pid=None):
        deadline = time.monotonic() + seconds
        with self.condition:
            while True:
                for item in list(self.events):
                    if (item.get('event') == kind and Path(item.get('root', root)).resolve() == root.resolve()
                            and (checkpoint is None or item.get('checkpoint') == checkpoint)
                            and (pid is None or item.get('pid') == pid)):
                        self.events.remove(item)
                        return item
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(f'No actual updater event: {kind}/{checkpoint}')
                self.condition.wait(min(remaining, 0.2))

    def close(self):
        self.stop.set()
        if self.thread.ident is not None:
            self.thread.join(timeout=6)
            if self.thread.is_alive():
                raise RuntimeError('Acceptance controller did not stop')
        self.server.server_close()


def http(url, *, token, origin, payload=None, method='GET'):
    headers = {'Authorization': 'Bearer ' + token, 'Origin': origin}
    data = None if payload is None else json.dumps(payload).encode('utf-8')
    if data is not None:
        headers['Content-Type'] = 'application/json'
    request = Request(url, data=data, headers=headers, method=method)
    try:
        with OPENER.open(request, timeout=10) as response:
            return response.status, json.loads(response.read(16385).decode('utf-8'))
    except HTTPError as error:
        return error.code, json.loads(error.read(16385).decode('utf-8'))


def poll(observe, description, seconds=60):
    deadline = time.monotonic() + seconds
    while True:
        value = observe()
        if value:
            return value
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(description)
        threading.Event().wait(min(0.05, remaining))


class Harness:
    def __init__(self, workspace, old_stage, new_stage, platform, seconds, log):
        self.workspace, self.old_stage, self.new_stage, self.platform, self.log = workspace, old_stage, new_stage, platform, log
        self.deadline = time.monotonic() + seconds
        self.owned = []
        self.env = os.environ.copy()
        for name in list(self.env):
            if name.startswith(('NUMERA_', 'CLOAKBROWSER_', 'CAMOUFOX_', 'PLAYWRIGHT_', 'PATCHRIGHT_')):
                self.env.pop(name)
        for name in ('PYTHONPATH', 'PYTHONHOME'):
            self.env.pop(name, None)
        self.env.update(PYTHONUTF8='1', PYTHONIOENCODING='utf-8')
        self.old = identity(old_stage, self.env)
        self.new = identity(new_stage, self.env)
        self.shared_assets = None
        self.shared_archive = None
        self.shared_new_build = None
        assert self.old['source_sha'] == self.new['source_sha'], 'Fixture builds must use the same actual source SHA'
        assert self.new['generation'] > self.old['generation'], 'Two actual compiled generations are required'

    def spawn(self, argv, env, *, handoff=False):
        number = len(self.owned)
        sink_path = self.workspace / f'process-{number}.log'
        # All separately-owned engine/SDK groups inherit this real receipt.
        # A pending transfer stays an error; cleanup never guesses it empty.
        reports = self.workspace / 'jobs'
        reports.mkdir(exist_ok=True)
        report = reports / f'acceptance-{number}-{secrets.token_hex(6)}.units'
        report.touch()
        unit_report.initialize(report)
        child_env = {**env, unit_report.UNITS_ENV: str(report)}
        with sink_path.open('wb') as sink:
            process, tree = proctree.spawn([str(arg) for arg in argv], env=child_env,
                                          stdin=subprocess.PIPE if handoff else subprocess.DEVNULL,
                                          stdout=sink, stderr=sink)
        self.owned.append((process, tree, report))
        if handoff:
            process.stdin.close()  # Real pipe EOF, including Windows PeekNamedPipe.
        self.log.info('Actual updater process owned pid=%s', process.pid)
        return process

    def track(self, pid, token=None):
        token = proctree.started_at(pid) if token is None else token
        if token is not None:
            self.owned.append((None, proctree.Tree(pid=pid, token=token, group=None, kind='pid'), None))

    def sweep(self):
        # Unix worker/new desktop may intentionally lead separate groups.
        # Adopt each reported fixture process only after capturing its birth.
        failed = []
        retained = []
        for process, tree, report in reversed(self.owned):
            try:
                if process is not None:
                    proctree.end_session(process, tree, grace=1, kill_grace=3)
                elif not proctree.end_tree(tree, grace=1, kill_grace=3):
                    raise RuntimeError('Reported owner did not settle')
                if report is not None:
                    receipt = unit_report.read(report)
                    for unit in receipt.trees:
                        if not proctree.end_tree(unit, grace=1, kill_grace=3):
                            failed.append(unit.pid)
                    if not receipt.complete:
                        raise RuntimeError('Ownership transfer receipt remains incomplete')
            except Exception:
                failed.append(tree.pid)
                retained.append((process, tree, report))
        self.owned = retained
        if failed:
            raise RuntimeError(f'Owned acceptance processes did not settle: {failed}')

    def fixture(self, name):
        directory = self.workspace / name
        directory.mkdir()
        old = directory / PRODUCT
        clone(self.old_stage, old)
        old_build = sign_build(old, self.old, self.platform)
        if self.shared_assets is not None:
            return directory, old, self.shared_archive, old_build, self.shared_new_build
        shared = self.workspace / 'shared-update'
        shared.mkdir()
        new = shared / PRODUCT
        clone(self.new_stage, new)
        new_build = sign_build(new, self.new, self.platform)
        assets = shared / 'assets'
        assets.mkdir()
        archive = assets / ('numera-acceptance.zip' if os.name == 'nt' else 'numera-acceptance.tar.gz')
        if archive.suffix == '.zip':
            with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as packed:
                for path in sorted(new.rglob('*')):
                    if path.is_file():
                        packed.write(path, PRODUCT + '/' + path.relative_to(new).as_posix())
            with zipfile.ZipFile(archive) as packed:
                members = len(packed.infolist())
                expanded = sum(entry.file_size for entry in packed.infolist())
        else:
            with tarfile.open(archive, 'w:gz', dereference=False) as packed:
                packed.add(new, arcname=PRODUCT)
            with tarfile.open(archive) as packed:
                rows = packed.getmembers()
                members, expanded = len(rows), sum(row.size for row in rows if row.isfile())
        asset = {'platform': self.platform, 'name': archive.name, 'size': archive.stat().st_size,
                 'sha256': digest(archive), 'expanded_size': expanded, 'max_members': members,
                 'archive_root': PRODUCT, 'build_manifest_sha256': digest(new / 'BUILD.json')}
        now = int(time.time())
        document = {'schema': 1, 'repository': 'KiaroSama/Numera-Browser', 'product': PRODUCT,
                    'version': self.new['version'], 'source_sha': self.new['source_sha'],
                    'generation': self.new['generation'], 'created_at': now - 1, 'expires_at': now + 3600,
                    'minimum_protocol': 1, 'assets': [asset], 'components': {self.platform: new_build['components']}}
        message = (json.dumps(document, separators=(',', ':')) + '\n').encode('utf-8')
        (assets / 'update-manifest.json').write_bytes(message)
        (assets / 'update-manifest.sig').write_bytes(key().sign(message))
        self.shared_assets, self.shared_archive, self.shared_new_build = assets, archive, new_build
        # Archive+BUILD are authenticated before this disposable duplicate is
        # removed; cases reuse one archive instead of cloning every new build.
        shutil.rmtree(new)
        return directory, old, archive, old_build, new_build

    def data(self, root, env):
        data = root / 'data'
        data.mkdir()
        (data / '.imported').write_text('Acceptance data only.\n', encoding='utf-8')
        engine = root / 'engine' / ('numera-engine.exe' if os.name == 'nt' else 'numera-engine')
        cli = [engine, '--data-dir', data]
        config = json.loads(command([*cli, 'config', '--default-engine', 'playwright', '--headless-launch', 'on', '--json'], env))
        assert config['default_engine'] == 'playwright' and config['headless_launch'] is True
        created = json.loads(command([*cli, 'create', 'Update preservation fixture', '--seed', '42', '--json'], env, 60))
        profile_id = created['profile']['id']
        imported = json.loads(command([*cli, 'pool', 'add', '-', '--json'], env,
                                     data=f'http://acceptance:{PROXY_PASSWORD}@127.0.0.1:9\n'.encode('utf-8')))
        assert imported['added'] == 1, 'Real proxy fixture was not imported'
        assigned = json.loads(command([*cli, 'pool', 'assign', profile_id, '--json'], env))
        assert assigned['assigned'] == 1, 'Real proxy fixture was not assigned'
        # Only these finite fixture commands receive the public synthetic master
        # password. No argv, global environment, shell/worker child or log gets it.
        fixture_env = {**env, 'NUMERA_PASSWORD': VAULT_PASSWORD}
        try:
            enabled = json.loads(command([*cli, 'vault', 'enable', '--json'], fixture_env, 60))
            assert enabled['enabled'] is True and enabled['encrypted'] == 3, 'Vault did not encrypt both profile stores and pool'
            reopened = json.loads(command([*cli, 'show', profile_id, '--json'], fixture_env))
            assert reopened['proxy']['password'] == PROXY_PASSWORD, 'Actual CLI could not reopen the vault fixture'
        finally:
            fixture_env.pop('NUMERA_PASSWORD', None)
        config = json.loads((data / 'config.json').read_text(encoding='utf-8'))
        assert config['vault']['salt'] and config['vault']['verifier'].startswith('enc:v')
        engine_profile = json.loads((data / 'profiles' / f'{profile_id}.json').read_text(encoding='utf-8'))
        dashboard_profile = json.loads((data / 'dashboard' / f'{profile_id}.json').read_text(encoding='utf-8'))
        pool = json.loads((data / 'proxies.json').read_text(encoding='utf-8'))
        assert all(value.startswith('enc:v') for value in (engine_profile['proxy']['password'],
                   dashboard_profile['proxy']['pass'], pool[0]['pass'])), 'Fixture contains an unencrypted stored proxy'
        browser = data / 'user-data' / profile_id / 'Default'
        browser.mkdir(parents=True)
        (browser / 'Preferences').write_text('{"acceptance":"Preserve café فارسی"}\n', encoding='utf-8')
        (data / 'user-preserved.txt').write_text('User Unicode fixture: café فارسی\n', encoding='utf-8')
        return {path.relative_to(data).as_posix(): path.read_bytes() for path in data.rglob('*')
                if path.is_file() and '.locks' not in path.parts and path.suffix != '.log'}

    def verify_data(self, root, expected):
        for name, contents in expected.items():
            assert (root / 'data' / name).read_bytes() == contents, f'Updater changed user fixture: {name}'

    def case(self, name, fault=None, tamper=False, active=False, staged_substitution=False):
        if time.monotonic() >= self.deadline:
            raise TimeoutError('Updater acceptance exceeded whole-run deadline')
        directory, root, archive, old_build, _ = self.fixture(name)
        token = secrets.token_hex(32)
        controller = Controller(self.shared_assets, token)
        controller.tamper = tamper
        if name == 'success':
            controller.discovery = 'metadata-offline'
        shell = root / ('numera.exe' if os.name == 'nt' else 'numera')
        holder = None
        passed = False
        try:
            origin = controller.start(self.new['source_sha'])
            env = {**self.env, 'NUMERA_ACCEPTANCE_FEED_ORIGIN': origin,
                   'NUMERA_ACCEPTANCE_REPORT_TOKEN': token, 'NUMERA_LOG_DIR': str(directory / 'logs')}
            if fault:
                env['NUMERA_ACCEPTANCE_CHECKPOINT'] = fault
            expected = self.data(root, env)
            process = self.spawn([shell], env)
            bridge = controller.event('bridge', root)
            initial_ready = controller.event('ready', root)
            webview = controller.event('webview', root, pid=process.pid)
            assert initial_ready['pid'] == webview['pid'] == process.pid
            assert initial_ready['generation'] == webview['generation'] == self.old['generation']
            def status():
                code, value = http(bridge['endpoint'] + '/status', token=bridge['token'], origin=bridge['engineOrigin'])
                assert code == 200
                return value
            if name == 'success':
                for index, (mode, expected_state) in enumerate([
                    ('metadata-offline', 'error'), ('signature-offline', 'error'),
                    ('untrusted', 'error'), ('empty', 'current'), ('available', 'available'),
                    ('metadata-offline', 'error'), ('available', 'available'),
                ]):
                    if index:
                        with controller.condition:
                            controller.discovery = mode
                            checks = controller.checks
                        code, _ = http(bridge['endpoint'] + '/check', token=bridge['token'],
                                       origin=bridge['engineOrigin'], payload={}, method='POST')
                        assert code == 200, 'Manual discovery was refused before starting'
                    else:
                        checks = 0
                    def checked():
                        with controller.condition:
                            started = controller.checks > checks
                        if not started:
                            return None
                        value = status()
                        return value if value['state'] not in {'idle', 'checking'} else None
                    terminal = poll(checked, 'Discovery did not settle its exact check')
                    assert terminal['state'] == expected_state, f'Discovery status differs: {mode}'
                    if expected_state != 'available':
                        assert terminal['offer'] is None, 'Failed check retained an installable offer'
                    assert 'private fixture' not in terminal['message'], 'Discovery leaked a raw response'
                    assert archive.name not in controller.requests, 'Discovery downloaded without consent'
                    self.verify_data(root, expected)
            else:
                poll(lambda: value if (value := status())['state'] == 'available' else None, 'No signed fixture offer')
            offer = status()['offer']['id']
            assert archive.name not in controller.requests, 'Automatic check downloaded an archive without consent'
            code, _ = http(bridge['endpoint'] + '/download', token=bridge['token'], origin=bridge['engineOrigin'],
                           payload={'offerId': offer, 'approve': False}, method='POST')
            assert code == 400 and archive.name not in controller.requests, 'Declined download mutated network state'
            code, _ = http(bridge['endpoint'] + '/download', token=bridge['token'], origin=bridge['engineOrigin'],
                           payload={'offerId': offer, 'approve': True}, method='POST')
            assert code == 200
            terminal = poll(lambda: (value if value['state'] in {'ready', 'error'} else None)
                            if (value := status()) else None, 'Approved download did not finish')
            if tamper:
                assert terminal['state'] == 'error', 'Tampered archive was accepted'
                assert identity(root, self.env)['generation'] == self.old['generation']
                self.verify_data(root, expected)
                passed = True
                return
            assert terminal['state'] == 'ready', terminal['state']
            if staged_substitution:
                assert fault == 'quiesced', 'Substitution requires the real pre-worker checkpoint'
                journal = next(root.glob('.numera-update-*/journal.json'))
                inventory = journal.parent / 'next/BUILD.json'
                candidate = json.loads(inventory.read_text(encoding='utf-8'))
                candidate['version'] = '0.0.0-substituted-fixture'
                content = (json.dumps(candidate, separators=(',', ':')) + '\n').encode('utf-8')
                proof = key().sign(content)
                key().public_key().verify(proof, content)
                inventory.write_bytes(content)
                inventory.with_name('BUILD.sig').write_bytes(proof)
                original_metadata = {name: (root / name).read_bytes() for name in ('BUILD.json', 'BUILD.sig')}
            if active:
                with socket.socket() as reservation:
                    reservation.bind(('127.0.0.1', 0))
                    port = reservation.getsockname()[1]
                other = directory / 'other-data-root'
                other.mkdir()
                (other / '.imported').write_text('Other-root fixture only.\n', encoding='utf-8')
                holder = self.spawn([root / 'engine' / ('numera-engine.exe' if os.name == 'nt' else 'numera-engine'),
                                     '--data-dir', other, 'serve', '--host', '127.0.0.1', '--port', str(port)], env)
                def other_ready():
                    try:
                        with OPENER.open(f'http://127.0.0.1:{port}/api/ready', timeout=1) as response:
                            return json.loads(response.read(4096))['service'] == 'numera-browser'
                    except (OSError, ValueError):
                        assert holder.poll() is None, 'Foreign-root lease fixture exited before readiness'
                        return False
                poll(other_ready, 'Foreign-root engine did not acquire a live runtime lease')
            code, _ = http(bridge['endpoint'] + '/install', token=bridge['token'], origin=bridge['engineOrigin'],
                           payload={'offerId': offer, 'approve': True}, method='POST')
            assert code == 200
            if fault:
                witness = controller.event('checkpoint', root, checkpoint=fault)
                witness_token = controller.reported_pids.get(witness['pid'])
                assert witness_token is not None and proctree.alive(witness['pid'], witness_token)
                self.track(witness['pid'], witness_token)
                if os.name == 'nt':
                    import ctypes
                    from ctypes import wintypes
                    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
                    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
                    kernel.OpenProcess.restype = wintypes.HANDLE
                    kernel.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
                    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
                    handle = kernel.OpenProcess(0x0001, False, witness['pid'])
                    if not handle:
                        raise ctypes.WinError(ctypes.get_last_error())
                    try:
                        assert proctree.alive(witness['pid'], witness_token), 'Checkpoint PID was recycled'
                        if not kernel.TerminateProcess(handle, 99):
                            raise ctypes.WinError(ctypes.get_last_error())
                    finally:
                        kernel.CloseHandle(handle)
                else:
                    os.kill(witness['pid'], signal.SIGKILL)
                def fault_exited():
                    if witness['pid'] == process.pid:
                        process.poll()  # Reap our child so its zombie is not mistaken for a live owner.
                    return not proctree.alive(witness['pid'], witness_token)
                poll(fault_exited, 'Faulted owner did not exit')
                journal = next(root.glob('.numera-update-*/journal.json'))
                worker = journal.parent / ('update-worker.exe' if os.name == 'nt' else 'update-worker')
                env.pop('NUMERA_ACCEPTANCE_CHECKPOINT', None)
                if staged_substitution:
                    log_path = self.workspace / f'process-{len(self.owned)}.log'
                    recovery = self.spawn([shell, '--numera-update-worker', journal], env, handoff=True)
                    recovery.wait(timeout=60)
                    assert recovery.returncode == 1, 'Actual worker accepted a substituted signed inventory'
                    diagnostics = log_path.read_text(encoding='utf-8')
                    assert 'Downloaded generation differs from the approved signed offer' in diagnostics, 'Worker refused before the intended identity policy'
                    retained = json.loads(journal.read_text(encoding='utf-8'))
                    assert retained['phase'] == 'ready' and not retained['moved'] and not retained['installed'] and retained['pending'] is None
                    assert not (journal.parent / 'previous').exists(), 'Refused worker started immutable replacement'
                    assert not (root / '.updates/activation.json').exists(), 'Refused worker published activation ownership'
                    gate = json.loads((root / 'data/.updates/admission.json').read_text(encoding='utf-8'))
                    assert gate['schema'] == 1 and gate['state'] == 'quiesced' and gate['ticket_sha256'] == hashlib.sha256(retained['ticket'].encode('ascii')).hexdigest(), 'Refused worker lost matching recoverable admission evidence'
                    for metadata_name, original in original_metadata.items():
                        assert (root / metadata_name).read_bytes() == original, 'Refused worker changed installed metadata'
                    for entry in old_build['files']:
                        path = root / entry['path']
                        if 'link' in entry:
                            assert path.is_symlink() and os.readlink(path) == entry['link']
                        else:
                            assert not path.is_symlink() and path.stat().st_size == entry['size'] and digest(path) == entry['sha256']
                    self.verify_data(root, expected)
                    self.log.info('Actual updater worker refused substituted signed inventory before mutation')
                    passed = True
                    return
                recovery = self.spawn([worker if worker.is_file() else shell, '--numera-update-recover', journal], env)
                recovery.wait(timeout=60)
                assert recovery.returncode == 0, 'Actual update recovery failed'
                ready = controller.event('ready', root)
                assert ready['generation'] == self.old['generation'], 'Interrupted activation did not restore old generation'
            else:
                ready = controller.event('ready', root)
                assert ready['generation'] == (self.old['generation'] if active else self.new['generation'])
            self.track(ready['pid'], controller.reported_pids.get(ready['pid']))
            webview = controller.event('webview', root, pid=ready['pid'])
            assert webview['pid'] == ready['pid'] and webview['generation'] == ready['generation'], 'Real replacement WebView did not finish loading'
            phase = 'rolled-back' if fault or active else 'complete'
            poll(lambda: json.loads(next(root.glob('.numera-update-*/journal.json')).read_text(encoding='utf-8'))['phase'] == phase, 'Worker did not settle its final journal')
            poll(lambda: not (root / 'data/.updates/admission.json').exists(), 'Root admission gate survived final readiness')
            poll(lambda: not (root / '.updates/activation.json').exists(), 'Installation activation gate survived final readiness')
            # A standalone identity command must wait until the activation marker
            # is removed; unlike the recovered shell it has no private nonce.
            assert identity(root, self.env)['generation'] == ready['generation']
            self.verify_data(root, expected)
            self.log.info('Actual two-generation updater case passed name=%s', name)
            passed = True
        finally:
            controller.release = True
            try:
                # Retain every reported actual process owner, even when an
                # assertion failed before the normal ready/checkpoint adoption.
                with controller.condition:
                    for pid, birth in controller.reported_pids.items():
                        if birth is not None:
                            self.track(pid, birth)
                self.sweep()
            finally:
                controller.close()
            if passed and not staged_substitution:
                # The refused substitution retains its matching admission/journal
                # for inspection; it is not a completed activation or recovery.
                # Other exact successful fixtures are disposable after the sweep.
                assert directory.parent == self.workspace and directory.name == name
                shutil.rmtree(directory)


def run(args, log):
    args.directory.mkdir(parents=True, exist_ok=False)
    harness = Harness(args.directory, args.old_stage.resolve(strict=True), args.new_stage.resolve(strict=True), args.platform, args.seconds, log)
    for name, options in [('success', {}), ('tampered', {'tamper': True}),
                          ('foreign-runtime', {'active': True}),
                          ('quiesced-crash', {'fault': 'quiesced'}),
                          ('staged-substitution', {'fault': 'quiesced', 'staged_substitution': True}),
                          ('file-move-crash', {'fault': 'first-file-moved'})]:
        harness.case(name, **options)
    log.info('All actual fixture-generation updater cases passed; not a production-key release claim')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    setup = commands.add_parser('prepare')
    setup.add_argument('--directory', type=Path, required=True)
    verify = commands.add_parser('run')
    verify.add_argument('--old-stage', type=Path, required=True)
    verify.add_argument('--new-stage', type=Path, required=True)
    verify.add_argument('--directory', type=Path, required=True)
    verify.add_argument('--platform', choices=('windows-x64', 'linux-x64', 'macos-arm64'), required=True)
    verify.add_argument('--seconds', type=float, default=1200)
    args = parser.parse_args()
    with run_log('update-acceptance') as logger:
        if args.command == 'prepare':
            prepare(args.directory)
            logger.info('Public synthetic updater fixture key and generation recipe prepared')
        else:
            run(args, logger)
