"""Pin the final image/client pair and test the offline browser smoke contract.

The controlled Playwright doubles below test failure handling, not an actual
browser. The required Docker CI step executes the real installed browser.
Resolver tests use synthetic wheels with pip offline, never registry packages.
"""
import importlib.util
import json
import logging
import os
import re
import shlex
import subprocess
import sys
import types
import zipfile
from pathlib import Path

import pytest

LOG = logging.getLogger(__name__)
ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts' / 'smoke_container_browser.py'


def image_version():
    text = (ROOT / 'Dockerfile').read_text(encoding='utf-8')
    matches = re.findall(r'^FROM mcr\.microsoft\.com/playwright/python:v([0-9.]+)-noble(?:\s|$)', text, re.M)
    assert len(matches) == 1, 'keep one exact literal runtime image tag visible to Dependabot'
    return matches[0]


def install_requirements():
    text = (ROOT / 'Dockerfile').read_text(encoding='utf-8').replace('\\\n', ' ')
    commands = [line for line in text.splitlines() if line.startswith('RUN ') and 'pip install' in line]
    assert len(commands) == 1, 'resolve the application and browser client together'
    tokens = shlex.split(commands[0])
    assert './engine[api,launch,secure]' in tokens
    env = re.search(r'NUMERA_CONTAINER_PLAYWRIGHT_VERSION=([0-9.]+)', text)
    requirements = ['numera_container_fixture']
    for token in tokens:
        if token.startswith('playwright=='):
            if env:
                token = token.replace('${NUMERA_CONTAINER_PLAYWRIGHT_VERSION}', env[1])
            requirements.append(token)
    return requirements


def test_container_definition_pins_client_to_literal_image():
    text = (ROOT / 'Dockerfile').read_text(encoding='utf-8')
    assert f'NUMERA_CONTAINER_PLAYWRIGHT_VERSION={image_version()}' in text
    assert install_requirements() == ['numera_container_fixture', f'playwright=={image_version()}']
    assert 'python -m pip check' in text


