"""Immutable historical inputs are reused without reusing proof verdicts."""
import hashlib
import logging
import sys
from types import SimpleNamespace

import pytest

from test_native_regressions import load


@pytest.mark.parametrize('change', ['none', 'hash', 'missing', 'linked'])
def test_reused_inputs_revalidate_integrity_and_current_paths(tmp_path, monkeypatch, change):
    module = load()
    path = tmp_path / 'engine-rust/src/models.rs'
    path.parent.mkdir(parents=True)
    current, historical = b'current source\n', b'historical source\n'
    path.write_bytes(current)
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(historical).hexdigest()}, companions={})
    second = dict(case, files=dict(case['files']))
    if change == 'hash':
        second['files']['engine-rust/src/models.rs'] = '0' * 64
    monkeypatch.setattr(module, 'REGRESSIONS', (case, second))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setitem(sys.modules, 'baseline_source',
                        SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    reads, proofs = [], []

    def git(root, *arguments):
        reads.append(arguments)
        return historical

    def run(command, wall, idle, logger):
        proofs.append(command)
        assert path.read_bytes() == historical
        assert (wall, idle) == (180, 90)
        logger.info('running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 101

    class Logger(logging.Logger):
        def info(self, message, *args, **kwargs):
            super().info(message, *args, **kwargs)
            if message.startswith('Historical regression caught') and len(proofs) == 1:
                assert path.read_bytes() == current
                if change == 'missing':
                    path.unlink()
                elif change == 'linked':
                    original_link = type(path).is_symlink
                    monkeypatch.setattr(type(path), 'is_symlink', lambda item:
                                        item == path or original_link(item))

    monkeypatch.setattr(module, 'git', git)
    monkeypatch.setattr(module.subprocess, 'run', lambda *args, **kwargs: None)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    logger = Logger('historical-input-fixture', level=logging.INFO)
    if change == 'none':
        module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(proofs) == 2 and len(reads) == 1
        module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(proofs) == 4 and len(reads) == 2, 'Inputs leaked across prove calls'
    else:
        with pytest.raises(ValueError, match='integrity differs|regular maintained file'):
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(proofs) == 1
        assert len(reads) == (2 if change == 'hash' else 1)
    assert logger.handlers == []
    if change != 'missing':
        assert path.read_bytes() == current


@pytest.mark.timeout(10)
@pytest.mark.parametrize('damage', [None, 'empty', 'platform', 'dirty'])
def test_explicit_platform_proofs_keep_exact_admission_and_do_not_run_other_cases(
        tmp_path, monkeypatch, damage):
    module = load()
    path = tmp_path / 'engine-rust/src/ownership/identity.rs'
    path.parent.mkdir(parents=True)
    current, historical = b'current identity\n', b'previous unsupported identity\n'
    path.write_bytes(current)
    case = dict(module.REGRESSIONS[0], platform='fixture-host', files={
        'engine-rust/src/ownership/identity.rs': hashlib.sha256(historical).hexdigest()},
        companions={})
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform='fixture-host'))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setitem(sys.modules, 'baseline_source',
                        SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    monkeypatch.setattr(module.subprocess, 'run', lambda *args, **kwargs: None)
    calls = []

    def git(root, *arguments):
        calls.append(arguments)
        if arguments[0] == 'diff':
            if damage == 'dirty':
                raise module.subprocess.CalledProcessError(1, ['git', 'diff'])
            return b''
        assert arguments == ('show', f"{case['commit']}:engine-rust/src/ownership/identity.rs")
        return historical

    def run(command, wall, idle, logger):
        assert path.read_bytes() == historical and (wall, idle) == (180, 90)
        logger.info('running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 101

    monkeypatch.setattr(module, 'git', git)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    logger = logging.Logger('explicit-platform-proof', level=logging.INFO)
    selected = () if damage == 'empty' else (dict(case, platform='other') if damage == 'platform' else case,)
    if damage:
        with pytest.raises((ValueError, module.subprocess.CalledProcessError)):
            module.prove(tmp_path, tmp_path / 'original', logger, cases=selected)
        assert not any(call[0] == 'show' for call in calls)
    else:
        module.prove(tmp_path, tmp_path / 'original', logger, cases=selected)
        assert calls == [('diff', '--exit-code', 'HEAD', '--', 'engine-rust/src/ownership/identity.rs'),
                         ('show', f"{case['commit']}:engine-rust/src/ownership/identity.rs")]
    assert path.read_bytes() == current and logger.handlers == []


@pytest.mark.timeout(10)
@pytest.mark.parametrize('previous', ['absent', 'present', 'rename', 'linked'])
def test_history_timings_preserve_previous_reports_and_refuse_uncertain_paths(tmp_path, monkeypatch, previous):
    module = load()
    timing = tmp_path / 'engine-rust/target/cargo-timings'
    owned = tmp_path / 'owned-temp'
    owned.mkdir()
    monkeypatch.setenv('NUMERA_TEST_TEMP', str(owned))
    output = tmp_path / 'step-output'
    monkeypatch.setenv('GITHUB_OUTPUT', str(output))
    timing.mkdir(parents=True)
    old = b'<html>previous build timing</html>'
    (timing / 'cargo-timing.html').write_bytes(old)
    if previous == 'absent':
        (timing / 'cargo-timing.html').unlink()
        timing.rmdir()
    elif previous == 'rename':
        rename = type(timing).rename

        def denied(path, destination):
            if path == timing:
                raise PermissionError('Fixture timing retention denied')
            return rename(path, destination)

        monkeypatch.setattr(type(timing), 'rename', denied)
    elif previous == 'linked':
        is_symlink = type(timing).is_symlink
        monkeypatch.setattr(type(timing), 'is_symlink',
                            lambda path: path == timing or is_symlink(path))
    source = tmp_path / 'engine-rust/src/models.rs'
    source.parent.mkdir()
    source.write_bytes(b'current source')
    historical = b'historical source'
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(historical).hexdigest()}, companions={})
    monkeypatch.setattr(module, 'REGRESSIONS', (case,))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setattr(module, 'git', lambda *args: historical)
    monkeypatch.setitem(sys.modules, 'baseline_source', SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    calls = []

    def compile(command, **kwargs):
        calls.append(command)
        assert '--timings' in command and not timing.exists()
        timing.mkdir()
        (timing / 'cargo-timing.html').write_bytes(b'<html>current build timing</html>')
        raise module.subprocess.CalledProcessError(1, command)

    monkeypatch.setattr(module.subprocess, 'run', compile)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=lambda *args: pytest.fail('Unexpected proof')))
    logger = logging.Logger('historical-timing-fixture', level=logging.INFO)
    if previous in ('rename', 'linked'):
        with pytest.raises((ValueError, PermissionError), match='Historical timing directory|Fixture timing retention denied'):
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert calls == [] and (timing / 'cargo-timing.html').read_bytes() == old
        assert not output.exists(), 'Old timing reports were advertised as fresh'
    else:
        with pytest.raises(module.subprocess.CalledProcessError):
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(calls) == 1
        assert output.read_text(encoding='utf-8') == 'timings_ready=true\n'
        assert (timing / 'cargo-timing.html').read_bytes() == b'<html>current build timing</html>'
        retained = list(owned.glob('native-history-timings-*/previous/cargo-timing.html'))
        assert len(retained) == (1 if previous == 'present' else 0)
        if retained:
            assert retained[0].read_bytes() == old
    assert source.read_bytes() == b'current source' and logger.handlers == []


