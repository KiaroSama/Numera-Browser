"""Prove current native regressions detect their exact historical production defects."""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import logging
import os
from pathlib import Path
import re
import subprocess

REGRESSIONS = (
    {
        'target': 'model_parity',
        'test': 'optional_proxy_reads_match_original_without_rewriting_storage',
        'symptom': '"false" model verdict',
        'commit': 'a2942c0e18c2d90b622bf10a7598c417158f39b6',
        'files': {
            'engine-rust/src/models.rs':
                'be512b37753170fc18a479cadd50e308f6088f76a040977a3f932bffd7fda0f4',
        },
    },
    {
        'target': 'cookie_export_parity',
        'test': 'netscape_export_matches_original_bytes_and_refusals',
        'symptom': '"json-padding-001c" cookie format detection',
        'commit': '87bc6022ef3e576ffe2d9433dfb83ea859d11fb2',
        'files': {
            'engine-rust/src/browser/cookie_format.rs':
                '50c6771ac687ab326b6c5f22a5be8e3f49920d1fe09086cfa2ea7ea9201bc936',
            'engine-rust/src/browser/cookie_normalize.rs':
                'a9f11c1651a1fef70b36380f5b89d09a80c3c36a1e07d5a9c194a5a08a9a97ee',
        },
    },
    {
        'target': 'cookie_export_parity',
        'test': 'netscape_export_matches_original_bytes_and_refusals',
        'symptom': '"positive-expires" numeric expiry normalization verdict',
        'commit': '41917dfa020f107ca95f1ee66ffb961786b6be00',
        'files': {
            'engine-rust/src/browser/cookie_expiry.rs':
                '50578cdaa9765e22754c430d910ae302eb1ae1fddc566ccba8fdefe9d23944e4',
        },
    },
)


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
    files = [name for case in REGRESSIONS for name in case['files']]
    git(root, 'diff', '--exit-code', 'HEAD', '--', *files)
    return sha


@contextmanager
def historical_sources(root: Path, case: dict):
    originals, replacements = {}, {}
    for name, expected in case['files'].items():
        raw = git(root, 'show', f"{case['commit']}:{name}")
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
    keys = (ENVIRONMENT, 'PYTHONDONTWRITEBYTECODE')
    previous = {key: os.environ.get(key) for key in keys}
    os.environ.update({ENVIRONMENT: str(original), 'PYTHONDONTWRITEBYTECODE': '1'})
    try:
        for case in REGRESSIONS:
            evidence = FailureEvidence(case)
            logger.addHandler(evidence)
            try:
                with historical_sources(root, case):
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
