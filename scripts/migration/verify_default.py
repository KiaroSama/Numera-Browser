"""Prove this partial native library is not selected as the application runtime."""
from __future__ import annotations

from pathlib import Path
from check_coverage import check
from run_logging import run_log

ROOT = Path(__file__).resolve().parents[2]


def verify_candidate_target(metadata: dict) -> None:
    # Cargo metadata is authoritative, including defaults and feature expansion.
    packages = [item for item in metadata['packages'] if item['name'] == 'numera-core']
    if len(packages) != 1:
        raise ValueError('Native candidate package is missing or duplicated')
    package = packages[0]
    targets = [item for item in package['targets'] if 'bin' in item['kind']]
    expected = {
        'numera-native-candidate': ('native-candidate', 'examples/native_candidate.rs'),
        'numera-browser-runtime': ('native-browser', 'src/bin/browser_runtime.rs'),
    }
    if (package['features'] != {'default': [], 'native-candidate': [],
                               'native-browser': ['dep:playwright-rs']}
            or len(targets) != len(expected) or {target['name'] for target in targets} != set(expected)
            or any(target.get('required-features') != [expected[target['name']][0]]
                   or Path(target['src_path']).resolve() !=
                       (ROOT / 'engine-rust' / expected[target['name']][1]).resolve()
                   or target['test'] for target in targets)):
        raise ValueError('Native candidate must be explicit, isolated and disabled by default')


def verify() -> None:
    # A passing candidate is not a release gate: incomplete source coverage
    # still refuses cutover and the retained product remains selected.
    if not check(ROOT / 'docs' / 'migration-coverage.json'):
        raise ValueError('Partial migration unexpectedly claims readiness')
    import json
    import subprocess

    metadata = json.loads(subprocess.run(
        ['cargo', 'metadata', '--manifest-path', str(ROOT / 'engine-rust/Cargo.toml'),
         '--locked', '--no-deps', '--format-version', '1'],
        stdin=subprocess.DEVNULL, capture_output=True, encoding='utf-8', errors='strict',
        timeout=30, check=True,
    ).stdout)
    verify_candidate_target(metadata)
    launcher = (ROOT / 'scripts' / 'build-desktop.ps1').read_text(encoding='utf-8')
    if 'PyInstaller' not in launcher or 'engine_entry.py' not in launcher:
        raise ValueError('Original portable engine path is no longer retained')


if __name__ == '__main__':
    with run_log('verify-default') as logger:
        verify()
        logger.info('Original application runtime retained; native migration not ready for cutover')
