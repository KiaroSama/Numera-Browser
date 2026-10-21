"""Prove current native regressions detect their exact historical production defects."""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import logging
import os
from pathlib import Path
import re
import subprocess
import sys

from native_regression_cases import REGRESSIONS


# Option-returning historical secret readers require their coherent caller group.
# Selected defect files and earlier compatibility snapshots always take precedence.
_STORAGE_READERS_COMMIT = 'bec37974fee24319633a515321cd24d280c68a86'
_STORAGE_READERS = {
    'engine-rust/src/storage/repo.rs':
        '5190d3963d0445188ed9637717012c5e182692218a0f7d580b3f5fffa39db143',
    'engine-rust/src/storage/repo_lookup.rs':
        '8e9302ebeb78527a8e11b2718087079b57ed59eb5e6baadec8649523b4553fc8',
    'engine-rust/src/storage/repo_reconcile.rs':
        '3abbe60377f64cb60a3805ee680705cd0636db8195ca2cefd65c30e123e5c0fa',
    'engine-rust/src/storage/repo_write.rs':
        '07310ce8d5bee16afdc5bc0fc4115f568878adad8ba49f7c9efccc164a8e327e',
    'engine-rust/src/storage/dashboard.rs':
        '70d0c8558d9ed892067437bdf1d29a815e5b3feb20eafd5d6468623b70425417',
    'engine-rust/src/storage/pool.rs':
        '38bc50a2381a5c18a025361ff5fd5d8d2ee060a9efe24cf957e3723e16ed8707',
    'engine-rust/src/storage/archive.rs':
        '8c8e6fcec5412a5f7d603cbaf9bb0f878636af294a8db72ea81e78a2dbdd696b',
}
REGRESSIONS = tuple(
    dict(case, companions={
        **{name: {'commit': _STORAGE_READERS_COMMIT, 'sha256': expected}
           for name, expected in _STORAGE_READERS.items() if name not in case['files']},
        **case.get('companions', {}),
    }) if _STORAGE_READERS.keys() & (case['files'].keys() | case.get('companions', {}).keys())
    else case
    for case in REGRESSIONS
) + ({
    'target': 'repo_parity',
    'test': 'settings_errors::storage_settings_errors_match_original_lazy_reads_and_preserve_bytes',
    'symptom': 'Storage settings admission differs',
    'commit': _STORAGE_READERS_COMMIT,
    'files': _STORAGE_READERS,
},)

# Settings signature swaps must keep all affected historical callers coherent.
import native_settings_history
_STORAGE_REGRESSIONS = REGRESSIONS
REGRESSIONS = native_settings_history.compatible(REGRESSIONS, _STORAGE_READERS)


def git(root: Path, *arguments: str) -> bytes:
    return subprocess.run(
        ['git', '-C', str(root), *arguments], stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=True,
        env={**os.environ, 'GIT_NO_REPLACE_OBJECTS': '1'},
    ).stdout


def require_hosted_source(root: Path) -> str:
    sha = os.environ.get('GITHUB_SHA', '')
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('GITHUB_REPOSITORY') != 'KiaroSama/Numera-Browser'
            or not re.fullmatch('[0-9a-f]{40}', sha)
            or git(root, 'rev-parse', 'HEAD').decode('ascii').strip() != sha):
        raise ValueError('Historical regression execution requires the exact owning hosted source')
    files = [name for case in REGRESSIONS
             for name in (*case['files'], *case.get('companions', {}))]
    git(root, 'diff', '--exit-code', 'HEAD', '--', *files)
    return sha


@contextmanager
def historical_sources(root: Path, case: dict):
    originals, replacements = {}, {}
    sources = {name: (case['commit'], expected) for name, expected in case['files'].items()}
    sources.update({name: (item['commit'], item['sha256'])
                    for name, item in case.get('companions', {}).items()})
    for name, (commit, expected) in sources.items():
        raw = git(root, 'show', f'{commit}:{name}')
        if len(raw) > 32768 or hashlib.sha256(raw).hexdigest() != expected:
            raise ValueError('Historical regression source integrity differs')
        raw.decode('utf-8', errors='strict')
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError('Historical regression source is not a regular maintained file')
        originals[path] = path.read_bytes()
        replacements[path] = raw
    try:
        for path, raw in replacements.items():
            path.write_bytes(raw)
        yield
    finally:
        failed = []
        for path, raw in originals.items():
            try:
                path.write_bytes(raw)
                if path.read_bytes() != raw:
                    failed.append(path.name)
            except OSError:
                failed.append(path.name)
        if failed:
            raise RuntimeError('Fixed native source restoration failed: ' + ', '.join(failed))


class FailureEvidence(logging.Handler):
    def __init__(self, case: dict):
        super().__init__()
        self.case = case
        self.started = self.named = self.symptom = self.summary = False

    def emit(self, record):
        text = record.getMessage()
        self.started |= text == 'running 1 test'
        self.named |= text == f"test {self.case['test']} ... FAILED"
        self.symptom |= self.case['symptom'] in text
        self.summary |= bool(re.fullmatch(
            r'test result: FAILED\. 0 passed; 1 failed; 0 ignored; 0 measured; '
            r'\d+ filtered out; finished in [0-9.]+s', text))

    def verify(self, code: int) -> None:
        if code != 101 or not all((self.started, self.named, self.symptom, self.summary)):
            raise ValueError('Historical native regression did not fail at its required production assertion: '
                             + self.case['target'])


def prove(root: Path, original: Path, logger) -> None:
    from run_owned import run
    from baseline_source import ENVIRONMENT

    sha = require_hosted_source(root)
    keys = (ENVIRONMENT, 'PYTHONDONTWRITEBYTECODE', 'CARGO_INCREMENTAL')
    previous = {key: os.environ.get(key) for key in keys}
    # rust-cache disables this for cross-job caches; these source swaps share one build tree.
    os.environ.update({ENVIRONMENT: str(original), 'PYTHONDONTWRITEBYTECODE': '1',
                       'CARGO_INCREMENTAL': '1'})
    try:
        applicable = (case for case in REGRESSIONS
                      if case.get('platform', sys.platform) == sys.platform)
        for index, case in enumerate(applicable):
            evidence = FailureEvidence(case)
            logger.addHandler(evidence)
            try:
                with historical_sources(root, case):
                    if index == 0:
                        # Cold dependency compilation belongs to the outer build budget.
                        subprocess.run(['cargo', 'test', '--manifest-path', str(root / 'engine-rust/Cargo.toml'),
                                        '--locked', '--offline', '--test', case['target'], '--no-run'],
                                       stdin=subprocess.DEVNULL, check=True)
                    command = ['cargo', 'test', '--manifest-path', str(root / 'engine-rust/Cargo.toml'),
                               '--locked', '--offline', '--test', case['target'], '--', case['test'],
                               '--exact', '--test-threads=1', '--color=never']
                    code = run(command, 180, 90, logger)
                evidence.verify(code)
            finally:
                logger.removeHandler(evidence)
                evidence.close()
            logger.info('Historical regression caught target=%s test=%s source_sha=%s; fixed bytes restored',
                        case['target'], case['test'], sha)
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
