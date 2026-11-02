"""Prove the updater rollback refuses occupied recovery destinations."""
from __future__ import annotations

import logging
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from native_regressions import FailureEvidence, historical_sources, require_hosted_source
from run_logging import run_log
from run_owned import run

CASE = {
    'target': 'desktop-updater',
    'test': 'updates::tests::rollback_preserves_an_occupied_recovery_destination',
    'symptom': 'Rollback overwrote an occupied recovery destination',
    'commit': '09d6fd84b0cbe0c9214cf9bb3471598b8b748507',
    'files': {'desktop/src-tauri/src/updates/files.rs':
              '10c24cb52eb6ec786584772b6359b865d9060e3827edbada9f8754e846482bc3'},
}


def prove(root: Path, logger: logging.Logger) -> None:
    if (sys.platform not in ('linux', 'darwin')
            or os.environ.get('RUNNER_OS') != {'linux': 'Linux', 'darwin': 'macOS'}.get(sys.platform)):
        raise ValueError('Desktop historical proof requires its owning hosted Unix platform')
    sha = require_hosted_source(root)
    subprocess.run(['git', '-C', str(root), 'diff', '--exit-code', 'HEAD', '--', *CASE['files']],
                   stdin=subprocess.DEVNULL, check=True, timeout=10)
    base = ['cargo', 'test', '--manifest-path', str(root / 'desktop/src-tauri/Cargo.toml'),
            '--locked', '--offline', '--no-default-features', '--bin', 'numera']
    evidence = FailureEvidence(CASE)
    logger.addHandler(evidence)
    try:
        with historical_sources(root, CASE, {}):
            # Compilation must succeed independently of the named semantic failure.
            subprocess.run([*base, '--no-run'], stdin=subprocess.DEVNULL, check=True)
            code = run([*base, '--', CASE['test'], '--exact', '--test-threads=1', '--color=never'],
                       180, 90, logger)
        evidence.verify(code)
    finally:
        logger.removeHandler(evidence)
        evidence.close()
    logger.info('Desktop historical rollback regression caught source_sha=%s; fixed bytes restored', sha)


if __name__ == '__main__':
    with run_log('desktop-history') as logger:
        prove(ROOT, logger)
