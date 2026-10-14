"""Published downloads must retain exact source and reject private/changed payloads."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tarfile

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts' / 'build'))


def packaging():
    spec = importlib.util.spec_from_file_location('distribution', ROOT / 'scripts/build/distribution.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('name', ['.ai/note.md', '.specify/config.json', 'specs/task.md',
                                  'secrets.md', 'docs/secrets.md', '.env', '.env.production',
                                  '.git/config', '.ignoreme/data', 'data/profiles.json',
                                  'node_modules/package/index.js', '../secret', '/absolute'])
def test_source_archive_rejects_private_or_generated_paths(name):
    with pytest.raises(ValueError, match='publication'):
        packaging().public_path(name)


@pytest.mark.parametrize('name', ['engine/numera/cli/__init__.py', 'engine/tests/fixtures/sample.json',
                                  'dashboard/package-lock.json', '.env.example',
                                  '.github/workflows/ci.yml', 'docs/licensing.md'])
def test_complete_source_keeps_build_files_and_safe_fixtures(name):
    packaging().public_path(name)


def test_missing_source_license_or_cloak_binary_refuses_download(tmp_path):
    module = packaging()
    stage = tmp_path / 'Numera Browser'
    stage.mkdir()
    with pytest.raises(ValueError, match='LICENSE'):
        module.validate_stage(stage, 'a' * 40)
    (stage / 'LICENSE').write_text('license', encoding='utf-8')
    with pytest.raises(ValueError, match='source'):
        module.validate_stage(stage, 'a' * 40)
    source = stage / 'sources'
    source.mkdir()
    (source / ('numera-source-' + 'a' * 40 + '.tar.gz')).write_bytes(b'source')
    browsers = stage / 'browsers'
    browsers.mkdir()
    (browsers / 'manifest.json').write_text(json.dumps({'cloak': {'executable': 'cloak/chrome'}}), encoding='utf-8')
    with pytest.raises(ValueError, match='Cloak'):
        module.validate_stage(stage, 'a' * 40)


@pytest.mark.parametrize('payload,expected', [
    (b'', 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'),
    (b'abc', 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'),
    (b'a' * 1000000, 'cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0'),
])
def test_asset_hashing_retains_python310_with_public_sha256_vectors(tmp_path, monkeypatch, payload, expected):
    import hashlib

    monkeypatch.delattr(hashlib, 'file_digest', raising=False)
    archive = tmp_path / 'numera-fixture.zip'
    archive.write_bytes(payload)
    assert packaging().asset_record(archive)['sha256'] == expected


def test_manifest_verification_rejects_tampered_or_unknown_assets(tmp_path):
    module = packaging()
    asset = tmp_path / 'numera-linux-x64.tar.gz'
    asset.write_bytes(b'original archive')
    record = module.asset_record(asset)
    manifest = {'schema': 1, 'source_sha': 'a' * 40, 'assets': [record]}
    module.verify_manifest(tmp_path, manifest, 'a' * 40)
    asset.write_bytes(b'changed archive')
    with pytest.raises(ValueError, match='checksum'):
        module.verify_manifest(tmp_path, manifest, 'a' * 40)
    asset.write_bytes(b'original archive')
    (tmp_path / 'private.log').write_text('diagnostic', encoding='utf-8')
    with pytest.raises(ValueError, match='unexpected'):
        module.verify_manifest(tmp_path, manifest, 'a' * 40)


def test_manifest_requires_exact_sha_and_unique_safe_names(tmp_path):
    module = packaging()
    asset = tmp_path / 'numera-linux-x64.tar.gz'
    asset.write_bytes(b'archive')
    record = module.asset_record(asset)
    manifest = {'schema': 1, 'source_sha': 'a' * 40, 'assets': [record]}
    with pytest.raises(ValueError, match='SHA'):
        module.verify_manifest(tmp_path, manifest, 'b' * 40)
    manifest['assets'].append(record)
    with pytest.raises(ValueError, match='duplicate'):
        module.verify_manifest(tmp_path, manifest, 'a' * 40)
    manifest['assets'] = [{**record, 'name': '../escape.tar.gz'}]
    with pytest.raises(ValueError, match='name'):
        module.verify_manifest(tmp_path, manifest, 'a' * 40)


def test_posix_archive_keeps_executable_permissions(tmp_path):
    module = packaging()
    stage = tmp_path / 'Numera Browser'
    stage.mkdir()
    executable = stage / 'numera'
    executable.write_bytes(b'not executed fixture')
    executable.chmod(0o755)
    archive = module.archive_stage(stage, tmp_path / 'portable.tar.gz')
    with tarfile.open(archive, 'r:gz') as bundle:
        assert bundle.getmember('Numera Browser/numera').mode & 0o111 == 0o111


@pytest.mark.parametrize('platform,header', [('linux-x64', b'wrong'),
                                            ('macos-arm64', b'wrong'),
                                            ('windows-x64', b'wrong')])
def test_mislabeled_shell_architecture_cannot_be_packaged(tmp_path, platform, header):
    shell = tmp_path / ('numera.exe' if platform == 'windows-x64' else 'numera')
    shell.write_bytes(header)
    with pytest.raises(ValueError):
        packaging().verify_architecture(tmp_path, platform)


def test_release_workflow_rejects_failed_fork_and_nonmain_runs():
    workflow = (ROOT / '.github/workflows/release.yml').read_text(encoding='utf-8')
    assert 'workflows: [CI]' in workflow
    assert 'types: [completed]' in workflow
    assert "conclusion == 'success'" in workflow
    assert "head_branch == 'main'" in workflow
    assert 'head_repository.full_name == github.repository' in workflow
    assert 'Required verification' in workflow
    assert '--prerelease' in workflow and '--latest=false' in workflow
    assert '--target "$SOURCE_SHA"' in workflow
    assert 'head_sha' in workflow and 'source_sha' in workflow
    assert 'continue-on-error' not in workflow
