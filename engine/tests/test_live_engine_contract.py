"""A green hosted gate requires selected SDK tests and exact trusted downloads."""

import importlib.util
from contextlib import contextmanager
from pathlib import Path
import re
from types import SimpleNamespace
import xml.etree.ElementTree as ET

import pytest

from numera import drivers

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("live_engine", ROOT / "scripts/live_engine.py")
live = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(live)


@pytest.mark.parametrize("damage", [None, "missing", "wrong-engine", "duplicate",
                                    "skipped", "failure", "error"])
def test_report_requires_both_selected_tests_to_pass(tmp_path, damage):
    suite = ET.Element("testsuite")
    for name in live.TREE_TESTS:
        ET.SubElement(suite, "testcase", name=f"{name}[patchright]")
    if damage == "missing":
        suite.remove(suite[0])
    elif damage == "wrong-engine":
        suite[0].set("name", f"{live.TREE_TESTS[0]}[playwright]")
    elif damage == "duplicate":
        suite[1].set("name", suite[0].get("name"))
    elif damage:
        ET.SubElement(suite[0], damage)
    report = tmp_path / "results.xml"
    ET.ElementTree(suite).write(report, encoding="utf-8", xml_declaration=True)
    if damage:
        with pytest.raises(RuntimeError):
            live.check_report(report, "patchright")
    else:
        live.check_report(report, "patchright")


def test_missing_report_and_browser_are_failures(tmp_path, monkeypatch):
    with pytest.raises(FileNotFoundError):
        live.check_report(tmp_path / "missing.xml", "playwright")

    @contextmanager
    def driver():
        yield SimpleNamespace(chromium=SimpleNamespace(executable_path=tmp_path / "missing"))

    monkeypatch.setattr(live, "version", lambda package: "test")
    monkeypatch.setattr(live.importlib, "import_module",
                        lambda module: SimpleNamespace(sync_playwright=driver))
    with pytest.raises(RuntimeError, match="browser binary is missing"):
        live.prepare("patchright")


def test_native_handoff_exports_only_the_verified_cloak_path(tmp_path, monkeypatch):
    binary = tmp_path / 'browser with spaces — fixture'
    binary.write_bytes(b'not executed')
    environment = tmp_path / 'github environment'
    environment.write_text('EXISTING=value\n', encoding='utf-8')
    monkeypatch.setenv('GITHUB_ENV', str(environment))
    live.export_native('cloak', binary)
    assert environment.read_text(encoding='utf-8') == (
        f'EXISTING=value\nCLOAKBROWSER_BINARY_PATH={binary.resolve()}\n')
    before = environment.read_bytes()
    live.export_native('playwright', binary)
    assert environment.read_bytes() == before
    with pytest.raises(FileNotFoundError):
        live.export_native('cloak', tmp_path / 'missing')
    assert environment.read_bytes() == before
    monkeypatch.delenv('GITHUB_ENV')
    with pytest.raises(RuntimeError, match='GitHub environment file'):
        live.export_native('cloak', binary)


@pytest.mark.parametrize('control', ['\n', '\r', '\x00', '\x1f', '\x7f'])
def test_native_handoff_rejects_environment_injection(tmp_path, monkeypatch, control):
    environment = tmp_path / 'github environment'
    monkeypatch.setenv('GITHUB_ENV', str(environment))
    binary = SimpleNamespace(resolve=lambda **kwargs: f'/fixture/browser{control}INJECTED=value')
    with pytest.raises(RuntimeError, match='invalid environment characters'):
        live.export_native('cloak', binary)
    assert not environment.exists()


def _jobs(path):
    text = path.read_text(encoding="utf-8").split("\njobs:\n", 1)[1]
    chunks = re.split(r"(?m)^  ([a-z][a-z0-9-]*):\n", text)
    return dict(zip(chunks[1::2], chunks[2::2]))


