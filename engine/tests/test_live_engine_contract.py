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
    assert "prepare --engine ${{ matrix.engine }}" in matrix
    assert "test_session_tree_live.py" in matrix
    assert "--strict-config" in matrix and "--timeout=120" in matrix
    assert "report --engine ${{ matrix.engine }}" in matrix
    assert "--test browser_adapter_live" in matrix and "--ignored" in matrix
    assert "continue-on-error" not in matrix
    assert "max-parallel: 2" in matrix and "timeout-minutes:" in matrix
    for name in ("engine", "engine-windows", "engine-macos"):
        assert "--ignore-glob=tests/test_*_live.py" in jobs[name]
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


def test_discovered_native_targets_are_all_selected_and_build_is_separate():
    coverage = _coverage()
    suites = coverage.discover()
    test = coverage.native_command('test')
    targets = {test[index + 1] for index, value in enumerate(test) if value == '--test'}
    assert targets == {path.stem for path in suites['native']}
    assert 'browser_adapter_live' not in targets
    assert '--lib' in test and '--no-fail-fast' in test and '--test-threads=2' in test
    assert '--no-run' in coverage.native_command('build') and '--no-run' not in test
    assert coverage.check_workflows()['native'] == len(targets)


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


def test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions(tmp_path, monkeypatch):
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
    base.write_text(official, encoding='utf-8')
    output = tmp_path / 'strict.toml'
    with pytest.raises(ValueError, match='checksum'):
        coverage.prepare_secret_config(base, output)
    monkeypatch.setattr(coverage, 'SCANNER_CONFIG_SHA256', hashlib.sha256(official.encode('utf-8')).hexdigest())
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
