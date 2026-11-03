"""Run bounded native macOS ownership acceptance against the frozen source."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from baseline_source import ENVIRONMENT, reference
from native_regressions import prove
from run_logging import run_log

HISTORICAL_CASES = (
    {
        'target': 'lib',
        'test': 'ownership::macos::tests::macos_public_identity_distinguishes_absence_from_observation_failure',
        'symptom': 'macOS process identity differs',
        'platform': 'darwin',
        'commit': '897caadef85053d6d74896e901fc9a21f7baa6dd',
        'files': {'engine-rust/src/ownership/identity.rs':
                  'c5f41e97c748c392ac896dd835bb4d13bef3d92ceefa6240e106538672b44f56'},
    },
    {
        'target': 'lib',
        'test': 'ownership::macos::tests::macos_public_group_observation_retains_every_uncertain_claim',
        'symptom': 'macOS group uncertainty differs',
        'platform': 'darwin',
        'commit': '897caadef85053d6d74896e901fc9a21f7baa6dd',
        'files': {'engine-rust/src/ownership/tree.rs':
                  '201260e01da09fea13951ba61ab0265faa68499554d4cfefaf712a179dc8b36b'},
    },
)


CURRENT_TESTS = (
    'ownership::macos::tests::macos_public_group_observation_retains_every_uncertain_claim',
    'ownership::macos::tests::macos_group_growth_is_bounded_and_never_calls_truncation_empty',
    'ownership::macos::tests::macos_public_identity_distinguishes_absence_from_observation_failure',
    'ownership::macos::tests::macos_birth_identity_refuses_malformed_records_and_reused_tokens',
)
IDENTITY_TEST = 'native_birth_token_matches_original_runtime_for_the_same_live_process'
TREE_TEST = 'native_owned_tree_contains_descendants_before_their_first_capability'


class CurrentEvidence:
    def __init__(self, names: tuple[str, ...]):
        self.names = set(names)
        self.passed = set()
        self.started = self.completed = 0

    def accept(self, line: str) -> None:
        if re.fullmatch(r'running \d+ tests?', line):
            if line not in (f'running {len(self.names)} test', f'running {len(self.names)} tests'):
                raise ValueError('Native macOS acceptance selected the wrong test count')
            self.started += 1
        elif line.startswith('test ') and ' ... ' in line:
            name, outcome = line[5:].rsplit(' ... ', 1)
            if outcome != 'ok' or name not in self.names or name in self.passed:
                raise ValueError('Native macOS acceptance has an unexpected or unsuccessful test')
            self.passed.add(name)
        elif line.startswith('test result:'):
            if not re.fullmatch(
                rf'test result: ok\. {len(self.names)} passed; 0 failed; 0 ignored; '
                r'0 measured; \d+ filtered out; finished in [0-9.]+s', line
            ):
                raise ValueError('Native macOS acceptance summary is incomplete')
            self.completed += 1

    def verify(self, code: int) -> None:
        if code != 0 or self.started != 1 or self.completed != 1 or self.passed != self.names:
            raise ValueError('Native macOS acceptance did not pass every required named test')


def run_current(command: list[str], environment: dict, names: tuple[str, ...]) -> None:
    evidence = CurrentEvidence(names)
    # The workflow's outer run_owned retains the unchanged wall/idle/tree bounds.
    with subprocess.Popen(command, env=environment, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, text=True, encoding='utf-8', errors='strict') as child:
        rejected = False
        for line in child.stdout:
            print(line, end='', flush=True)
            try:
                evidence.accept(line.rstrip('\r\n'))
            except ValueError:
                # Drain until exit rather than leave a child blocked on its pipe.
                rejected = True
        code = child.wait()
        if rejected:
            raise ValueError('Native macOS acceptance emitted invalid test evidence')
        evidence.verify(code)


def require_host() -> None:
    expected = {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'KiaroSama/Numera-Browser',
                'RUNNER_OS': 'macOS'}
    if (any(os.environ.get(key) != value for key, value in expected.items())
            or platform.system() != 'Darwin'):
        raise ValueError('Native ownership acceptance requires its owning hosted macOS job')


def execute(phase: str, logger) -> None:
    require_host()
    if phase not in ('build', 'test'):
        raise ValueError('Unknown macOS ownership acceptance phase')
    base = ['cargo', 'test', '--manifest-path', str(ROOT / 'engine-rust/Cargo.toml'), '--locked']
    with reference() as original:
        environment = {**os.environ, ENVIRONMENT: str(original),
                       'NUMERA_ORACLE_PYTHON': sys.executable, 'PYTHONDONTWRITEBYTECODE': '1',
                       'CARGO_BUILD_JOBS': '2'}

        def run(command: list[str], selected: dict | None = None) -> None:
            subprocess.run(command, env=environment if selected is None else selected,
                           stdin=subprocess.DEVNULL, check=True)

        if phase == 'build':
            run(['cargo', 'fetch', '--manifest-path', str(ROOT / 'engine-rust/Cargo.toml'), '--locked'])
            prove(ROOT, original, logger, cases=HISTORICAL_CASES)
            run([*base, '--lib', '--test', 'ownership_identity_parity',
                 '--test', 'ownership_tree_parity', '--no-run'])
        else:
            run_current([*base, '--lib', '--', 'ownership::macos::tests::',
                         '--test-threads=2', '--color=never'], environment, CURRENT_TESTS)
            # Separate processes avoid mutating the test runner's global locale/TZ.
            for locale, timezone in (('C', 'UTC'), ('en_US.UTF-8', 'UTC'),
                                     ('en_US.UTF-8', 'America/Los_Angeles'),
                                     ('numera_invalid_locale', 'UTC')):
                selected = {**environment, 'LC_ALL': locale, 'TZ': timezone}
                run_current([*base, '--test', 'ownership_identity_parity', '--',
                             IDENTITY_TEST, '--exact', '--test-threads=1', '--color=never'],
                            selected, (IDENTITY_TEST,))
            run_current([*base, '--test', 'ownership_tree_parity', '--',
                         TREE_TEST, '--exact', '--test-threads=1', '--color=never'],
                        environment, (TREE_TEST,))
        logger.info('Native macOS ownership phase completed phase=%s', phase)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--phase', choices=('build', 'test'), required=True)
    args = parser.parse_args()
    with run_log('macos-native-ownership') as logger:
        try:
            execute(args.phase, logger)
        except Exception as error:
            logger.error('macOS ownership acceptance failed error=%s', type(error).__name__)
            raise
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
