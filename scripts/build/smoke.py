"""Exercise a frozen distribution through CLI/HTTP with bounded owned cleanup."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from numera import proctree, unit_report  # noqa: E402
from run_logging import run_log  # noqa: E402


class Smoke:
    def __init__(self, engine: Path, workspace: Path, seconds: float, logger):
        self.engine = engine.resolve()
        self.workspace = workspace
        self.logger = logger
        self.deadline = time.monotonic() + seconds
        self.data = workspace / 'isolated-data'
        self.data.mkdir()
        (self.data / '.imported').write_text('Isolated hosted package fixture.\n', encoding='utf-8')
        self.owned = []
        self.sequence = 0
        self.env = os.environ.copy()
        for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR',
                    'NUMERA_PARENT_PID', 'PLAYWRIGHT_BROWSERS_PATH', 'CLOAKBROWSER_BINARY_PATH',
                    'CLOAKBROWSER_LICENSE_KEY'):
            self.env.pop(key, None)
        self.env.update(PYTHONUTF8='1', PYTHONIOENCODING='utf-8')
        for key in ('TEMP', 'TMP', 'TMPDIR'):
            self.env[key] = str(workspace)

    def remaining(self, limit: float = 60) -> float:
        left = self.deadline - time.monotonic()
        if left <= 0:
            raise TimeoutError('Packaged smoke exceeded its whole-run deadline')
        return min(limit, left)

    def spawn(self, arguments: list[str]):
        number = self.sequence
        self.sequence += 1
        report = self.workspace / f'units-{number}.jsonl'
        report.touch()
        unit_report.initialize(report)
        output = self.workspace / f'child-{number}.log'
        sink = output.open('wb')
        errors = self.workspace / f'child-{number}.stderr.log'
        err_sink = errors.open('wb')
        env = {**self.env, unit_report.UNITS_ENV: str(report)}
        try:
            proc, tree = proctree.spawn([str(self.engine), '--data-dir', str(self.data), *arguments],
                                        env=env, stdin=subprocess.DEVNULL, stdout=sink,
                                        stderr=err_sink)
        finally:
            sink.close()
            err_sink.close()
        self.owned.append((proc, tree, report, output))
        self.logger.info('Packaged child started pid=%s command=%s', proc.pid,
                         arguments[0] if arguments else 'unknown')
        return self.owned[-1]

    def settle(self, owned) -> None:
        proc, tree, report, _ = owned
        ends_at = time.monotonic() + 10
        problem = None
        try:
            proctree.end_session(proc, tree, ends_at=ends_at)
        except Exception as error:
            problem = error
        receipt = unit_report.read(report)
        for unit in receipt.trees:
            if not proctree.end_tree(unit, ends_at=ends_at):
                problem = RuntimeError('Packaged child SDK unit could not be proved empty')
        if not receipt.complete:
            problem = RuntimeError(f'Packaged child receipt incomplete: {receipt.problem}')
        if problem:
            raise problem
        self.logger.info('Packaged child settled pid=%s clean=True', proc.pid)
        if owned in self.owned:
            self.owned.remove(owned)

    def command(self, *arguments: str, allow_failure: bool = False, limit: float = 60,
                structured: bool = True):
        argv = list(arguments) + (['--json'] if structured else [])
        timeout = self.remaining(limit)
        owned = self.spawn(argv)
        proc, _, _, output = owned
        try:
            proc.wait(timeout=timeout)
            text = output.read_text(encoding='utf-8', errors='strict')
            if proc.returncode and not allow_failure:
                raise RuntimeError(f'Packaged command {arguments[0]} exited {proc.returncode}: {text[:1500]}')
            return json.loads(text) if structured else text
        finally:
            self.settle(owned)

    def http(self, port: int, path: str, *, method='GET', body=None):
        data = json.dumps(body).encode('utf-8') if body is not None else None
        request = Request(f'http://127.0.0.1:{port}{path}', data=data, method=method,
                          headers={'Content-Type': 'application/json'} if data is not None else {})
        with urlopen(request, timeout=self.remaining(60 if method == 'POST' else 5)) as response:
            content = response.read(16 * 1024 * 1024 + 1)
            if len(content) > 16 * 1024 * 1024:
                raise ValueError('Packaged HTTP response exceeded its limit')
            return content.decode('utf-8')

    def verify(self, browsers: bool) -> None:
        version = self.command('--version', structured=False)
        if not version.strip():
            raise AssertionError('Frozen engine version is empty')
        # The user's persisted default is unchanged; this isolated fixture must
        # select a redistributed browser rather than trigger Cloak's vendor download.
        created = self.command('create', 'package-smoke', *(['--engine', 'playwright'] if browsers else []))
        profile_id = created['profile']['id']
        if not created.get('ok'):
            raise AssertionError('Frozen engine did not create its profile')
        if self.command('list')['total'] != 1:
            raise AssertionError('Frozen engine did not read its profile back')
        if browsers:
            score = self.command('trust', profile_id, limit=90)
            if not score.get('checks') or 'score' not in score or not score.get('passed'):
                raise AssertionError('Bundled browser never produced a real inspected trust report')
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        server = self.spawn(['serve', '--port', str(port)])
        try:
            ready = time.monotonic() + self.remaining(60)
            while True:
                if server[0].poll() is not None:
                    raise RuntimeError('Frozen server exited before readiness')
                try:
                    answer = json.loads(self.http(port, '/api/ready'))
                    if answer.get('ok'):
                        break
                except (URLError, HTTPError, TimeoutError):
                    if time.monotonic() >= ready:
                        raise TimeoutError('Frozen server never became ready')
                    time.sleep(min(0.1, self.remaining()))
            html = self.http(port, '/')
            if '<div id="root">' not in html:
                raise AssertionError('Frozen engine did not serve the built dashboard')
            import re
            asset = re.search(r'/assets/[^"\s]+', html)
            if not asset or not self.http(port, asset[0]):
                raise AssertionError('Frozen dashboard asset is unavailable')
            profiles = json.loads(self.http(port, '/api/profiles'))
            if not any(item['id'] == profile_id for item in profiles):
                raise AssertionError('HTTP API lost the CLI-created profile')
            if browsers:
                launched = json.loads(self.http(port, f'/api/profiles/{profile_id}/launch',
                                                method='POST', body={'headless': True}))
                if not (launched.get('ok') or launched.get('status')):
                    raise AssertionError('Frozen API could not launch the bundled browser')
                running = json.loads(self.http(port, '/api/profiles'))
                if not any(row['id'] == profile_id and row.get('status') == 'running' for row in running):
                    raise AssertionError('Frozen launch did not retain browser ownership')
                self.http(port, f'/api/profiles/{profile_id}/stop', method='POST')
                probe = json.loads(self.http(port, f'/api/profiles/{profile_id}/trust', method='POST'))
                if not {'grade', 'score'} <= probe.keys():
                    raise AssertionError('Frozen self-spawn helper report is incomplete')
                after = json.loads(self.http(port, '/api/profiles'))
                if any(row.get('status') == 'running' for row in after):
                    raise AssertionError('Frozen stop/probe left a browser running')
        finally:
            self.settle(server)
        if self.command('list')['profiles'][0]['id'] != profile_id:
            raise AssertionError('Packaged profile did not survive a process restart')
        secret = 'package-smoke-proxy-password'
        proxy_file = self.workspace / 'fixture-proxies.txt'
        proxy_file.write_text(f'http://fixture:{secret}@203.0.113.7:8080\n', encoding='utf-8')
        self.command('pool', 'add', str(proxy_file))
        self.command('--password', 'package-fixture-master', 'vault', 'enable', limit=90)
        stored = (self.data / 'proxies.json').read_text(encoding='utf-8')
        if secret in stored or 'enc:v' not in stored:
            raise AssertionError('Frozen vault did not encrypt its secret')
        unlocked = self.command('--password', 'package-fixture-master', 'pool', 'export', structured=False)
        if secret not in unlocked:
            raise AssertionError('Frozen vault cannot reopen its secret')
        locked = self.command('pool', 'export', allow_failure=True, structured=False)
        if secret in locked:
            raise AssertionError('Frozen vault leaked a locked secret')
        self.logger.info('Packaged CLI/browser=%s/HTTP/dashboard/restart/vault checks passed', browsers)

    def close(self) -> None:
        errors = []
        for owned in reversed(list(self.owned)):
            try:
                self.settle(owned)
            except Exception as error:
                errors.append(error)
        if errors:
            raise RuntimeError(f'Package cleanup failed for {len(errors)} owned children') from errors[0]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stage', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=300)
    parser.add_argument('--no-browsers', action='store_true')
    args = parser.parse_args()
    import math
    if not math.isfinite(args.seconds) or args.seconds <= 0:
        parser.error('--seconds must be finite and positive')
    engine = args.stage / 'engine' / ('numera-engine.exe' if os.name == 'nt' else 'numera-engine')
    if not engine.is_file():
        parser.error('The assembled frozen engine is missing')
    scratch = ROOT / '.ci-work/package-smoke'
    scratch.mkdir(parents=True, exist_ok=True)
    workspace = Path(tempfile.mkdtemp(prefix='smoke-', dir=scratch))
    with run_log('package-smoke') as logger:
        smoke = Smoke(engine, workspace, args.seconds, logger)
        try:
            smoke.verify(not args.no_browsers)
        finally:
            smoke.close()
        shutil.rmtree(workspace)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