def test_every_supported_engine_is_required_on_both_platforms():
    jobs = _jobs(ROOT / ".github/workflows/ci.yml")
    matrix = jobs["engine-live"]
    engines = re.search(r"engine: \[([^\]]+)\]", matrix).group(1).split(", ")
    assert set(engines) == set(drivers.names()) == set(live.ENGINES)
    assert "os: [ubuntu-latest, windows-latest]" in matrix
    pins = re.findall(r"- engine: (\w+)\n\s+package: ([^\n]+)", matrix)
    assert {name for name, _ in pins} == set(engines)
    assert all(re.fullmatch(r"[a-z]+(?:\[geoip\])?==\d+\.\d+\.\d+", pin)
               for _, pin in pins)
    assert "NUMERA_TEST_ENGINE: ${{ matrix.engine }}" in matrix
    assert "NUMERA_NATIVE_LIVE_ENGINE: ${{ matrix.engine }}" in matrix
    assert 'CLOAKBROWSER_VERSION: "' + live.CLOAK_VERSION + '"' in matrix
    assert "prepare --engine ${{ matrix.engine }} --export-native" in matrix
    assert "--fetch --export-native" in (ROOT / '.github/workflows/migration-browser.yml').read_text(encoding='utf-8')
    assert "test_session_tree_live.py" in matrix
    assert "--strict-config" in matrix and "--timeout=120" in matrix
    assert "report --engine ${{ matrix.engine }}" in matrix
    assert './.github/actions/native-candidate-acceptance' in matrix
    candidate_action = (ROOT / '.github/actions/native-candidate-acceptance/action.yml').read_text(encoding='utf-8')
    assert '--test browser_adapter_live' in candidate_action and '--ignored' in candidate_action
    assert 'candidate_acceptance.py --report' in candidate_action and 'candidate_stage.py --release --report' in candidate_action
    assert 'benchmark_browser.py --report' in candidate_action and 'browser-performance.json' in candidate_action
    assert 'dashboard-dist' in candidate_action and 'ci-source.json' in candidate_action
    assert '--report-units' in candidate_action
    assert 'continue-on-error' not in candidate_action
    assert "continue-on-error" not in matrix
    assert "max-parallel: 2" in matrix and "timeout-minutes:" in matrix
    windows = jobs['engine-windows']
    assert 'timeout-minutes: 12' in windows
    assert 'Tee-Object -FilePath ../build/source-windows-output.log -Encoding utf8' in windows
    assert 'exit $LASTEXITCODE' in windows and 'build/source-windows-output.log' in windows
    assert 'python_source.py --shard ${{ matrix.shard }} --shards 4' in windows
    assert 'shard: [0, 1, 2, 3]' in windows and 'fail-fast: false' in windows
    assert 'max-parallel: 2' in windows
    assert 'if: matrix.shard != 1' in windows
    action = (ROOT / '.github/actions/source-module-checkpoints/action.yml').read_text(encoding='utf-8')
    assert 'uses: ./.github/actions/source-module-checkpoints' in windows
    for block in range(4):
        assert f'--shard 1 --shards 4 --block {block} --blocks 4' in (action if block == 2 else windows)
        assert f'name: source-windows-checkpoint-{block}' in windows
    assert windows.index('name: Reject missing suites') > windows.index('--block 3 --blocks 4')
    assert windows.count('if: matrix.shard == 1 && !cancelled()') == 8
    assert windows.count("steps.source_modules.outputs.settled == 'true'") == 2
    assert 'id: source_modules' in windows
    source = (ROOT / 'scripts/ci/python_source.py').read_text(encoding='utf-8')
    assert '--ignore-glob=tests/test_*_live.py' in source and '--junitxml=' in source
    for name in ("engine", "engine-macos"):
        assert "--ignore-glob=tests/test_*_live.py" in jobs[name]
    for name in ("engine", "engine-windows", "engine-macos"):
        assert "suites.py report --tier python" in jobs[name]
    for test in live.TREE_TESTS:
        assert "--deselect=tests/test_session_tree_live.py::" + test in jobs["browser"]
    assert "suites.py native --phase build" in jobs["rust-core"]
    assert "suites.py native --phase test" in jobs["rust-core"]
    required = jobs["required"]
    claimed = re.search(r"needs: \[([^\]]+)\]", required).group(1).split(", ")
    assert set(claimed) == set(jobs) - {"required", "promote", "publish"}
    assert 'needs: required' in jobs['promote']
    assert 'needs: [required, promote]' in jobs['publish']
    assert "github.ref == 'refs/heads/main'" in jobs['publish']
    assert "inputs.trusted_sha == github.sha" in jobs['publish']
    assert "if: always()" in required
    assert 'job["result"] != "success"' in required
    assert "DISTRIBUTION_SHA" in required and "EXPECTED_SHA" in required
    assert 'name == "codeql" and job["result"] == "skipped"' in required
    assert 'os.environ["PRIVATE_REPOSITORY"] == "true"' in required
    assert '--redact=100' in jobs['secrets'] and '--ignore-gitleaks-allow' in jobs['secrets']
    assert "uses: ./.github/workflows/build-desktop.yml" in jobs["distribution"]
    build = _jobs(ROOT / ".github/workflows/build-desktop.yml")
    assert "uses: ./.github/workflows/ci.yml" not in "".join(build.values()), "Recursive CI would duplicate verification"
    for job in ("desktop", "desktop-windows"):
        assert "cargo test --locked --no-default-features" in build[job]
        assert 'update_acceptance_build.py' in build[job] and 'update_acceptance.py run' in build[job]
        assert "continue-on-error" not in build[job]


