"""Camoufox prerequisites preserve required libraries without redundant APT work."""
from __future__ import annotations

import importlib.util
from functools import partial
from pathlib import Path
import subprocess
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)
PACKAGES = (
    'libasound2t64', 'libatk1.0-0t64', 'libavcodec60', 'libcairo-gobject2',
    'libcairo2', 'libdbus-1-3', 'libfontconfig1', 'libfreetype6',
    'libgdk-pixbuf-2.0-0', 'libglib2.0-0t64', 'libgtk-3-0t64',
    'libpango-1.0-0', 'libpangocairo-1.0-0', 'libx11-6', 'libx11-xcb1',
    'libxcb-shm0', 'libxcb1', 'libxcomposite1', 'libxcursor1', 'libxdamage1',
    'libxext6', 'libxfixes3', 'libxi6', 'libxrandr2', 'libxrender1',
    'xvfb', 'libdbus-glib-1-2', 'libxtst6', 'libpci3', 'zstd',
)


def load():
    spec = importlib.util.spec_from_file_location(
        'camoufox_runtime_fixture', ROOT / 'scripts/ci/camoufox_runtime.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def hosted(helper, monkeypatch, tmp_path):
    for name, value in {
        'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'KiaroSama/Numera-Browser',
        'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64', 'NUMERA_TEST_ENGINE': 'camoufox',
    }.items():
        monkeypatch.setenv(name, value)
    monkeypatch.setattr(helper, 'platform', SimpleNamespace(
        system=lambda: 'Linux', machine=lambda: 'x86_64',
        freedesktop_os_release=lambda: {'ID': 'ubuntu', 'VERSION_ID': '24.04'}))
    monkeypatch.setattr(helper, 'version', lambda name: {
        'camoufox': '0.5.7', 'playwright': '1.62.0'}[name])
    monkeypatch.setattr(helper, 'run_log', partial(helper.run_log, directory=tmp_path / 'logs'))


def status(missing=()):
    return ''.join(f'{name}\tamd64\tinstalled\tok\n'
                   for name in PACKAGES if name not in missing)


@pytest.mark.parametrize('missing', [(), ('libpci3',), ('libpci3', 'zstd')],
                         ids=['installed', 'one-missing', 'two-missing'])
def test_entrypoint_installs_only_missing_runtime_and_rechecks_all(
        monkeypatch, tmp_path, missing):
    helper = load()
    hosted(helper, monkeypatch, tmp_path)
    calls = []
    queries = 0

    def command(argv, **kwargs):
        nonlocal queries
        calls.append(argv)
        assert kwargs['stdin'] == subprocess.DEVNULL
        assert kwargs.get('shell', False) is False
        assert kwargs.get('timeout', 0) > 0
        if argv[0] == 'dpkg-query':
            assert argv[-30:] == list(PACKAGES)
            assert kwargs['encoding'] == 'utf-8' and kwargs['errors'] == 'strict'
            queries += 1
            absent = missing if queries == 1 else ()
            return SimpleNamespace(returncode=int(bool(absent)), stdout=status(absent), stderr='')
        assert missing, 'Healthy installed runtime must not invoke sudo or APT'
        assert argv[:7] == ['sudo', '-n', 'env', 'DEBIAN_FRONTEND=noninteractive',
                            'LC_ALL=C.UTF-8', 'LANGUAGE=C', 'apt-get']
        assert kwargs['env']['DEBIAN_FRONTEND'] == 'noninteractive'
        return SimpleNamespace(returncode=0)

    monkeypatch.setattr(helper.subprocess, 'run', command)
    assert helper.main() == 0
    if missing:
        assert len(calls) == 4 and queries == 2
        prefix = ['sudo', '-n', 'env', 'DEBIAN_FRONTEND=noninteractive',
                  'LC_ALL=C.UTF-8', 'LANGUAGE=C', 'apt-get']
        assert calls[1] == [*prefix, 'update']
        assert calls[2] == [*prefix, 'install', '-y', '--no-install-recommends',
                            '--no-remove', '--no-upgrade', *missing]
    else:
        assert len(calls) == 1 and queries == 1
    logs = list((tmp_path / 'logs').glob('camoufox-runtime_*_UTC.log'))
    assert len(logs) == 1
    assert 'Runtime packages verified count=30' in logs[0].read_text(encoding='utf-8')
    assert helper.main() == 0
    assert len(list((tmp_path / 'logs').glob('camoufox-runtime_*.log'))) == 2


@pytest.mark.parametrize('damage', [
    'foreign-repo', 'local', 'engine', 'runner-arch', 'host-arch', 'distro',
    'release', 'sdk', 'query-fatal', 'query-timeout', 'query-malformed',
    'duplicate', 'foreign-package', 'foreign-architecture', 'reinst-required',
    'half-configured', 'empty-success', 'contradictory-exit',
    'update-failed', 'install-failed', 'postcondition',
])
def test_entrypoint_refuses_unproved_host_observation_or_install(
        monkeypatch, tmp_path, damage):
    helper = load()
    hosted(helper, monkeypatch, tmp_path)
    if damage in {'foreign-repo', 'local', 'engine', 'runner-arch'}:
        key = {'foreign-repo': 'GITHUB_REPOSITORY', 'local': 'GITHUB_ACTIONS',
               'engine': 'NUMERA_TEST_ENGINE', 'runner-arch': 'RUNNER_ARCH'}[damage]
        monkeypatch.setenv(key, 'unapproved')
    if damage == 'host-arch':
        helper.platform.machine = lambda: 'aarch64'
    if damage in {'distro', 'release'}:
        helper.platform.freedesktop_os_release = lambda: {
            'ID': 'debian' if damage == 'distro' else 'ubuntu',
            'VERSION_ID': '26.04' if damage == 'release' else '24.04'}
    if damage == 'sdk':
        monkeypatch.setattr(helper, 'version', lambda name: 'unsupported')
    calls = []
    queries = 0

    def command(argv, **kwargs):
        nonlocal queries
        calls.append(argv)
        if argv[0] != 'dpkg-query':
            operation = argv[argv.index('apt-get') + 1]
            failed = ((damage == 'update-failed' and operation == 'update') or
                      (damage == 'install-failed' and operation == 'install'))
            return SimpleNamespace(returncode=100 if failed else 0)
        queries += 1
        if damage == 'query-timeout':
            raise subprocess.TimeoutExpired(argv, 10)
        missing = damage in {'update-failed', 'install-failed', 'postcondition'}
        output = status(('libpci3',)) if missing else status()
        code = int(missing)
        if damage == 'query-fatal':
            code = 2
        elif damage == 'query-malformed':
            output = 'not a package record\n'
        elif damage == 'duplicate':
            output += output.splitlines()[0] + '\n'
        elif damage == 'foreign-package':
            output += 'unexpected\tamd64\tinstalled\tok\n'
        elif damage == 'foreign-architecture':
            output = output.replace('amd64', 'arm64', 1)
        elif damage == 'reinst-required':
            output = output.replace('\tok\n', '\treinstreq\n', 1)
        elif damage == 'half-configured':
            output = output.replace('installed', 'half-configured', 1)
        elif damage == 'empty-success':
            output = ''
        elif damage == 'contradictory-exit':
            code = 1
        return SimpleNamespace(returncode=code, stdout=output, stderr='private diagnostic')

    monkeypatch.setattr(helper.subprocess, 'run', command)
    assert helper.main() == 1
    if damage in {'foreign-repo', 'local', 'engine', 'runner-arch', 'host-arch',
                  'distro', 'release', 'sdk'}:
        assert calls == [], 'Unsupported host must refuse before external commands'
    elif damage == 'postcondition':
        assert len(calls) == 4 and queries == 2
    elif damage == 'install-failed':
        assert len(calls) == 3
    elif damage == 'update-failed':
        assert len(calls) == 2
    else:
        assert len(calls) == 1, 'Unknown package state must not be repaired speculatively'
    logs = list((tmp_path / 'logs').glob('camoufox-runtime_*.log'))
    assert len(logs) == 1
    output = logs[0].read_text(encoding='utf-8')
    assert '[ERROR]' in output and 'exit=1' in output
    assert 'private diagnostic' not in output


def test_workflow_keeps_camoufox_runtime_and_real_sdk_acceptance_bounded():
    workflow = (ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8')
    start = workflow.index('      - name: Install Firefox system dependencies\n')
    step = workflow[start:workflow.index('      - name:', start + 10)]
    assert "if: runner.os == 'Linux' && matrix.engine == 'camoufox'" in step
    assert 'python scripts/migration/run_owned.py --wall 300 --idle 120 -- ' in step
    assert 'python scripts/ci/camoufox_runtime.py' in step, \
        'Camoufox still acquires generic Playwright fonts instead of verified runtime prerequisites'
    assert 'playwright install-deps firefox' not in step
    assert 'name: Required source session-tree live tests' in workflow
    assert 'name: Required native SDK context and cleanup' in workflow
    assert 'python -m camoufox fetch' in workflow


@pytest.mark.parametrize('layout', ['grouped', 'legacy', 'flat'])
@pytest.mark.parametrize('damage', [None, 'config', 'empty-fonts', 'bad-groups',
                                    'missing-group', 'wrong-binding', 'malformed-config',
                                    'comment-marker', 'cdata-marker'])
def test_prepare_requires_real_camoufox_font_bundle_without_launch_or_mutation(
        tmp_path, monkeypatch, layout, damage):
    spec = importlib.util.spec_from_file_location('live_font_fixture', ROOT / 'scripts/live_engine.py')
    live = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(live)
    assert_font_preparation(live, tmp_path, monkeypatch, layout, damage)


def assert_font_preparation(live, tmp_path, monkeypatch, layout, damage):
    import json
    import sys

    bundle = tmp_path / 'browser with spaces'
    bundle.mkdir()
    binary = bundle / 'camoufox-bin'
    binary.write_bytes(b'not executed')
    binary.chmod(0o700)
    config_root = 'fontconfigs' if layout == 'legacy' else 'fontconfig'
    for identity in ('linux', 'macos', 'windows'):
        path = bundle / config_root / identity / 'fonts.conf'
        path.parent.mkdir(parents=True)
        path.write_text('<fontconfig><dir prefix="cwd">fonts</dir></fontconfig>', encoding='utf-8')
    fonts = bundle / 'fonts'
    fonts.mkdir()
    if layout == 'grouped':
        groups = {'readBy': {'lin': ['L'], 'mac': ['M', 'MX'], 'win': ['W']}}
        (fonts / 'groups.json').write_text(json.dumps(groups), encoding='utf-8')
        directories = [fonts / group for group in ('L', 'M', 'MX', 'W')]
    elif layout == 'legacy':
        directories = [fonts / identity for identity in ('linux', 'macos', 'windows')]
    else:
        directories = [fonts]
    for directory in directories:
        directory.mkdir(exist_ok=True)
        (directory / 'fixture.ttf').write_bytes(b'\x00\x01\x00\x00synthetic font header')
    if damage == 'config':
        (bundle / config_root / 'macos/fonts.conf').unlink()
    elif damage == 'empty-fonts':
        for directory in directories:
            (directory / 'fixture.ttf').unlink()
    elif damage == 'bad-groups':
        (fonts / 'groups.json').write_text('{invalid', encoding='utf-8')
    elif damage == 'missing-group':
        (fonts / 'groups.json').write_text(json.dumps({
            'readBy': {'lin': ['LX'], 'mac': ['LX'], 'win': ['LX']}}), encoding='utf-8')
    elif damage in {'wrong-binding', 'malformed-config', 'comment-marker', 'cdata-marker'}:
        marker = '<dir prefix="cwd">fonts</dir>'
        config = {
            'wrong-binding': '<fontconfig><dir>/missing</dir></fontconfig>',
            'malformed-config': '<fontconfig>',
            'comment-marker': f"<fontconfig><!-- {marker} --><dir prefix='cwd'>fonts</dir></fontconfig>",
            'cdata-marker': f"<fontconfig><![CDATA[{marker}]]><dir prefix='cwd'>fonts</dir></fontconfig>",
        }[damage]
        (bundle / config_root / 'linux/fonts.conf').write_text(config, encoding='utf-8')
    before = {str(p.relative_to(bundle)): p.read_bytes() for p in bundle.rglob('*') if p.is_file()}
    package = SimpleNamespace(camoufox_path=lambda **kwargs: bundle, launch_path=lambda path: binary)
    monkeypatch.setitem(sys.modules, 'camoufox.pkgman', package)
    monkeypatch.setattr(live, 'version', lambda name: '0.5.7')
    monkeypatch.setattr(live, 'sys', SimpleNamespace(platform='linux'))
    if damage:
        with pytest.raises((ValueError, RuntimeError), match='Camoufox bundled font'):
            live.prepare('camoufox')
    else:
        assert live.prepare('camoufox') == binary
    assert {str(p.relative_to(bundle)): p.read_bytes() for p in bundle.rglob('*') if p.is_file()} == before


@pytest.mark.parametrize('damage', ['config', 'empty-fonts', 'wrong-binding',
                                    'comment-marker', 'cdata-marker'])
def test_font_regression_rejects_the_previous_missing_guard(tmp_path, monkeypatch, damage):
    path = ROOT / 'scripts/live_engine.py'
    source = path.read_text(encoding='utf-8')
    guard = '            require_camoufox_fonts(binary.parent)'
    assert source.count(guard) == 1
    from types import ModuleType

    previous = ModuleType('live_font_before_guard')
    previous.__file__ = str(path)
    exec(compile(source.replace(guard, '            pass'), str(path), 'exec'), previous.__dict__)
    with pytest.raises(pytest.fail.Exception, match='DID NOT RAISE'):
        assert_font_preparation(previous, tmp_path, monkeypatch, 'grouped', damage)
