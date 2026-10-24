"""Generate validated immutable identity inputs before freezing/signing a desktop build."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log

OWNER = 'KiaroSama'
PRODUCT = 'Numera Browser'


def identity() -> dict:
    sha = subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'],
                                  encoding='utf-8', errors='strict', timeout=10).strip()
    if not re.fullmatch('[0-9a-f]{40}', sha) or os.environ.get('GITHUB_SHA', sha) != sha:
        raise ValueError('Build identity checkout differs from its source SHA')
    version = json.loads((ROOT / 'dashboard/package.json').read_text(encoding='utf-8'))['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+', version) or any(int(part) > 65535 for part in version.split('.')):
        raise ValueError('Build identity requires a supported three-part product version')
    for name in ('engine/pyproject.toml', 'desktop/src-tauri/Cargo.toml'):
        text = (ROOT / name).read_text(encoding='utf-8')
        declared = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
        if declared is None or declared.group(1) != version:
            raise ValueError('Build identity product versions disagree')
    config = json.loads((ROOT / 'desktop/src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
    if config['version'] != version or config['productName'] != PRODUCT or config['bundle']['publisher'] != OWNER:
        raise ValueError('Desktop identity differs from the approved product and owner')
    generation = os.environ.get('GITHUB_RUN_ID', '0')
    if not re.fullmatch(r'0|[1-9][0-9]{0,15}', generation) or (
            os.environ.get('GITHUB_ACTIONS') == 'true' and generation == '0'):
        raise ValueError('Hosted build identity requires its actual positive generation')
    return {'schema': 1, 'owner': OWNER, 'product': PRODUCT, 'version': version,
            'source_sha': sha, 'generation': int(generation)}


def prepare(directory: Path, acceptance_generation: int | None = None) -> dict:
    record = identity()
    if acceptance_generation is not None:
        if record['generation'] <= 0 or acceptance_generation not in (
                record['generation'] * 2, record['generation'] * 2 + 1):
            raise ValueError('Acceptance requires an actual hosted fixture generation')
        record.update(generation=acceptance_generation, acceptance=True)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / 'numera_build.py').write_text('IDENTITY = ' + repr(record) + '\n', encoding='utf-8')
    fields = {'CompanyName': OWNER, 'FileDescription': PRODUCT + ' engine',
              'FileVersion': record['version'], 'ProductName': PRODUCT,
              'ProductVersion': record['version'], 'OriginalFilename': 'numera-engine.exe',
              'LegalCopyright': OWNER, 'Comments': 'Source ' + record['source_sha'],
              'PrivateBuild': record['source_sha']}
    numbers = tuple(map(int, record['version'].split('.'))) + (0,)
    strings = ',\n'.join(f'        StringStruct({name!r}, {value!r})' for name, value in fields.items())
    resource = f'''VSVersionInfo(
  ffi=FixedFileInfo(filevers={numbers!r}, prodvers={numbers!r},
    mask=0x3f, flags=0x0, OS=0x40004, fileType=0x1, subtype=0x0, date=(0, 0)),
  kids=[StringFileInfo([StringTable('040904B0', [
{strings}
  ])]), VarFileInfo([VarStruct('Translation', [1033, 1200])])]
)
'''
    (directory / 'windows-version.txt').write_text(resource, encoding='utf-8')
    return record


def verify(stage: Path) -> dict:
    expected = identity()
    suffix = '.exe' if os.name == 'nt' else ''
    env = os.environ.copy()
    for name in ('NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX', 'NUMERA_CODE_SIGNING_PASSWORD'):
        env.pop(name, None)
    for relative in ('numera' + suffix, 'engine/numera-engine' + suffix):
        path = stage / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError('Build identity executable is missing or linked')
        result = subprocess.run([str(path.resolve()), '--build-info'], stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env,
                                timeout=15, check=False)
        if result.returncode or len(result.stdout) > 4096:
            raise ValueError('Embedded build identity command failed')
        if json.loads(result.stdout.decode('utf-8', errors='strict')) != expected:
            raise ValueError('Embedded executable identity differs from the exact build source')
    return expected


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    preparation = commands.add_parser('prepare')
    preparation.add_argument('--directory', type=Path, required=True)
    preparation.add_argument('--acceptance-generation', type=int)
    commands.add_parser('verify').add_argument('--stage', type=Path, required=True)
    args = parser.parse_args()
    with run_log('build-identity') as logger:
        if args.command == 'prepare':
            record = prepare(args.directory, args.acceptance_generation)
        else:
            record = verify(args.stage)
        logger.info('Build identity %s owner=%s version=%s source_sha=%s generation=%s',
                    args.command, record['owner'], record['version'], record['source_sha'], record['generation'])
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
