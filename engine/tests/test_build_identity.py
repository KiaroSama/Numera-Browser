"""Build identity is embedded before signing and tied to the exact source."""
import ast
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def module():
    spec = importlib.util.spec_from_file_location('build_identity', ROOT / 'scripts/build/identity.py')
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


@pytest.fixture
def build(tmp_path, monkeypatch):
    identity = module()
    monkeypatch.setattr(identity, 'ROOT', tmp_path)
    for path in ('dashboard', 'engine', 'desktop/src-tauri'):
        (tmp_path / path).mkdir(parents=True, exist_ok=True)
    (tmp_path / 'dashboard/package.json').write_text('{"version":"0.36.0"}', encoding='utf-8')
    for path in ('engine/pyproject.toml', 'desktop/src-tauri/Cargo.toml'):
        (tmp_path / path).write_text('version = "0.36.0"\n', encoding='utf-8')
    (tmp_path / 'desktop/src-tauri/tauri.conf.json').write_text(json.dumps({
        'version': '0.36.0', 'productName': 'Numera Browser', 'bundle': {'publisher': 'KiaroSama'}}), encoding='utf-8')
    monkeypatch.setattr(identity.subprocess, 'check_output', lambda *args, **kwargs: 'a' * 40 + '\n')
    monkeypatch.setenv('GITHUB_ACTIONS', 'true')
    monkeypatch.setenv('GITHUB_SHA', 'a' * 40)
    monkeypatch.setenv('GITHUB_RUN_ID', '42')
    return identity


def test_generated_frozen_module_and_pe_resource_carry_same_exact_identity(build, tmp_path):
    output = tmp_path / 'generated'
    record = build.prepare(output)
    statement = ast.parse((output / 'numera_build.py').read_text(encoding='utf-8')).body[0]
    assert ast.literal_eval(statement.value) == {
        'schema': 1, 'owner': 'KiaroSama', 'product': 'Numera Browser', 'version': '0.36.0',
        'source_sha': 'a' * 40, 'generation': 42}
    assert record['source_sha'] == 'a' * 40
    resource = (output / 'windows-version.txt').read_text(encoding='utf-8')
    for value in ("StringStruct('CompanyName', 'KiaroSama')", "filevers=(0, 36, 0, 0)",
                  "StringStruct('PrivateBuild', '" + 'a' * 40 + "')"):
        assert value in resource
    ast.parse(resource)  # Never execute generated resource code in the test.


@pytest.mark.parametrize('name,value', [('GITHUB_SHA', 'b' * 40), ('GITHUB_RUN_ID', '0'),
                                       ('GITHUB_RUN_ID', '-1'), ('GITHUB_RUN_ID', '42\n')])
def test_identity_refuses_wrong_source_and_invalid_hosted_generation(build, monkeypatch, name, value):
    monkeypatch.setenv(name, value)
    with pytest.raises(ValueError):
        build.identity()


def test_verification_rejects_different_embedded_generation_without_signing_inputs(build, tmp_path, monkeypatch):
    expected = build.identity()
    suffix = '.exe' if build.os.name == 'nt' else ''
    for path in (tmp_path / ('numera' + suffix), tmp_path / ('engine/numera-engine' + suffix)):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b'not executable: the external process boundary is mocked')
    calls = []
    for name in ('NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX', 'NUMERA_CODE_SIGNING_PASSWORD'):
        monkeypatch.setenv(name, 'private fixture material')

    def command(argv, **options):
        assert argv[1:] == ['--build-info']
        assert not any(name.startswith('NUMERA_CODE_SIGNING_') or name == 'NUMERA_RELEASE_SIGNING_KEY'
                       for name in options['env'])
        calls.append(argv[0])
        actual = expected if len(calls) == 1 else {**expected, 'generation': 41}
        return SimpleNamespace(returncode=0, stdout=json.dumps(actual).encode('utf-8'), stderr=b'')

    monkeypatch.setattr(build.subprocess, 'run', command)
    with pytest.raises(ValueError, match='Embedded executable identity'):
        build.verify(tmp_path)
    assert len(calls) == 2


def test_fixture_generation_is_embedded_only_when_explicitly_requested(build, tmp_path):
    record = build.prepare(tmp_path / 'fixture', acceptance_generation=85)
    assert record['generation'] == 85 and record['acceptance'] is True
    assert 'acceptance' not in build.identity() and build.identity()['generation'] == 42
    with pytest.raises(ValueError, match='hosted fixture generation'):
        build.prepare(tmp_path / 'invalid', acceptance_generation=86)


def test_publication_refuses_a_fixture_compiled_executable(build, tmp_path, monkeypatch):
    expected = build.identity()
    suffix = '.exe' if build.os.name == 'nt' else ''
    for path in (tmp_path / ('numera' + suffix), tmp_path / ('engine/numera-engine' + suffix)):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b'fixture boundary only')
    monkeypatch.setattr(build.subprocess, 'run', lambda *args, **kwargs: SimpleNamespace(
        returncode=0, stdout=json.dumps({**expected, 'acceptance': True}).encode('utf-8'), stderr=b''))
    with pytest.raises(ValueError, match='Embedded executable identity'):
        build.verify(tmp_path)


def test_both_freezers_embed_identity_and_windows_uses_approved_icon():
    windows = (ROOT / 'scripts/build-desktop.ps1').read_text(encoding='utf-8')
    workflow = (ROOT / '.github/workflows/build-desktop.yml').read_text(encoding='utf-8')
    assert '--version-file' in windows and '--icon' in windows
    assert '--paths $identity' in windows and '--paths build/identity' in workflow
    assert workflow.count('identity.py verify --stage') == 2
    assert 'from numera_build import IDENTITY' in (ROOT / 'scripts/engine_entry.py').read_text(encoding='utf-8')
    assert workflow.count('update_acceptance_build.py --stage') == 2
    assert workflow.count('update_acceptance.py run --old-stage') == 3
    assert 'cargo test --locked --no-default-features' in workflow
    assert '--all-features' not in workflow
    assert 'verify_identity(stage)' in (ROOT / 'scripts/build/distribution.py').read_text(encoding='utf-8')
