"""macOS ownership must reach real native/source boundaries before distribution."""
from copy import deepcopy
import importlib.util
from pathlib import Path
from types import SimpleNamespace

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture(autouse=True)
def helper_imports(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/ci'))


def load():
    spec = importlib.util.spec_from_file_location('macos_ownership_fixture', ROOT / 'scripts/ci/macos_ownership.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.timeout(10)
@pytest.mark.parametrize('phase', ['build', 'test'])
def test_macos_ownership_phases_retain_public_seams_and_bounds(monkeypatch, phase):
    from contextlib import contextmanager

    module = load()
    monkeypatch.setattr(module, 'require_host', lambda: None)
    observed = []

    @contextmanager
    def reference():
        yield Path('/fixture-original')

    def run(command, *, env, stdin, check):
        assert check is True and stdin == module.subprocess.DEVNULL
        assert env['NUMERA_ORACLE_PYTHON'] == module.sys.executable
        assert env['NUMERA_ORACLE_SOURCE_ROOT'] == str(Path('/fixture-original'))
        assert env['CARGO_BUILD_JOBS'] == '2'
        observed.append((command, env))
        return SimpleNamespace(returncode=0)

    proofs = []

    def current(command, environment, names):
        assert names and environment['NUMERA_ORACLE_PYTHON'] == module.sys.executable
        observed.append((command, environment))

    monkeypatch.setattr(module, 'run_current', current)
    monkeypatch.setattr(module, 'reference', reference)
    monkeypatch.setattr(module.subprocess, 'run', run)
    monkeypatch.setattr(module, 'prove', lambda root, original, logger, *, cases:
                        proofs.append((root, original, cases)))
    module.execute(phase, SimpleNamespace(info=lambda *args: None))
    if phase == 'build':
        assert len(proofs) == 1 and len(proofs[0][2]) == 2
        assert all(case['platform'] == 'darwin' for case in proofs[0][2])
        assert observed[0][0][1] == 'fetch'
        command = observed[-1][0]
        assert '--no-run' in command and '--lib' in command
        assert command.count('--test') == 2
        assert {'ownership_identity_parity', 'ownership_tree_parity'} <= set(command)
    else:
        assert proofs == []
        assert len(observed) == 6
        assert 'ownership::macos::tests::' in observed[0][0]
        identity = observed[1:-1]
        assert len({(env['LC_ALL'], env['TZ']) for _, env in identity}) == 4
        assert all('ownership_identity_parity' in command and '--exact' in command for command, _ in identity)
        assert 'ownership_tree_parity' in observed[-1][0]
        assert all('--test-threads=2' in command or '--test-threads=1' in command for command, _ in observed)


@pytest.mark.timeout(10)
@pytest.mark.parametrize('damage', [None, 'zero', 'missing', 'duplicate', 'ignored',
                                    'wrong-name', 'summary', 'exit', 'incomplete'])
def test_macos_current_acceptance_rejects_false_green_output(monkeypatch, damage):
    module = load()
    names = ('fixture_identity', 'fixture_tree')
    lines = ['running 2 tests', 'test fixture_identity ... ok',
             'test fixture_tree ... ok',
             'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; '
             '4 filtered out; finished in 0.01s']
    if damage == 'zero':
        lines = ['running 0 tests', 'test result: ok. 0 passed; 0 failed; 0 ignored; '
                 '0 measured; 6 filtered out; finished in 0.00s']
    elif damage == 'missing':
        lines.pop(2)
    elif damage == 'duplicate':
        lines[2] = lines[1]
    elif damage == 'ignored':
        lines[2] = 'test fixture_tree ... ignored'
    elif damage == 'wrong-name':
        lines[2] = 'test unrelated ... ok'
    elif damage == 'summary':
        lines[-1] = lines[-1].replace('0 ignored', '1 ignored')
    elif damage == 'incomplete':
        lines.pop()

    class Child:
        stdout = [line + '\n' for line in lines]

        def __enter__(self):
            return self

        def __exit__(self, *args):
            return False

        def wait(self):
            return 1 if damage == 'exit' else 0

    def spawn(command, **options):
        assert command == ['cargo-fixture']
        assert options == {'env': {}, 'stdin': module.subprocess.DEVNULL,
                           'stdout': module.subprocess.PIPE, 'text': True,
                           'encoding': 'utf-8', 'errors': 'strict'}
        return Child()

    monkeypatch.setattr(module.subprocess, 'Popen', spawn)
    if damage:
        with pytest.raises(ValueError, match='Native macOS acceptance'):
            module.run_current(['cargo-fixture'], {}, names)
    else:
        module.run_current(['cargo-fixture'], {}, names)


@pytest.mark.timeout(10)
@pytest.mark.parametrize('damage', ['repository', 'actions', 'runner', 'system'])
def test_macos_ownership_refuses_foreign_hosts_before_any_execution(monkeypatch, damage):
    module = load()
    monkeypatch.setenv('GITHUB_ACTIONS', 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('RUNNER_OS', 'macOS')
    monkeypatch.setattr(module.platform, 'system', lambda: 'Darwin')
    if damage == 'repository':
        monkeypatch.setenv('GITHUB_REPOSITORY', 'fixture/other')
    elif damage == 'actions':
        monkeypatch.delenv('GITHUB_ACTIONS')
    elif damage == 'runner':
        monkeypatch.setenv('RUNNER_OS', 'Linux')
    else:
        monkeypatch.setattr(module.platform, 'system', lambda: 'Linux')
    monkeypatch.setattr(module.subprocess, 'run', lambda *args, **kwargs: pytest.fail('Foreign host executed a command'))
    with pytest.raises(ValueError, match='owning hosted macOS job'):
        module.execute('test', None)


@pytest.mark.timeout(10)
@pytest.mark.parametrize('damage', [None, 'build', 'test', 'platform', 'bound', 'history'])
def test_macos_ownership_is_required_in_existing_three_platform_contract_job(damage):
    workflow = yaml.safe_load((ROOT / '.github/workflows/desktop-contracts.yml').read_text(encoding='utf-8'))
    workflow = deepcopy(workflow)
    steps = workflow['jobs']['contracts']['steps']
    selected = [step for step in steps if 'macos_ownership.py' in step.get('run', '')]
    if damage in ('build', 'test'):
        selected[:] = [step for step in selected if f'--phase {damage}' not in step['run']]
    elif damage == 'platform':
        selected[0]['if'] = "runner.os == 'Linux'"
    elif damage == 'bound':
        selected[0]['run'] = selected[0]['run'].replace('--wall 900', '--wall 1800')
    elif damage == 'history':
        next(step for step in steps if 'actions/checkout@' in step.get('uses', ''))['with']['fetch-depth'] = 1

    def verify():
        assert len(selected) == 2
        for phase, wall, idle in [('build', 900, 300), ('test', 600, 120)]:
            matches = [step for step in selected if step.get('run') == (
                f'python scripts/migration/run_owned.py --wall {wall} --idle {idle} -- '
                f'python scripts/ci/macos_ownership.py --phase {phase}')]
            assert len(matches) == 1
            assert matches[0].get('if') == "runner.os == 'macOS'"
            assert not matches[0].get('continue-on-error')
        checkout = next(step for step in steps if 'actions/checkout@' in step.get('uses', ''))
        assert checkout['with']['fetch-depth'] == 0

    if damage:
        with pytest.raises(AssertionError):
            verify()
    else:
        verify()
