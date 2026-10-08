"""Prove this partial native library is not selected as the application runtime."""
from __future__ import annotations

from pathlib import Path
from check_coverage import check
from run_logging import run_log

ROOT = Path(__file__).resolve().parents[2]


def verify() -> None:
    # A passing pure seam is not a release gate: incomplete source ledger must
    # still refuse cutover and no replacement application binary is advertised.
    if not check(ROOT / 'docs' / 'migration-coverage.json'):
        raise ValueError('Partial migration unexpectedly claims readiness')
    manifest = (ROOT / 'engine-rust' / 'Cargo.toml').read_text(encoding='utf-8')
    if '[[bin]]' in manifest or (ROOT / 'engine-rust' / 'src' / 'main.rs').exists():
        raise ValueError('Partial core must not publish replacement application binary')
    launcher = (ROOT / 'scripts' / 'build-desktop.ps1').read_text(encoding='utf-8')
    if 'PyInstaller' not in launcher or 'engine_entry.py' not in launcher:
        raise ValueError('Original portable engine path is no longer retained')


if __name__ == '__main__':
    with run_log('verify-default') as logger:
        verify()
        logger.info('Original application runtime retained; native migration not ready for cutover')