@pytest.mark.timeout(10)
def test_directory_metadata_history_retains_exact_prior_source_and_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[31]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 35
    assert case == {
        'target': 'first_run_parity',
        'test': 'imported_new_directories_preserve_original_metadata_without_replacing_existing_modes',
        'symptom': 'First-run directory metadata differs',
        'commit': '1a46f71178be5c482ecad04f14666eadaa52dc0d',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '6a9afa7356a02b123d13ebf8d5742a67e4d6e2bf8abff0d1cb8f2459a94c5670'},
    }
    assert module.REGRESSIONS[-4] == case


@pytest.mark.timeout(10)
def test_cli_integer_history_retains_exact_prior_source_and_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[32]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 35
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'integer_options_preserve_original_digit_bound_before_zero_normalization',
        'symptom': 'CLI integer digit admission differs',
        'commit': 'ff56fc25f25a4ad9874e644b85295ccd622fd34f',
        'files': {'engine-rust/src/cli/definitions.rs':
                  '62577821c1120e9518a30277bb165d4e77c103ef5803c06157a2a4bc4922c4a7'},
    }
    assert module.REGRESSIONS[-3] == case


@pytest.mark.timeout(10)
def test_hint_peer_disconnect_history_retains_compile_compatible_source_and_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[33]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 35
    assert case == {
        'target': 'browser_hints_parity',
        'test': 'hint_response_peer_disconnects_preserve_other_io_failures',
        'symptom': 'Hint response peer disconnect escaped',
        'commit': 'f173f3ae59c91fc9524507f91fe6d936b2c4a0d4',
        'files': {'engine-rust/tests/common/hint_site.rs':
                  '4dcfdaa91e4079186c5b7c9268ad50efa06c7f6f5782170c4af8fa2172a879ae'},
    }
    assert module.REGRESSIONS[-2] == case


@pytest.mark.timeout(10)
def test_cli_option_token_history_retains_prior_parser_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[-1]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 35
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'separate_option_values_match_original_token_classification',
        'symptom': 'CLI option token admission differs',
        'commit': 'c299ef506c593623bfd0646b3d2efe1d83596532',
        'files': {'engine-rust/src/cli/mod.rs':
                  'c534342a6f7db08b99641bd52e02e729d3abb68a1e78257b0c502c077479fcd8'},
    }
    assert module.REGRESSIONS[-1] == case