PRODUCT_ARTIFACTS = ("numera-release-server", "numera-release-linux-x64",
                     "numera-release-macos-arm64", "numera-release-windows-x64")


def test_release_downloads_and_publishes_only_verified_manifest_products():
    jobs = _jobs(ROOT / ".github/workflows/build-desktop.yml")
    manifest = jobs["manifest"]
    names = re.findall(r"name: (numera-release-[\w-]+)\n\s+path:", manifest)
    assert sorted(names) == sorted(PRODUCT_ARTIFACTS)
    for job in jobs.values():
        for upload in re.findall(r"uses: actions/upload-artifact@.*?(?=\n      - |\Z)", job, re.S):
            assert "if-no-files-found: error" in upload
            assert "continue-on-error" not in upload
    release = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    downloads = re.findall(r"uses: actions/download-artifact@.*?(?=\n      - |\Z)", release, re.S)
    assert len(downloads) == len(PRODUCT_ARTIFACTS) + 1
    selected = []
    for download in downloads:
        name = re.search(r'(?m)^          name: ([^\n]+)', download).group(1)
        selected.append(name)
        assert "run-id: ${{ needs.gate.outputs.run_id }}" in download
        assert 'digest-mismatch: error' in download
    assert set(selected) == set(PRODUCT_ARTIFACTS) | {'distribution-manifest'}
    assert "Required verification" in release
    assert "run['head_sha'] == os.environ['SOURCE_SHA']" in release
    assert "Untrusted verification repository" in release
    assert "Unexpected publication payload" in release
    assert "Missing/duplicate/unknown product asset" in release
    assert "Asset checksum/size mismatch" in release
    assert "Published asset checksum differs" in release
    assert "--target \"$SOURCE_SHA\"" in release
    assert "--prerelease --latest=false" in release
    assert 'ref: ${{ inputs.source_sha }}' in release
    assert 'update-manifest.sig' in release and 'NUMERA_RELEASE_SIGNING_KEY' in release
    assert 'signing.py sign' in release and 'signing.py verify' in release
    browser = _jobs(ROOT / '.github/workflows/ci.yml')['browser']
    assert 'update_ui_smoke.py --dashboard dashboard/dist --output build/update-ui' in browser
    assert 'name: rendered-update-ui' in browser
    assert 'workflow_run:' not in release and 'workflow_call:' in release
    assert "continue-on-error" not in release