def test_container_ci_requires_bundled_browser_without_network_or_host_cache():
    ci = (ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8')
    docker = ci.split('\n  docker:', 1)[1].split('\n  dashboard:', 1)[0]
    assert '< scripts/smoke_container_browser.py' in docker
    smoke = docker.split('- name: The container launches its bundled Chromium', 1)[1].split('\n      - name:', 1)[0]
    for flag in ('--network none', '--init', '--shm-size=1g', '--entrypoint python', 'numera-browser:ci -'):
        assert flag in smoke
    for forbidden in ('continue-on-error', '|| true', '--user', '--volume', ' -v ', '--mount', 'playwright install'):
        assert forbidden not in smoke
    assert 'timeout --signal=TERM --kill-after=5s 60s' in smoke
    assert 'GITHUB_RUN_ID' in smoke and 'GITHUB_RUN_ATTEMPT' in smoke
    assert 'Remove an interrupted browser smoke container\n        if: always()' in docker
    assert 'needs: [engine, engine-windows, engine-live, browser, rust, docker, dashboard]' in ci
    triggers = ci.split('\non:\n', 1)[1].split('\n# Read-only', 1)[0]
    assert set(re.findall(r'^  ([a-z_]+):', triggers, re.M)) == {'workflow_dispatch', 'workflow_call'}


def _wheel(directory, name, version, requires=()):
    info = f'{name}-{version}.dist-info'
    data = f'Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n'
    data += ''.join(f'Requires-Dist: {item}\n' for item in requires) + '\n'
    with zipfile.ZipFile(directory / f'{name}-{version}-py3-none-any.whl', 'w') as archive:
        archive.writestr(f'{info}/METADATA', data)
        archive.writestr(f'{info}/WHEEL', 'Wheel-Version: 1.0\nGenerator: numera-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n')
        archive.writestr(f'{info}/RECORD', '')


@pytest.fixture
def wheels(tmp_path):
    pinned = image_version()
    major, minor, patch = map(int, pinned.split('.'))
    newer = f'{major}.{minor + 1}.0'
    _wheel(tmp_path, 'numera_container_fixture', '1.0.0', ['playwright>=1.40'])
    for release in (pinned, newer):
        _wheel(tmp_path, 'playwright', release)
    return tmp_path, pinned, newer


def _resolve(wheels, requirements):
    directory, pinned, newer = wheels
    report = directory / 'resolution.json'
    # These are synthetic metadata-only wheels; nothing is installed or fetched.
    result = subprocess.run([
        sys.executable, '-m', 'pip', 'install', '--disable-pip-version-check',
        '--no-cache-dir', '--no-index', '--find-links', str(directory),
        '--ignore-installed', '--dry-run', '--report', str(report), *requirements,
    ], capture_output=True, text=True, encoding='utf-8', timeout=30)
    LOG.debug('Offline resolver exited %s', result.returncode)
    return result, json.loads(report.read_text(encoding='utf-8')) if report.exists() else None


def test_container_install_does_not_select_newer_client(wheels):
    result, report = _resolve(wheels, install_requirements())
    assert result.returncode == 0, result.stderr
    versions = {item['metadata']['name']: item['metadata']['version'] for item in report['install']}
    assert versions['playwright'] == wheels[1]


def test_unconstrained_baseline_control_selects_newer_client(wheels):
    result, report = _resolve(wheels, ['numera_container_fixture'])
    assert result.returncode == 0, result.stderr
    assert next(item['metadata']['version'] for item in report['install'] if item['metadata']['name'] == 'playwright') == wheels[2]


def test_conflicting_client_requirement_is_refused(wheels):
    result, _ = _resolve(wheels, [*install_requirements(), f'playwright=={wheels[2]}'])
    assert result.returncode != 0, 'conflicting client requirements were accepted'


@pytest.fixture
def smoke():
    spec = importlib.util.spec_from_file_location('numera_image_smoke_test', SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def runtime(smoke, tmp_path, monkeypatch):
    """Only the Playwright boundary is doubled; execute the real smoke code."""
    events = []
    faults = {}
    root = tmp_path / 'ms-playwright'
    root.mkdir()
    executable = root / 'chromium' / 'chrome'
    executable.parent.mkdir()
    executable.write_text('fixture, not a real browser', encoding='utf-8')
    executable.chmod(0o700)
    monkeypatch.setattr(smoke, 'BROWSER_ROOT', root)
    monkeypatch.setattr(smoke, 'LOG_DIR', tmp_path / 'logs', raising=False)
    monkeypatch.setattr(smoke.os, 'geteuid', lambda: 1000, raising=False)
    monkeypatch.setattr(smoke.metadata, 'version', lambda name: image_version())

    def event(name):
        events.append(name)
        if faults.get(name):
            raise RuntimeError('synthetic ' + name)

    class Resource:
        def __init__(self, name):
            self.name = name
        def close(self):
            event('close-' + self.name)
        def new_context(self):
            event('context')
            return Resource('context')
        def new_page(self):
            event('page')
            return Resource('page')
        def set_content(self, html, **kwargs):
            event('content')
            assert smoke.PAGE_TEXT in html and kwargs['timeout'] > 0
        def locator(self, selector):
            assert selector == '#numera-smoke'
            return self
        def inner_text(self, **kwargs):
            event('text')
            return 'wrong' if faults.get('wrong-dom') else smoke.PAGE_TEXT
        def evaluate(self, expression):
            event('evaluate')
            return -1 if faults.get('wrong-js') else 42
        def is_connected(self):
            event('connection-check')
            return bool(faults.get('still-connected'))

    chromium = types.SimpleNamespace(executable_path=str(executable))
    def launch(**kwargs):
        event('launch')
        assert kwargs == {'executable_path': str(executable.resolve()), 'headless': True, 'timeout': 10000}
        return Resource('browser')
    chromium.launch = launch

    class Driver:
        def __enter__(self):
            event('driver-start')
            return types.SimpleNamespace(chromium=chromium)
        def __exit__(self, *args):
            event('driver-stop')

    api = types.ModuleType('playwright.sync_api')
    api.sync_playwright = Driver
    package = types.ModuleType('playwright')
    package.sync_api = api
    monkeypatch.setitem(sys.modules, 'playwright', package)
    monkeypatch.setitem(sys.modules, 'playwright.sync_api', api)
    return events, faults, chromium, executable


def test_smoke_success_closes_all_resources_and_driver(smoke, runtime):
    events, faults, chromium, executable = runtime
    smoke.check_browser(image_version())
    assert events[-5:] == ['close-page', 'close-context', 'close-browser', 'connection-check', 'driver-stop']


@pytest.mark.parametrize('expected', ['', 'latest', '1.62', '^1.62.0', '1.62.0\n'])
def test_missing_or_invalid_version_never_starts_driver(smoke, runtime, expected):
    with pytest.raises(smoke.SmokeFailure):
        smoke.check_browser(expected)
    assert not runtime[0]


def test_mismatched_client_never_starts_driver(smoke, runtime, monkeypatch):
    monkeypatch.setattr(smoke.metadata, 'version', lambda name: '0.0.0')
    with pytest.raises(smoke.SmokeFailure, match='does not match'):
        smoke.check_browser(image_version())
    assert not runtime[0]


def test_root_execution_is_refused(smoke, runtime, monkeypatch):
    monkeypatch.setattr(smoke.os, 'geteuid', lambda: 0)
    with pytest.raises(smoke.SmokeFailure, match='non-root'):
        smoke.check_browser(image_version())
    assert not runtime[0]


@pytest.mark.parametrize('damage', ['absent', 'directory', 'not-executable', 'outside-image'])
def test_invalid_image_executable_never_launches(smoke, runtime, monkeypatch, tmp_path, damage):
    events, faults, chromium, executable = runtime
    if damage == 'absent':
        executable.unlink()
    elif damage == 'directory':
        executable.unlink()
        executable.mkdir()
    elif damage == 'not-executable':
        monkeypatch.setattr(smoke.os, 'access', lambda *args: False)
    else:
        outside = tmp_path / 'host-cache-browser'
        outside.write_text('fixture', encoding='utf-8')
        chromium.executable_path = str(outside)
    with pytest.raises((smoke.SmokeFailure, FileNotFoundError)):
        smoke.check_browser(image_version())
    assert 'launch' not in events and events[-1] == 'driver-stop'


@pytest.mark.parametrize('failure', [
    'launch', 'context', 'page', 'content', 'text', 'evaluate', 'wrong-dom', 'wrong-js',
    'close-page', 'close-context', 'close-browser', 'connection-check', 'still-connected', 'driver-stop',
])
def test_failures_never_become_success_and_cleanup_continues(smoke, runtime, monkeypatch, failure):
    events, faults, chromium, executable = runtime
    faults[failure] = True
    monkeypatch.setenv(smoke.VERSION_ENV, image_version())
    assert smoke.main([]) == 1
    assert events[-1] == 'driver-stop'
    if failure != 'launch':
        assert 'close-browser' in events
    if failure not in ('launch', 'context'):
        assert 'close-context' in events
    if failure not in ('launch', 'context', 'page'):
        assert 'close-page' in events


def test_smoke_run_logs_are_unique_sanitized_and_closed(smoke, tmp_path, monkeypatch, capsys):
    directory = tmp_path / 'path with spaces' / 'logs'
    monkeypatch.setattr(smoke, 'LOG_DIR', directory, raising=False)

    def fail(expected):
        raise RuntimeError('password=synthetic-do-not-log-this')

    monkeypatch.setattr(smoke, 'check_browser', fail)
    for _ in range(2):
        assert smoke.main([]) == 1
    files = list(directory.glob('smoke_container_browser_*_UTC*.log'))
    assert len(files) == 2, 'each run must leave an independent diagnostic log'
    text = '\n'.join(path.read_text(encoding='utf-8') for path in files)
    console = capsys.readouterr().err
    assert 'synthetic-do-not-log-this' not in text + console
    assert 'UTC] [ERROR] [numera.container-smoke]' in text
    assert 'exit=1' in text
    for path in files:
        path.unlink()  # an open FileHandler prevents deletion on Windows


def test_cli_returns_failure_without_an_image_manifest(tmp_path):
    env = {key: value for key, value in os.environ.items() if key != 'NUMERA_CONTAINER_PLAYWRIGHT_VERSION'}
    env['UNRELATED_SECRET'] = 'synthetic-do-not-log-this'
    result = subprocess.run([sys.executable, str(SCRIPT)], env=env, capture_output=True,
                            text=True, timeout=10)
    assert result.returncode == 1
    assert '[ERROR]' in result.stderr and 'exact Playwright release' in result.stderr
    assert env['UNRELATED_SECRET'] not in result.stdout + result.stderr
