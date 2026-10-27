"""Published downloads must retain exact source and reject private/changed payloads."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tarfile

import pytest
import yaml

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
], ids=['empty', 'abc', 'million-a'])
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


@pytest.mark.parametrize('extension', ['.tar.gz', '.zip'])
def test_archive_excludes_private_update_runtime_state(tmp_path, extension):
    import zipfile
    stage = tmp_path / 'Numera Browser'
    stage.mkdir()
    (stage / 'numera').write_bytes(b'not executed')
    (stage / '.updates').mkdir()
    (stage / '.updates/activation.json').write_text('{"fixture":"private"}', encoding='utf-8')
    (stage / '.numera-update.lock').touch()
    (stage / '.numera-update-generation.json').write_text('{"generation":42}', encoding='utf-8')
    retained = stage / ('.numera-update-' + 'a' * 64)
    retained.mkdir()
    (retained / 'journal.json').write_text('{"ticket":"private fixture"}', encoding='utf-8')
    archive = packaging().archive_stage(stage, tmp_path / ('portable' + extension))
    if extension == '.zip':
        with zipfile.ZipFile(archive) as packed:
            names = packed.namelist()
    else:
        with tarfile.open(archive, 'r:gz') as packed:
            names = packed.getnames()
    assert 'Numera Browser/numera' in names
    assert not any('.updates' in name or '.numera-update' in name for name in names)


def fixture_mode(path, mode, monkeypatch):
    import os
    import stat

    path.chmod(mode)
    if os.name == 'nt':
        # Windows chmod cannot provide POSIX execute bits. Substitute only
        # this file's OS metadata; archive and manifest bytes remain real.
        original_stat, original_lstat = Path.stat, os.lstat

        def adjusted(result):
            values = list(result)
            values[0] = stat.S_IFMT(result.st_mode) | mode
            # The tuple omits fractional times and Windows named-only fields.
            return os.stat_result(values, result.__reduce__()[1][1])

        def file_stat(candidate, *args, **kwargs):
            result = original_stat(candidate, *args, **kwargs)
            return adjusted(result) if candidate == path else result

        def file_lstat(candidate, *args, **kwargs):
            result = original_lstat(candidate, *args, **kwargs)
            return adjusted(result) if Path(candidate) == path else result

        monkeypatch.setattr(Path, 'stat', file_stat)
        monkeypatch.setattr(os, 'lstat', file_lstat)


@pytest.mark.parametrize('mode', [0o755, 0o644], ids=['executable', 'data'])
def test_posix_archive_keeps_executable_permissions(tmp_path, monkeypatch, mode):
    import os

    module = packaging()
    stage = tmp_path / 'Numera Browser'
    stage.mkdir()
    executable = stage / 'numera'
    executable.write_bytes(b'not executed fixture')
    os.utime(executable, ns=(1_700_000_000_123_456_700,) * 2)
    original = executable.stat()
    fixture_mode(executable, mode, monkeypatch)
    observed = executable.stat()
    assert observed.st_mtime == original.st_mtime
    assert observed.st_mtime_ns == original.st_mtime_ns
    archive = module.archive_stage(stage, tmp_path / 'portable.tar.gz')
    with tarfile.open(archive, 'r:gz') as bundle:
        member = bundle.getmember('Numera Browser/numera')
        assert member.mode & 0o777 == mode
        assert member.mtime == original.st_mtime
        with bundle.extractfile(member) as stream:
            assert stream.read() == b'not executed fixture'


@pytest.mark.parametrize('platform,header', [('linux-x64', b'wrong'),
                                            ('macos-arm64', b'wrong'),
                                            ('windows-x64', b'wrong')])
def test_mislabeled_shell_architecture_cannot_be_packaged(tmp_path, platform, header):
    shell = tmp_path / ('numera.exe' if platform == 'windows-x64' else 'numera')
    shell.write_bytes(header)
    with pytest.raises(ValueError):
        packaging().verify_architecture(tmp_path, platform)


def test_release_workflow_rejects_failed_forks_and_unapproved_source_refs():
    workflow = (ROOT / '.github/workflows/release.yml').read_text(encoding='utf-8')
    assert 'workflow_call:' in workflow and 'workflow_run:' not in workflow
    assert "github.event_name == 'workflow_dispatch'" in workflow
    assert "github.ref == 'refs/heads/main'" in workflow
    assert "inputs.source_sha == github.sha" in workflow
    assert "run['head_repository']['full_name'] == os.environ['REPOSITORY']" in workflow
    assert 'Required verification' in workflow
    assert '--prerelease' in workflow and '--latest=false' in workflow
    assert '--target "$SOURCE_SHA"' in workflow
    assert 'head_sha' in workflow and 'source_sha' in workflow
    assert 'continue-on-error' not in workflow
    assert 'signing.py sign' in workflow and 'signing.py verify' in workflow


def signing():
    spec = importlib.util.spec_from_file_location('release_signing', ROOT / 'scripts/build/signing.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_ed25519_exact_manifest_rejects_tampering_wrong_key_and_size(tmp_path, monkeypatch):
    module = signing()
    monkeypatch.setenv('RUNNER_TEMP', str(tmp_path))
    monkeypatch.setenv('NUMERA_RELEASE_SIGNING_KEY', '')
    monkeypatch.setenv('NUMERA_SIGNING_SCOPE', 'validation-only')
    public = tmp_path / 'fixture-public.pem'
    module.fixture_ed25519(public)
    manifest = tmp_path / 'update-manifest.json'
    manifest.write_text('{"schema":1,"generation":42}\n', encoding='utf-8')
    signature = tmp_path / 'update-manifest.sig'
    def forbidden_transport(*args, **kwargs):
        pytest.fail('Manifest signing must not stage private keys or start signing subprocesses')
    monkeypatch.setattr(module, 'private_directory', forbidden_transport)
    monkeypatch.setattr(module, 'execute', forbidden_transport)
    module.sign_manifest(manifest, signature, public)
    proof = signature.read_bytes()
    assert len(proof) == 64
    module.verify_manifest(manifest, signature, public)
    module.public_ed25519(public).verify(proof, manifest.read_bytes())
    with pytest.raises(ValueError, match='destination new'):
        module.sign_manifest(manifest, signature, public)
    assert signature.read_bytes() == proof
    original = manifest.read_bytes()
    manifest.write_bytes(original.replace(b'42', b'43'))
    with pytest.raises(ValueError, match='signature'):
        module.verify_manifest(manifest, signature, public)
    manifest.write_bytes(original)
    wrong = tmp_path / 'wrong-public.pem'
    module.fixture_ed25519(wrong)
    refused = tmp_path / 'wrong-signer.sig'
    with pytest.raises(ValueError, match='pinned public signer'):
        module.sign_manifest(manifest, refused, public)
    assert not refused.exists() and signature.read_bytes() == proof
    with pytest.raises(ValueError, match='signature'):
        module.verify_manifest(manifest, signature, wrong)
    signature.write_bytes(b'x' * 63)
    with pytest.raises(ValueError, match='size'):
        module.verify_manifest(manifest, signature, public)
    assert not list(tmp_path.glob('numera-sign-*')), 'Private temporary signing inputs survived'


def test_macos_native_signing_checks_identity_and_discriminates_rejection(tmp_path, monkeypatch):
    import base64
    import struct
    import subprocess

    module = signing()
    stage = tmp_path / 'portable path with spaces'
    (stage / 'engine').mkdir(parents=True)
    paths = [stage / 'numera', stage / 'engine/numera-engine']
    # Thin ARM64 layout with one signature command; data is never executed.
    content = bytearray(192)
    struct.pack_into('<8I', content, 0, 0xFEEDFACF, 0x0100000C, 0, 2, 1, 16, 0, 0)
    struct.pack_into('<4I', content, 32, 0x1D, 16, 128, 64)
    for path in paths:
        path.write_bytes(content)
    public = tmp_path / 'fixture.pem'
    material = tmp_path / 'fixture.p12'
    password = b'public synthetic fixture password'
    module.fixture_certificate(public, material, password)
    _, fingerprint, sha1 = module.certificate(public)
    approved = f'=certificate leaf = H"{sha1}"'
    wrong_sha1 = ('0' if sha1[0] != '0' else '1') + sha1[1:]
    wrong = f'=certificate leaf = H"{wrong_sha1}"'
    monkeypatch.setenv('GITHUB_ACTIONS', 'false')
    monkeypatch.setenv('RUNNER_TEMP', str(tmp_path / 'private staging'))
    monkeypatch.setenv('NUMERA_CODE_SIGNING_PFX', base64.b64encode(material.read_bytes()).decode('ascii'))
    monkeypatch.setenv('NUMERA_CODE_SIGNING_PASSWORD', password.decode('ascii'))
    monkeypatch.setenv('NUMERA_RELEASE_SIGNING_KEY', 'public fixture, not a key')
    result_path = stage / 'SIGNING.json'
    original_run = module.subprocess.run
    modes = {'specific-mismatch': 3, 'accepted-wrong-signer': 0,
             'invalid-arguments': 2, 'other-verification-failure': 1,
             'wrong-signer-timeout': 3, 'wrong-signer-missing-tool': 3,
             'accepted-tamper': 3, 'tamper-timeout': 3, 'tamper-tool-error': 3,
             'tamper-invalid-arguments': 3, 'tamper-missing-tool': 3}
    for mode, mismatch_exit in modes.items():
        result_path.unlink(missing_ok=True)
        calls = []

        def native_tool(command, **kwargs):
            if command[0] in {'whoami', 'icacls'}:
                return original_run(command, **kwargs)
            calls.append(command)
            assert not any(name in kwargs['env'] for name in (
                'NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX', 'NUMERA_CODE_SIGNING_PASSWORD'))
            assert kwargs['timeout'] == 60
            code = 0
            diagnostic = b'private fixture tool detail'
            if command[0] == 'codesign' and '--verify' in command:
                assert '--strict' in command and '--test-requirement' in command
                requirement = command[command.index('--test-requirement') + 1]
                assert requirement in {approved, wrong}
                path = Path(command[-1])
                if requirement == wrong:
                    assert path in paths, 'Wrong-signer control must use the intact owned binary'
                    if mode == 'wrong-signer-timeout':
                        raise subprocess.TimeoutExpired(command, 60, stderr=diagnostic)
                    if mode == 'wrong-signer-missing-tool':
                        raise FileNotFoundError('native fixture tool missing')
                    code = mismatch_exit
                elif path.name.startswith('tampered-'):
                    damaged = bytearray(content)
                    damaged[127] ^= 1
                    assert path.read_bytes() == damaged, 'Tamper control changed the object layout'
                    assert kwargs['env']['LC_ALL'] == 'C'
                    if mode == 'tamper-timeout':
                        raise subprocess.TimeoutExpired(command, 60, stderr=diagnostic)
                    if mode == 'tamper-missing-tool':
                        raise FileNotFoundError('native fixture tool missing')
                    code = 0 if mode == 'accepted-tamper' else 2 if mode == 'tamper-invalid-arguments' else 1
                    if mode != 'tamper-tool-error':
                        diagnostic += b'\nfixture: code or signature modified\n'
                else:
                    assert path in paths
            else:
                assert command[0] == 'security' or (command[0] == 'codesign' and '--sign' in command)
            return subprocess.CompletedProcess(command, code, b'private fixture stdout', diagnostic)

        monkeypatch.setattr(module.subprocess, 'run', native_tool)
        if mode == 'specific-mismatch':
            module.native_sign(stage, 'macos-arm64', public)
            result = json.loads(result_path.read_text(encoding='utf-8'))
            assert result['certificate_sha256'] == fingerprint
            assert result['trust'] == 'owner-self-signed-private-trust'
            assert not result['notarized'] and not result['publicly_trusted']
            for path in paths:
                verification = [command for command in calls
                                if command[0] == 'codesign' and '--verify' in command
                                and command[-1] == str(path)]
                assert [command[command.index('--test-requirement') + 1]
                        for command in verification] == [approved, wrong, approved, approved]
            assert sum(command[0] == 'codesign' and Path(command[-1]).name.startswith('tampered-')
                       for command in calls) == 2
        else:
            expected_type = FileNotFoundError if mode.endswith('missing-tool') else ValueError
            with pytest.raises(expected_type) as caught:
                module.native_sign(stage, 'macos-arm64', public)
            assert 'private fixture' not in str(caught.value)
            assert not result_path.exists(), 'Failed native verification published signing metadata'
            if mode.endswith('timeout'):
                assert 'exceeded its deadline' in str(caught.value)
            elif not mode.endswith('missing-tool'):
                expected_exit = (0 if mode == 'accepted-tamper' else
                                 2 if mode == 'tamper-invalid-arguments' else
                                 1 if mode == 'tamper-tool-error' else mismatch_exit)
                assert f'exit={expected_exit}' in str(caught.value)
        assert any(command[:2] == ['security', 'delete-keychain'] for command in calls)
        assert not list((tmp_path / 'private staging').glob('numera-sign-*'))
        assert all(path.read_bytes() == content for path in paths)


@pytest.mark.parametrize('mode,executable', [(0o755, True), (0o644, False)], ids=['executable', 'data'])
def test_immutable_build_inventory_excludes_only_its_own_signature_cycle(tmp_path, monkeypatch, mode, executable):
    module = signing()
    stage = tmp_path / 'Numera Browser'
    stage.mkdir()
    (stage / 'numera').write_bytes(b'fixture executable, never run')
    fixture_mode(stage / 'numera', mode, monkeypatch)
    (stage / 'LICENSE').write_text('fixture terms', encoding='utf-8')
    (stage / '.updates').mkdir()
    (stage / '.updates/runtime.lock').write_bytes(b'')
    (stage / '.updates/activation.json').write_text('{"private":"fixture"}', encoding='utf-8')
    monkeypatch.setenv('GITHUB_RUN_ID', '42')
    monkeypatch.setenv('NUMERA_SIGNING_SCOPE', 'validation-only')
    module.installed_manifest(stage, 'linux-x64', 'a' * 40)
    build = json.loads((stage / 'BUILD.json').read_text(encoding='utf-8'))
    assert set(build) == {'schema','owner','product','version','source_sha','generation',
                          'platform','minimum_protocol','components','files'}
    assert build['owner'] == 'KiaroSama' and build['generation'] == 42
    rows = {row['path']: row for row in build['files']}
    assert set(rows) == {'numera','LICENSE'}
    assert rows['numera']['sha256'] == packaging().digest(stage/'numera')
    assert rows['numera']['executable'] is executable
    (stage/'data').mkdir()
    (stage/'data/profile.json').write_text('{}', encoding='utf-8')
    with pytest.raises(ValueError, match='publication'):
        module.installed_manifest(stage, 'linux-x64', 'a' * 40)


def test_publisher_reads_owner_signed_build_from_actual_archive(tmp_path, monkeypatch):
    import hashlib
    import zipfile

    module = signing()
    monkeypatch.setenv('RUNNER_TEMP', str(tmp_path))
    monkeypatch.setenv('NUMERA_RELEASE_SIGNING_KEY', '')
    monkeypatch.setenv('NUMERA_SIGNING_SCOPE', 'validation-only')
    docs = tmp_path/'docs'
    docs.mkdir()
    public = docs/'release-signing-key.pem'
    module.fixture_ed25519(public)
    monkeypatch.setattr(module, 'ROOT', tmp_path)
    monkeypatch.setenv('GITHUB_ACTIONS', 'false')
    manifest = tmp_path/'BUILD.json'
    build = {'generation': 42, 'source_sha': 'a'*40, 'platform': 'windows-x64', 'components': {'sdk':{},'browsers':{}}}
    manifest.write_text(json.dumps(build), encoding='utf-8')
    signature = tmp_path/'BUILD.sig'
    module.sign_manifest(manifest, signature, public)
    archive = tmp_path/'numera-windows.zip'
    with zipfile.ZipFile(archive, 'w') as packed:
        packed.write(manifest, 'Numera Browser/BUILD.json')
        packed.write(signature, 'Numera Browser/BUILD.sig')
    metadata = {'build_manifest_sha256': hashlib.sha256(manifest.read_bytes()).hexdigest(),
                'components': build['components'], 'max_members': 2,
                'expanded_size': manifest.stat().st_size + signature.stat().st_size}
    module.validate_archive_build(archive, metadata, 'windows-x64', 42, 'a'*40)
    with pytest.raises(ValueError, match='generation'):
        module.validate_archive_build(archive, metadata, 'windows-x64', 43, 'a'*40)
    metadata['build_manifest_sha256'] = '0'*64
    with pytest.raises(ValueError, match='hash/signature'):
        module.validate_archive_build(archive, metadata, 'windows-x64', 42, 'a'*40)


@pytest.mark.parametrize('damage', [None, 'scanner-only', 'wrong-sha', 'failed', 'pending', 'newer-failure'])
def test_signing_requires_the_independent_exact_source_codeql_verdict(damage):
    spec = importlib.util.spec_from_file_location('codeql_gate', ROOT / 'scripts/ci/codeql_gate.py')
    gate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gate)
    check = {'id': 42, 'name': 'CodeQL', 'app': {'slug': 'github-advanced-security'},
             'head_sha': 'a' * 40, 'status': 'completed', 'conclusion': 'success'}
    checks = [check]
    if damage == 'scanner-only':
        check.update(name='codeql / CodeQL (rust)', app={'slug': 'github-actions'})
    elif damage == 'wrong-sha':
        check['head_sha'] = 'b' * 40
    elif damage == 'failed':
        check['conclusion'] = 'failure'
    elif damage == 'pending':
        check['status'] = 'in_progress'
    elif damage == 'newer-failure':
        checks.append({**check, 'id': 43, 'conclusion': 'failure'})
    if damage is None:
        assert gate.verify([{'check_runs': checks}], 'a' * 40) == 42
    else:
        with pytest.raises(ValueError):
            gate.verify([{'check_runs': checks}], 'a' * 40)


def test_owner_signing_context_rejects_arbitrary_ref_actor_event_and_sha(monkeypatch):
    module = signing()
    values = {'GITHUB_ACTIONS':'true','GITHUB_EVENT_NAME':'workflow_dispatch','GITHUB_ACTOR':'KiaroSama',
              'GITHUB_REF':'refs/heads/main','GITHUB_SHA':'a'*40,'NUMERA_TRUSTED_SHA':'a'*40}
    for key,value in values.items():
        monkeypatch.setenv(key,value)
    module.require_owner_context()
    for key,value in [('GITHUB_REF','refs/heads/unreviewed'),('GITHUB_ACTOR','other'),
                      ('GITHUB_EVENT_NAME','pull_request'),('NUMERA_TRUSTED_SHA','b'*40)]:
        monkeypatch.setenv(key,value)
        with pytest.raises(ValueError,match='approved manual'):
            module.require_owner_context()
        monkeypatch.setenv(key,values[key])


def test_private_signing_tool_failure_never_displays_captured_secrets(monkeypatch):
    module = signing()
    from types import SimpleNamespace

    monkeypatch.setenv('NUMERA_RELEASE_SIGNING_KEY', 'private fixture material')
    monkeypatch.setenv('NUMERA_CODE_SIGNING_PFX', 'private fixture material')
    monkeypatch.setenv('NUMERA_CODE_SIGNING_PASSWORD', 'private fixture material')
    def failed(*args, **kwargs):
        assert not any(name in kwargs['env'] for name in
                       ('NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX', 'NUMERA_CODE_SIGNING_PASSWORD'))
        return SimpleNamespace(returncode=1, stdout=b'private fixture material', stderr=b'private fixture material')
    monkeypatch.setattr(module.subprocess, 'run', failed)
    with pytest.raises(ValueError) as failure:
        module.execute(['openssl', 'pkeyutl'])
    assert 'private fixture material' not in str(failure.value)


@pytest.mark.parametrize('damage', [None, 'protected', 'rules', 'ruleset', 'unknown-rules', 'diverged',
                                   'wrong-tip', 'failed-required', 'different-sha', 'other-ref', 'other-actor'])
def test_embedded_promotion_gate_refuses_unverified_or_unsafe_state(tmp_path, monkeypatch, damage):
    import textwrap
    ci = (ROOT/'.github/workflows/ci.yml').read_text(encoding='utf-8')
    promoter = ci.split('\n  promote:', 1)[1].split('\n  publish:', 1)[0]
    script = textwrap.dedent(promoter.split("python3 -I - <<'PY'\n", 1)[1].split('\n          PY', 1)[0])
    sha, base = 'a'*40, 'b'*40
    documents = {
        'branch': {'name':'main','protected':False,'commit':{'sha':base}},
        'rules':[[]], 'rulesets':[[]],
        'run': {'id':42,'head_sha':sha,'name':'CI','path':'.github/workflows/ci.yml',
                'event':'workflow_dispatch','actor':{'login':'KiaroSama'},
                'head_repository':{'full_name':'KiaroSama/Numera-Browser'},
                'head_branch':'migration/native-runtime-checkpoint-20261012'},
        'jobs':[{'jobs':[{'name':'Required verification','conclusion':'success'}]}],
        'comparison':{'base_commit':{'sha':base},'status':'ahead','commits':[{'sha':sha}]},
    }
    if damage == 'protected': documents['branch']['protected'] = True
    elif damage == 'rules': documents['rules'] = [[{'type':'pull_request'}]]
    elif damage == 'ruleset': documents['rulesets'] = [[{'name':'policy'}]]
    elif damage == 'unknown-rules': documents['rules'] = [{}]
    elif damage == 'diverged': documents['comparison']['status'] = 'diverged'
    elif damage == 'wrong-tip': documents['comparison']['commits'][-1]['sha'] = base
    elif damage == 'failed-required': documents['jobs'][0]['jobs'][0]['conclusion'] = 'failure'
    elif damage == 'different-sha': documents['run']['head_sha'] = base
    elif damage == 'other-ref': documents['run']['head_branch'] = 'unapproved'
    elif damage == 'other-actor': documents['run']['actor']['login'] = 'other'
    for name, value in documents.items():
        (tmp_path/(name+'.json')).write_text(json.dumps(value), encoding='utf-8')
    output = tmp_path/'output.txt'
    for name, value in {'EXPECTED_SHA':sha,'TRUSTED_SHA':sha,'GITHUB_RUN_ID':'42',
                        'REPOSITORY':'KiaroSama/Numera-Browser','GITHUB_OUTPUT':str(output),
                        'GITHUB_REF_NAME':'migration/native-runtime-checkpoint-20261012'}.items():
        monkeypatch.setenv(name, value)
    monkeypatch.chdir(tmp_path)
    if damage:
        with pytest.raises(AssertionError):
            exec(compile(script, 'promotion-gate-fixture', 'exec'), {})
        assert not output.exists()
    else:
        exec(compile(script, 'promotion-gate-fixture', 'exec'), {})
        assert output.read_text(encoding='utf-8') == f'base_sha={base}\n'


@pytest.mark.parametrize('timeout', [False, True])
def test_package_smoke_pipes_fixture_input_without_plaintext_staging(tmp_path, monkeypatch, timeout):
    from types import SimpleNamespace
    import subprocess

    spec = importlib.util.spec_from_file_location('package_smoke', ROOT / 'scripts/build/smoke.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    received, settled = [], []
    logger = SimpleNamespace(info=lambda *args: None)
    smoke = module.Smoke(tmp_path / 'not-executed', tmp_path, 60, logger)

    should_timeout = timeout

    def communicate(input, timeout):
        received.append(input)
        assert 0 < timeout <= 60
        if should_timeout:
            raise subprocess.TimeoutExpired('fixture', timeout)

    def spawn(command, **options):
        assert command[3:] == ['pool', 'add', '-', '--json']
        assert options['stdin'] is subprocess.PIPE
        options['stdout'].write(b'{"ok":true}')
        return SimpleNamespace(pid=42, returncode=0, communicate=communicate,
                               stdin=SimpleNamespace(close=lambda: None)), object()

    monkeypatch.setattr(module.proctree, 'spawn', spawn)
    monkeypatch.setattr(module.proctree, 'end_session', lambda *args, **kwargs: settled.append(args[0].pid))
    monkeypatch.setattr(module.unit_report, 'read', lambda *args: SimpleNamespace(trees=[], complete=True))
    if timeout:
        with pytest.raises(subprocess.TimeoutExpired):
            smoke.command('pool', 'add', '-', input_text='public fixture α\n')
    else:
        assert smoke.command('pool', 'add', '-', input_text='public fixture α\n') == {'ok': True}
    assert received == [b'public fixture \xce\xb1\n']
    assert settled == [42] and not smoke.owned
    assert not (tmp_path / 'fixture-proxies.txt').exists()


def test_manual_ci_secrets_are_named_and_publish_requires_verified_promotion():
    ci = (ROOT/'.github/workflows/ci.yml').read_text(encoding='utf-8')
    triggers = ci.split('\non:\n', 1)[1].split('\n# Read-only', 1)[0]
    assert set(__import__('re').findall(r'^  ([a-z_]+):', triggers, __import__('re').M)) == {'workflow_dispatch'}
    assert 'secrets: inherit' not in ci
    assert "inputs.trusted_sha == github.sha" in ci
    assert "github.actor == 'KiaroSama'" in ci
    promoter = ci.split('\n  promote:', 1)[1].split('\n  publish:', 1)[0]
    assert 'needs: required' in promoter and 'contents: write' in promoter and 'actions: read' in promoter
    assert "inputs.publish" in promoter and "inputs.trusted_sha == github.sha" in promoter
    assert "github.actor == 'KiaroSama'" in promoter and "github.event_name == 'workflow_dispatch'" in promoter
    assert "github.ref == 'refs/heads/main'" in promoter
    assert "github.ref == 'refs/heads/migration/native-runtime-checkpoint-20261012'" in promoter
    assert 'rules/branches/main' in promoter and 'rulesets?includes_parents=true' in promoter
    assert "branch['protected'] is False" in promoter
    assert "comparison['status'] in ('ahead', 'identical')" in promoter
    assert 'Required verification' in promoter and "required[0]['conclusion'] == 'success'" in promoter
    assert 'git/refs/heads/main' in promoter and '-F force=false' in promoter and 'force=true' not in promoter
    assert 'Read back exact promoted main' in promoter and 'EXPECTED_SHA' in promoter
    assert 'checkout@' not in promoter, 'Promotion must not execute repository or artifact code'
    publisher = ci.split('\n  publish:', 1)[1]
    assert 'needs: [required, promote]' in publisher and "needs.promote.result == 'success'" in publisher
    assert "github.ref == 'refs/heads/main'" in publisher
    workflow = (ROOT/'.github/workflows/release.yml').read_text(encoding='utf-8')
    assert "main['object']['sha'] == os.environ['SOURCE_SHA']" in workflow
    assert "run['head_branch'] == os.environ['GITHUB_REF_NAME']" in workflow
    assert 'Promote exact verified source' in workflow
    assert "run['id'] == int(os.environ['GITHUB_RUN_ID'])" in workflow
    build = (ROOT/'.github/workflows/build-desktop.yml').read_text(encoding='utf-8')
    assert 'owner_signing:' in build
    assert 'NUMERA_RELEASE_SIGNING_KEY:' in build
    assert 'NUMERA_CODE_SIGNING_PFX:' in build and 'NUMERA_CODE_SIGNING_PASSWORD:' in build
    assert build.index('Sign owned Windows executables') < build.index("--platform windows-x64 --output")
    distribution = ci.split('\n  distribution:', 1)[1].split('\n  required:', 1)[0]
    prerequisites = yaml.safe_load(ci)['jobs']['distribution']['needs']
    assert {'codeql', 'dependencies', 'desktop-contracts'} <= set(prerequisites)
    assert "needs.codeql.result == 'success'" in distribution
    assert 'github.event.repository.private' in distribution
    codeql = (ROOT / '.github/workflows/codeql.yml').read_text(encoding='utf-8')
    assert 'wait-for-processing: true' in codeql
    assert 'needs: analyze' in codeql and 'scripts/ci/codeql_gate.py' in codeql
    assert 'checks: read' in codeql