def _coverage():
    spec = importlib.util.spec_from_file_location('hosted_suites', ROOT / 'scripts/ci/suites.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('damage', [None, 'old-order', 'missing', 'duplicate'])
def test_independent_native_acceptance_precedes_the_required_benchmark(damage):
    coverage = _coverage()
    commands = ('native_http_acceptance.py', 'native_parent_acceptance.py',
                'native_console_acceptance.py', 'native_tty_acceptance.py',
                'native_job_acceptance.py', 'native_signal_acceptance.py', 'verify_default.py')
    steps = [{'run': 'python scripts/migration/' + name} for name in commands]
    performance = {'uses': './.github/actions/native-entry-performance'}
    steps.append(performance)
    if damage == 'old-order':
        steps.insert(0, steps.pop())
    elif damage == 'missing':
        steps.pop(0)
    elif damage == 'duplicate':
        steps.append(performance)
    if damage:
        with pytest.raises(ValueError, match='must'):
            coverage.check_native_acceptance_order(steps)
    else:
        coverage.check_native_acceptance_order(steps)


def test_native_development_optimizes_real_crypto_without_weakening_checks():
    manifest = (ROOT / 'engine-rust/Cargo.toml').read_text(encoding='utf-8')
    profiles = dict(re.findall(r'(?ms)^\[(profile\.[^\]]+)\]\s*\n(.*?)(?=^\[|\Z)', manifest))
    assert set(profiles) == {'profile.dev.package.numera-core', 'profile.dev.package."sha2:0.11.1"'}
    for settings in profiles.values():
        settings = re.sub(r'(?m)^\s*#.*$', '', settings).strip()
        assert settings == 'opt-level = 1', 'Development optimization must retain default safety checks'


def test_discovered_native_targets_are_all_selected_and_build_is_separate():
    coverage = _coverage()
    suites = coverage.discover()
    test = coverage.native_command('test')
    targets = {test[index + 1] for index, value in enumerate(test) if value == '--test'}
    assert targets == {path.stem for path in suites['native']}
    assert 'browser_adapter_live' not in targets
    assert '--lib' in test and '--no-fail-fast' in test and '--test-threads=2' in test
    build = coverage.native_command('build')
    assert '--no-run' in build and '--no-run' not in test
    format_option = '--message-format=json,json-render-diagnostics'

    def require_progress(command):
        boundary = command.index('--') if '--' in command else len(command)
        assert format_option in command and command.index(format_option) < boundary, \
            'Native compilation progress is missing before the test boundary'

    import hashlib

    historical = (ROOT / 'engine/tests/fixtures/native-command-before-progress.py.txt').read_bytes()
    assert hashlib.sha256(historical).hexdigest() == \
        'a3d38dc210a8348e6e7de703a24cf23a13f6bef8190339219fdf27b0f0f741bc'
    namespace = {'Path': Path, 'ROOT': ROOT, 'discover': coverage.discover}
    exec(compile(historical, 'native-command-before-progress.py.txt', 'exec'), namespace)
    for phase in ('build', 'test'):
        with pytest.raises(AssertionError, match='Native compilation progress is missing'):
            require_progress(namespace['native_command'](phase))
    require_progress(test)
    require_progress(build)
    assert '--' not in build, 'No-run compilation must not execute libtest'
    assert format_option not in coverage.seam_command('test', 'identity-script')
    assert coverage.check_workflows()['native'] == len(targets)


@pytest.mark.parametrize('damage', [None, 'fetch', 'proof'])
def test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test(monkeypatch, damage):
    import argparse
    import hashlib
    import subprocess
    import sys

    coverage = _coverage()
    events = []

    @contextmanager
    def logger(*args):
        yield SimpleNamespace(info=lambda *args: None)

    @contextmanager
    def reference():
        events.append('original')
        try:
            yield ROOT / 'original-fixture'
        finally:
            events.append('original-closed')

    def prove(root, original, log):
        assert root == ROOT and original == ROOT / 'original-fixture'
        events.append('proof')
        if damage == 'proof':
            raise ValueError('fixture proof failed')

    def run(command, **options):
        if command[1] == 'fetch':
            assert command == ['cargo', 'fetch', '--manifest-path',
                               str(ROOT / 'engine-rust/Cargo.toml'), '--locked']
            assert options == {'stdin': subprocess.DEVNULL, 'check': True}
            events.append('fetch')
            if damage == 'fetch':
                raise subprocess.CalledProcessError(1, command)
        else:
            assert command == coverage.native_command('build')
            assert '--no-run' in command
            events.append('fixed-build')
        return SimpleNamespace(returncode=0)

    def fixed_test(command, environment):
        if '--no-run' in command:
            assert command == coverage.native_command('build')
            events.append('fixed-build')
            return 0
        assert command == coverage.native_command('test')
        assert environment['NUMERA_ORACLE_SOURCE_ROOT'] == str(ROOT / 'original-fixture')
        assert environment['PYTHONDONTWRITEBYTECODE'] == '1'
        events.append('fixed-test')
        return 0

    monkeypatch.setattr(coverage, 'run_log', logger)
    monkeypatch.setattr(coverage, 'run_native', fixed_test)
    monkeypatch.setattr(subprocess, 'run', run)
    monkeypatch.setitem(sys.modules, 'baseline_source', SimpleNamespace(
        ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT', reference=reference))
    monkeypatch.setitem(sys.modules, 'native_regressions', SimpleNamespace(prove=prove))
    phase = 'history' if damage else 'build'
    monkeypatch.setattr(argparse.ArgumentParser, 'parse_args',
                        lambda _: SimpleNamespace(command='native', phase=phase))
    if damage:
        with pytest.raises((ValueError, subprocess.CalledProcessError)):
            coverage.main()
        assert events == (['fetch'] if damage == 'fetch' else
                          ['fetch', 'original', 'proof', 'original-closed'])
        return

    expected = ['fetch', 'original', 'proof', 'original-closed', 'fixed-build']
    phase = 'build'
    historical = (ROOT / 'engine/tests/fixtures/native-phase-before-separation.py.txt').read_bytes()
    assert hashlib.sha256(historical).hexdigest() == \
        '4732b4ca79120a3ae0bde753798e3490918d63b9f97c258c700ac218c56dc08c'
    namespace = {**coverage.main.__globals__}
    exec(compile(historical, 'native-phase-before-separation.py.txt', 'exec'), namespace)
    namespace['main']()
    with pytest.raises(AssertionError, match='Historical proofs must precede'):
        assert events == expected, 'Historical proofs must precede the fixed native build'
    events.clear()
    phase = 'history'
    assert coverage.main() == 0
    assert events == expected[:-1], 'Historical proofs must have their own bounded phase'
    events.clear()
    phase = 'build'
    assert coverage.main() == 0
    assert events == ['fixed-build'], 'Fixed compilation must not repeat historical proofs'
    events.clear()
    phase = 'test'
    assert coverage.main() == 0
    assert events == ['original', 'fixed-test', 'original-closed']
    import yaml

    workflow = yaml.safe_load((ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8'))
    job = workflow['jobs']['rust-core']
    steps = [step for step in job['steps'] if 'suites.py native --phase' in step.get('run', '')]
    phases = [command.strip() for step in steps for command in step['run'].split('&&')]
    assert [command.split('--phase ')[1] for command in phases] == ['history', 'build', 'test']
    assert all('run_owned.py --wall 900 --idle 300 --' in command for command in phases[:2])
    assert len(steps) == 2 and steps[0]['run'].count(' && ') == 1
    assert not any(step.get('if') or step.get('continue-on-error') for step in steps)
    assert job['timeout-minutes'] == 45


@pytest.mark.parametrize('exit_code', [0, 101])
@pytest.mark.parametrize('phase', ['build', 'test'])
def test_native_progress_projects_artifacts_without_exposing_cargo_payloads(monkeypatch, capsys, exit_code, phase):
    import io
    import json
    import subprocess

    coverage = _coverage()
    private = 'synthetic-build-environment-value'
    records = [
        {'reason': 'compiler-artifact', 'target': {'name': 'numera_core'}, 'fresh': False,
         'filenames': [private], 'manifest_path': private},
        {'reason': 'build-script-executed', 'env': [['BUILD_VALUE', private]]},
        {'reason': 'compiler-message', 'message': {'rendered': private}},
        {'reason': 'future-event', 'value': private},
        {'reason': 'compiler-artifact', 'target': {'name': private + '\n::error::'}, 'fresh': True},
        {'reason': 'compiler-artifact', 'target': None, 'fresh': False},
        {'reason': 'compiler-artifact', 'target': {'name': 'wrong_flag'}, 'fresh': 1},
        {'reason': 'compiler-artifact', 'target': {'name': 'x' * 129}, 'fresh': False},
        {'reason': 'compiler-artifact', 'target': {'name': 'cached-dependency'}, 'fresh': True},
        {'reason': 'build-finished', 'success': 'true'},
        {'reason': 'build-finished', 'success': exit_code == 0},
    ]
    output = '{malformed-build-record\n' + '\n'.join(json.dumps(record) for record in records) + '\n'
    output += '{"ordinary":"test-payload"}\nrunning 1 test\ntest contract ... ' + ('ok' if exit_code == 0 else 'FAILED') + '\n'
    calls = []
    stream = io.StringIO(output)

    @contextmanager
    def child(command, **kwargs):
        calls.append((command, kwargs))
        try:
            yield SimpleNamespace(stdout=stream, wait=lambda: exit_code)
        finally:
            stream.close()

    monkeypatch.setattr(coverage.subprocess, 'Popen', child)
    command = coverage.native_command(phase)
    environment = {'SYNTHETIC_SOURCE': 'source with spaces'}
    assert coverage.run_native(command, environment) == exit_code
    assert calls == [(command, {'env': environment, 'stdin': subprocess.DEVNULL,
                               'stdout': subprocess.PIPE, 'text': True,
                               'encoding': 'utf-8', 'errors': 'strict'})]
    captured = capsys.readouterr()
    assert captured.err == ''  # Cargo-rendered diagnostics inherit stderr, not a pipe.
    assert captured.out == (
        'Cargo artifact target=numera_core fresh=false\n'
        'Cargo artifact target=cached-dependency fresh=true\n'
        f'Cargo build finished success={str(exit_code == 0).lower()}\n'
        '{"ordinary":"test-payload"}\nrunning 1 test\ntest contract ... ' + ('ok' if exit_code == 0 else 'FAILED') + '\n')
    assert private not in captured.out and stream.closed


@pytest.mark.parametrize('damage', [None, 'missing', 'prerequisite-skip', 'failed', 'unexpected',
                                     'wholly-capability-skipped', 'xfail'])
def test_suite_report_rejects_omissions_false_green_and_missing_prerequisites(tmp_path, monkeypatch, damage):
    coverage = _coverage()
    monkeypatch.setattr(coverage, 'discover', lambda root: {'python': [Path('test_one.py'), Path('test_two.py')]})
    monkeypatch.setitem(coverage.CAPABILITY_REASONS, 'test_one', {'fixture capability unavailable'})
    suite = ET.Element('testsuite')
    for module in ('test_one', 'test_two'):
        ET.SubElement(suite, 'testcase', classname='tests.' + module, name='test_actual_path')
    if damage == 'missing':
        suite.remove(suite[1])
    elif damage == 'prerequisite-skip':
        ET.SubElement(suite[0], 'skipped', message='could not import fastapi')
    elif damage == 'failed':
        ET.SubElement(suite[0], 'failure')
    elif damage == 'unexpected':
        suite[0].set('classname', 'tests.unclaimed')
    elif damage in ('wholly-capability-skipped', 'xfail'):
        ET.SubElement(suite[0], 'skipped', message='fixture capability unavailable',
                      type='pytest.xfail' if damage == 'xfail' else 'pytest.skip')
    path = tmp_path / 'result.xml'
    ET.ElementTree(suite).write(path, encoding='utf-8', xml_declaration=True)
    if damage:
        with pytest.raises(ValueError):
            coverage.check_report(path, 'python')
    else:
        assert coverage.check_report(path, 'python') == {'passed': 2, 'explicit_limits': 0, 'suites': 2}


@pytest.mark.parametrize('platform,module,reason,accepted', [
    ('win32', 'test_posix_observation_errors', 'POSIX process-group observation', True),
    ('win32', 'test_shell_death_cleanup', 'Windows owns the engine through a job object', True),
    ('linux', 'test_posix_observation_errors', 'POSIX process-group observation', False),
    ('darwin', 'test_posix_observation_errors', 'requires Linux procfs', False),
    ('win32', 'test_parent_ownership', 'liveness is only answerable on POSIX', False),
])
def test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites(tmp_path, monkeypatch, platform, module, reason, accepted):
    coverage = _coverage()
    monkeypatch.setattr(coverage.sys, 'platform', platform)
    monkeypatch.setattr(coverage, 'discover', lambda root: {'python': [Path(module + '.py'), Path('test_control.py')]})
    suite = ET.Element('testsuite')
    case = ET.SubElement(suite, 'testcase', classname='tests.' + module, name='test_os_path')
    ET.SubElement(case, 'skipped', type='pytest.skip', message=reason)
    ET.SubElement(suite, 'testcase', classname='tests.test_control', name='test_passed')
    path = tmp_path / 'os-results.xml'
    ET.ElementTree(suite).write(path, encoding='utf-8', xml_declaration=True)
    if accepted:
        assert coverage.check_report(path, 'python')['explicit_limits'] == 1
    else:
        with pytest.raises(ValueError):
            coverage.check_report(path, 'python')


@pytest.mark.parametrize('attribute', ['#[ignore]', '#[ignore = "unmapped"]'])
def test_native_guard_rejects_ignored_library_cases(tmp_path, attribute):
    coverage = _coverage()
    library = tmp_path / 'library_tests.rs'
    library.write_text('#[test]\nfn required_case() {}\n', encoding='utf-8')
    coverage.check_native_ignored([library])
    library.write_text('#[test]\n' + attribute + '\nfn required_case() {}\n', encoding='utf-8')
    with pytest.raises(ValueError, match='unmapped ignored'):
        coverage.check_native_ignored([library])


def test_python_audit_excludes_only_local_distribution_and_keeps_every_dependency():
    coverage = _coverage()
    frozen = 'Numera_Engine @ file:///checkout/engine\nfastapi==0.143.1\npip-audit==2.10.1\ncryptography==50.0.2\n'
    assert coverage.third_party_requirements(frozen) == ['fastapi==0.143.1', 'pip-audit==2.10.1', 'cryptography==50.0.2']
    for invalid in ('fastapi==0.143.1\n', frozen + 'numera-engine==0.36.0\n',
                    frozen + 'private-lib @ file:///other\n', frozen + '-e /other\n',
                    frozen + 'unresolved>=1\n'):
        with pytest.raises(ValueError):
            coverage.third_party_requirements(invalid)


def test_utf8_gate_rejects_invalid_text_without_decoding_binary(tmp_path):
    coverage = _coverage()
    text = tmp_path / 'source.py'
    binary = tmp_path / 'image.png'
    text.write_text('value = "Ω"\n', encoding='utf-8')
    binary.write_bytes(b'\x89PNG\xff\x00')
    assert coverage.check_utf8([text, binary]) == 1
    text.write_bytes(b'value = "\xff"\n')
    with pytest.raises(ValueError, match='not UTF-8'):
        coverage.check_utf8([text, binary])


@pytest.mark.parametrize('damage', [None, 'missing', 'skipped'])
def test_dashboard_guard_requires_every_discovered_file_and_actual_pass(tmp_path, monkeypatch, damage):
    import json

    coverage = _coverage()
    files = [tmp_path / 'one.test.ts', tmp_path / 'two.test.tsx']
    monkeypatch.setattr(coverage, 'discover', lambda root: {'dashboard': files})
    suites = [{'name': str(path), 'status': 'passed',
               'assertionResults': [{'status': 'passed'}]} for path in files]
    report = {'success': True, 'numPassedTests': 2, 'numPendingTests': 0, 'testResults': suites}
    if damage == 'missing':
        suites.pop()
    elif damage == 'skipped':
        suites[0]['assertionResults'][0]['status'] = 'pending'
    path = tmp_path / 'dashboard-results.json'
    path.write_text(json.dumps(report), encoding='utf-8')
    if damage:
        with pytest.raises(ValueError):
            coverage.check_dashboard(path)
    else:
        assert coverage.check_dashboard(path) == {'tests': 2, 'suites': 2}


def test_secret_exceptions_are_only_exact_reviewed_rule_path_and_literal():
    import hashlib

    config = (ROOT / 'scripts/ci/gitleaks.toml').read_text(encoding='utf-8')
    assert 'useDefault = true' in config and 'disabledRules' not in config
    assert config.count('[[rules.allowlists]]') == 4
    assert config.count('condition = "AND"') == 4
    assert config.count('regexTarget = "secret"') == 4
    assert re.findall(r'(?m)^id = "([^"]+)"', config) == ['generic-api-key']
    blocks = config.split('[[rules.allowlists]]')[1:]
    patterns = [re.findall(r"'''(.*?)'''", re.search(r'(?m)^regexes = (.+)$', block).group(1))
                for block in blocks]
    paths = [re.search(r"paths = \['''(.+?)'''\]", block).group(1) for block in blocks]
    digests = (('f577f82af741d32085294020c093c7decd65544fe2f43c88bc76768245a5b409',),
               ('ac215b3e0151a72e3e3e5b898b61827612735ed8fdf7865d7a4c90959a242e03',
                '4c0b89ca29dd4fd13eeefc41a69fee0b64d8244e5fc8e2654ac31ad7c96fc426'),
               ('7e65f47df01dbc8677d2ced7c8ad02a06afbb21e07c1e872713d2bfaf8773128',),
               ('bf23e95aa0ff67388daa3728f36a013b9ddafdc2bd280decb03bacbaad9eb8a1',))
    assert len(patterns) == len(paths) == len(digests)
    for group, expected in zip(patterns, digests):
        assert len(group) == len(expected)
        for pattern, digest in zip(group, expected):
            assert pattern.startswith('^') and pattern.endswith('$')
            assert hashlib.sha256(pattern[1:-1].encode('utf-8')).hexdigest() == digest
            assert re.fullmatch(pattern, pattern[1:-1])
            assert re.fullmatch(pattern, pattern[1:-1] + 'changed') is None
    assert re.search(paths[0], 'engine-rust/tests/fixtures/request-auth.json')
    assert re.search(paths[0], 'engine-rust/tests/fixtures/token-store.json')
    assert re.search(paths[1], 'engine-rust/src/http/documentation-assets/provenance.json')
    assert re.search(paths[2], 'engine/tests/test_portable_root.py')
    assert not re.search(paths[0], 'engine-rust/tests/fixtures/unreviewed.json')
    assert not re.search(paths[1], 'engine-rust/src/http/documentation-assets/unreviewed.json')
    assert not re.search(paths[2], 'engine/tests/test_new_credential.py')
    assert re.search(paths[3], 'desktop/src-tauri/build.rs')
    assert not re.search(paths[3], 'desktop/src-tauri/src/unreviewed.rs')
    import base64
    pem = (ROOT / 'docs/release-signing-key.pem').read_text(encoding='utf-8')
    der = base64.b64decode(''.join(pem.splitlines()[1:-1]), validate=True)
    assert der[:12] == bytes.fromhex('302a300506032b6570032100')
    public = patterns[3][0][1:-1]
    assert der[12:].hex() == public
    assert f'pub const PUBLIC_KEY: &str = "{public}";' in (
        ROOT / 'desktop/src-tauri/src/updates/manifest.rs').read_text(encoding='utf-8')
    import json

    assets = ROOT / 'engine-rust/src/http/documentation-assets'
    provenance = json.loads((assets / 'provenance.json').read_text(encoding='utf-8'))
    reviewed = {pattern[1:-1] for pattern in patterns[1]}
    assert reviewed == {provenance['sha256'][name] for name in ('openapi.json', 'oauth2-redirect.html')}
    for name in ('openapi.json', 'oauth2-redirect.html'):
        assert hashlib.sha256((assets / name).read_bytes()).hexdigest() == provenance['sha256'][name]


@pytest.mark.parametrize('newline', ['\n', '\r\n'], ids=['lf', 'crlf'])
def test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions(tmp_path, monkeypatch, newline):
    import hashlib

    coverage = _coverage()
    base = tmp_path / 'official.toml'
    official = '''[allowlist]
description = "official values"
paths = [
    "package-lock.json",
    "asset[0-9].json",
]
regexes = ["^false$"]
[[rules]]
id = "required-rule"
regex = "required-detection"
path = "required-selector"
[[rules.allowlists]]
paths = ["hidden.txt"]
regexes = ["^official-constant$"]
[[rules.allowlists]]
paths = ["path-only.txt"]
[[rules]]
id = "another-required-rule"
regex = "another-detection"
'''
    payload = official.replace('\n', newline).encode('utf-8')
    base.write_bytes(payload)
    assert base.read_bytes() == payload, 'Checksum fixtures must preserve exact intended bytes'
    output = tmp_path / 'strict.toml'
    with pytest.raises(ValueError, match='checksum'):
        coverage.prepare_secret_config(base, output)
    monkeypatch.setattr(coverage, 'SCANNER_CONFIG_SHA256', hashlib.sha256(payload).hexdigest())
    coverage.prepare_secret_config(base, output)
    changed = base.read_text(encoding='utf-8')
    assert changed.count('paths = []') == 1
    assert 'package-lock.json' not in changed and 'hidden.txt' not in changed and 'path-only.txt' not in changed
    assert '[[rules.allowlists]]' not in changed and 'another-detection' in changed
    assert 'required-detection' in changed and 'path = "required-selector"' in changed
    assert 'regexes = ["^false$"]' in changed
    assert 'official-constant' not in changed, 'An AND path waiver must not turn into a global regex waiver'
    config = output.read_text(encoding='utf-8')
    assert 'useDefault' not in config and 'path = ' in config
    assert config.count('condition = "AND"') == 4
    if __import__('sys').version_info >= (3, 11):
        import tomllib

        parsed = tomllib.loads(changed)
        assert parsed['allowlist']['paths'] == []
        assert {rule['id'] for rule in parsed['rules']} == {'required-rule', 'another-required-rule'}
        strict = tomllib.loads(config)
        assert strict['extend']['path'] == str(base.resolve())
        assert len(strict['rules'][0]['allowlists']) == 4
