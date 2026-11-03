"""Package exact-source hosted builds without private files or restricted binaries."""
from __future__ import annotations

import argparse
import gzip
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log  # noqa: E402

NOTICES = {
    'LICENSE': 'LICENSE',
    'docs/licensing.md': 'docs/licensing.md',
    'docs/browser-notices.txt': 'docs/browser-notices.txt',
    'docs/migration-random-notices.txt': 'docs/migration-random-notices.txt',
    'engine-rust/src/http/documentation-assets/NOTICE.md':
        'engine-rust/src/http/documentation-assets/NOTICE.md',
}
PLATFORMS = ('windows-x64', 'linux-x64', 'macos-arm64')
SHA = re.compile(r'[0-9a-f]{40}')
ASSET = re.compile(r'numera-[a-z0-9][a-z0-9._-]*\.(?:tar\.gz|zip|whl)')


def public_path(name: str) -> None:
    path = PurePosixPath(name)
    forbidden = {'.ai', '.specify', 'specs', '.git', '.claude', '.agents', '.kiro',
                 '.codex', '.cursor', '.cline', '.codebase-memory', 'graphify-out',
                 '.ignoreme', 'node_modules', '__pycache__', '.venv', 'venv'}
    if (not name or not path.parts or path.is_absolute() or '\\' in name or '..' in path.parts
            or any(ord(character) < 32 for character in name)
            or any(part in forbidden for part in path.parts)
            or path.name.lower() in {'secrets.md', 'explain-ai.md', 'reference.md'}
            or path.parts[0] in {'data', 'plans', '.ci-work', '.ci-cache', '.ci-runner-win'}
            or name in {'CLAUDE.md', 'AGENTS.md'}
            or (path.name.startswith('.env') and path.name not in
                {'.env.example', '.env.sample', '.env.template', '.env.dist'})):
        raise ValueError(f'Refusing a non-public publication path: {name}')


def digest(path: Path) -> str:
    checksum = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            checksum.update(block)
    return checksum.hexdigest()


def asset_record(path: Path) -> dict:
    if not ASSET.fullmatch(path.name) or path.is_symlink() or not path.is_file():
        raise ValueError('Invalid release asset name or type')
    return {'name': path.name, 'size': path.stat().st_size, 'sha256': digest(path)}


def write_json(path: Path, value) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')


def git(*args: str) -> str:
    return subprocess.check_output(['git', '-C', str(ROOT), *args],
                                   encoding='utf-8', errors='strict', timeout=30).strip()


def exact_sha() -> str:
    sha = git('rev-parse', 'HEAD')
    if not SHA.fullmatch(sha) or (os.environ.get('GITHUB_SHA') and os.environ['GITHUB_SHA'] != sha):
        raise ValueError('Build checkout does not match its exact source SHA')
    return sha


def complete_source(destination: Path) -> Path:
    sha = exact_sha()
    # Staging adds only untracked generated inputs. A changed tracked file means
    # the binary and archived source no longer describe the same completed SHA.
    if git('diff', '--name-only', sha):
        raise ValueError('Complete source refuses a modified tracked build input')
    names = subprocess.check_output(['git', '-C', str(ROOT), 'ls-tree', '-r', '--name-only', '-z', sha],
                                    encoding='utf-8', errors='strict', timeout=30).rstrip('\0').split('\0')
    for name in names:
        public_path(name)
    required = {'LICENSE', 'README.md', 'docs/licensing.md', 'engine/pyproject.toml',
                'desktop/src-tauri/Cargo.lock', 'dashboard/package-lock.json',
                'scripts/build-desktop.ps1', '.github/workflows/build-desktop.yml'}
    if not required <= set(names):
        raise ValueError('Complete source is missing license, dependencies or build instructions')
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / f'numera-source-{sha}.tar.gz'
    temporary = archive.with_suffix('.part')
    try:
        with temporary.open('wb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as packed:
            # Bound archive generation before compression; no blocking pipe copy
            # can silently outlive the command timeout.
            result = subprocess.run(['git', '-C', str(ROOT), 'archive', '--format=tar',
                                     f'--prefix=numera-source-{sha}/', sha],
                                    stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                    check=True, timeout=30)
            packed.write(result.stdout)
        temporary.replace(archive)
    finally:
        temporary.unlink(missing_ok=True)
    with tarfile.open(archive, 'r:gz') as source:
        prefix = f'numera-source-{sha}/'
        for member in source.getmembers():
            if member.name.rstrip('/') == prefix.rstrip('/'):
                continue
            if not member.name.startswith(prefix):
                raise ValueError('Complete source archive has an unexpected root')
            public_path(member.name[len(prefix):])
            if member.issym() or member.islnk():
                raise ValueError('Source publication refuses unchecked symbolic links')
    return archive


def copy_notices(destination: Path) -> None:
    for source, target in NOTICES.items():
        original = ROOT / source
        if not original.is_file() or original.is_symlink():
            raise ValueError(f'Required license or notice is missing: {source}')
        output = destination / target
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(original, output)


def stage_wheel() -> None:
    license_path = ROOT / 'engine/LICENSE'
    legal_path = ROOT / 'engine/numera/legal'
    if license_path.exists() or legal_path.exists():
        raise ValueError('Refusing to overwrite existing distribution staging')
    shutil.copyfile(ROOT / 'LICENSE', license_path)
    copy_notices(legal_path)


def clear_wheel_staging() -> None:
    license_path = ROOT / 'engine/LICENSE'
    legal_path = ROOT / 'engine/numera/legal'
    if license_path.is_file() and license_path.read_bytes() == (ROOT / 'LICENSE').read_bytes():
        license_path.unlink()
    elif license_path.exists():
        raise ValueError('Refusing to remove changed staged LICENSE')
    if legal_path.is_dir():
        expected = {PurePosixPath(name) for name in NOTICES.values()}
        actual = {PurePosixPath(path.relative_to(legal_path).as_posix())
                  for path in legal_path.rglob('*') if path.is_file()}
        if actual != expected or any((legal_path / target).read_bytes() != (ROOT / source).read_bytes()
                                     for source, target in NOTICES.items()):
            raise ValueError('Refusing to remove changed or unowned notice staging')
        shutil.rmtree(legal_path)


def validate_stage(stage: Path, sha: str) -> None:
    if not (stage / 'LICENSE').is_file():
        raise ValueError('Product archive is missing its full LICENSE')
    source = stage / 'sources' / f'numera-source-{sha}.tar.gz'
    if not source.is_file():
        raise ValueError('Product archive is missing matching complete source')
    browsers = stage / 'browsers'
    if browsers.is_dir():
        manifest = json.loads((browsers / 'manifest.json').read_text(encoding='utf-8'))
        if 'cloak' in manifest or (browsers / 'cloak').exists():
            raise ValueError('Cloak binary redistribution is not authorized')
    for name in NOTICES.values():
        if not (stage / name).is_file():
            raise ValueError(f'Product archive is missing a notice: {name}')
    for path in stage.rglob('*'):
        relative = path.relative_to(stage).as_posix()
        if update_state(relative.split('/')[0]):
            continue
        # Frozen SDK dependencies may intentionally contain node_modules. They
        # are runtime inputs, not the development directories excluded from Git source.
        safe_relative = '/'.join('sdk-runtime' if part == 'node_modules' else part
                                 for part in PurePosixPath(relative).parts)
        public_path(safe_relative)
        if path.is_symlink():
            resolved = path.resolve(strict=True)
            if not resolved.is_relative_to(stage.resolve()):
                raise ValueError('Product archive contains an escaping symbolic link')


def update_state(name: str) -> bool:
    return name == '.updates' or name == '.numera-update.lock' or name.startswith('.numera-update-')


def archive_stage(stage: Path, archive: Path) -> Path:
    archive.parent.mkdir(parents=True, exist_ok=True)
    if archive.exists():
        raise ValueError('Refusing to overwrite a completed distribution archive')
    if archive.name.endswith('.tar.gz'):
        with tarfile.open(archive, 'w:gz', dereference=False) as packed:
            def include(member):
                relative = PurePosixPath(member.name).parts[1:]
                return None if relative and update_state(relative[0]) else member
            packed.add(stage, arcname=stage.name, filter=include)
    elif archive.suffix == '.zip':
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as packed:
            for path in sorted(stage.rglob('*')):
                if update_state(path.relative_to(stage).parts[0]):
                    continue
                if path.is_file():
                    packed.write(path, path.relative_to(stage.parent).as_posix())
    else:
        raise ValueError('Unsupported distribution archive format')
    return archive


def dependencies() -> dict:
    return {item.metadata['Name']: item.version for item in importlib.metadata.distributions()
            if item.metadata.get('Name') and item.metadata['Name'] != 'numera-engine'}


def verify_architecture(stage: Path, platform: str) -> None:
    import struct
    shell = stage / ('numera.exe' if platform == 'windows-x64' else 'numera')
    with shell.open('rb') as stream:
        header = stream.read(64)
        if platform == 'windows-x64':
            if header[:2] != b'MZ' or len(header) < 64:
                raise ValueError('Windows shell is not a PE executable')
            stream.seek(struct.unpack_from('<I', header, 60)[0])
            pe = stream.read(6)
            valid = len(pe) == 6 and pe[:4] == b'PE\0\0' and struct.unpack_from('<H', pe, 4)[0] == 0x8664
        elif platform == 'linux-x64':
            valid = (header[:6] == b'\x7fELF\x02\x01'
                     and struct.unpack_from('<H', header, 18)[0] == 62)
        else:
            valid = (header[:4] == b'\xcf\xfa\xed\xfe'
                     and struct.unpack_from('<I', header, 4)[0] == 0x0100000C)
        if not valid:
            raise ValueError('Portable shell architecture differs from its download label')


def package(stage: Path, platform: str, output: Path) -> None:
    sha = exact_sha()
    if stage.is_symlink() or not stage.is_dir():
        raise ValueError('Product staging must be an existing real directory')
    if (stage / 'data').exists():
        raise ValueError('Product archive must never include application/user data')
    if platform != 'server':
        verify_architecture(stage, platform)
        from identity import verify as verify_identity
        verify_identity(stage)
    copy_notices(stage)
    source = complete_source(stage / 'sources')
    secret = os.environ.pop('NUMERA_RELEASE_SIGNING_KEY', '')
    provenance = {'schema': 1, 'source_sha': sha, 'platform': platform,
                  'application_runtime': 'retained-python', 'native_candidate': 'unfinished-not-selected',
                  'signing': 'signed-metadata-private-native-trust-not-notarized',
                  'signing_scope': os.environ.get('NUMERA_SIGNING_SCOPE', 'owner'), 'python': sys.version.split()[0],
                  'dependencies': dependencies(), 'source': asset_record(source),
                  'host_prerequisites': {
                      'windows-x64': 'Windows with Microsoft WebView2 runtime',
                      'linux-x64': 'Compatible glibc and WebKitGTK4.1/GTK3 system libraries',
                      'macos-arm64': 'Apple Silicon macOS; self-signed/not-notarized download may need Gatekeeper approval',
                      'server': 'Python and explicitly selected dependencies; browsers not bundled',
                  }[platform]}
    if platform != 'server':
        from third_party import browser_sources
        provenance['third_party'] = browser_sources(stage)
    write_json(stage / 'PROVENANCE.json', provenance)
    from signing import fixture_ed25519, installed_manifest, sign_manifest

    public_key = ROOT / 'docs/release-signing-key.pem'
    if os.environ.get('NUMERA_SIGNING_SCOPE') == 'validation-only':
        public_key = stage / 'VALIDATION-KEY.pem'
        fixture_ed25519(public_key)
    else:
        os.environ['NUMERA_RELEASE_SIGNING_KEY'] = secret
    installed_manifest(stage, platform, sha)
    try:
        sign_manifest(stage / 'BUILD.json', stage / 'BUILD.sig', public_key)
    finally:
        os.environ.pop('NUMERA_RELEASE_SIGNING_KEY', None)
    validate_stage(stage, sha)
    extension = '.zip' if platform == 'windows-x64' else '.tar.gz'
    output.mkdir(parents=True, exist_ok=True)
    archive = archive_stage(stage, output / f'numera-{platform}-{sha}{extension}')
    build = json.loads((stage / 'BUILD.json').read_text(encoding='utf-8'))
    write_json(output / f'{platform}.json', {'schema': 1, 'source_sha': sha,
                                           'platform': platform, 'asset': asset_record(archive),
                                           'build_manifest_sha256': digest(stage / 'BUILD.json'),
                                           'expanded_size': sum(path.stat().st_size for path in stage.rglob('*') if path.is_file()),
                                           'max_members': sum(1 for _ in stage.rglob('*')) + 1,
                                           'components': build['components'],
                                           'signing_scope': os.environ.get('NUMERA_SIGNING_SCOPE', 'owner')})


def verify_manifest(directory: Path, manifest: dict, sha: str, platform_metadata: bool = False) -> None:
    if not SHA.fullmatch(sha) or manifest.get('schema') != 1 or manifest.get('source_sha') != sha:
        raise ValueError('Distribution manifest source SHA does not match')
    expected = set()
    for asset in manifest.get('assets', []):
        name = asset.get('name', '')
        if not ASSET.fullmatch(name) or '/' in name or chr(92) in name:
            raise ValueError('Invalid distribution asset name')
        if name in expected:
            raise ValueError('Distribution manifest has duplicate asset names')
        expected.add(name)
        actual = directory / name
        if not actual.is_file() or actual.is_symlink() or asset_record(actual) != asset:
            raise ValueError(f'Distribution checksum or size mismatch: {name}')
    if not expected:
        raise ValueError('Distribution manifest has no assets')
    present = {path.name for path in directory.iterdir()}
    metadata = {f'{platform}.json' for platform in (*PLATFORMS, 'server')} if platform_metadata else set()
    if present - expected - {'distribution.json', 'SHA256SUMS'} - metadata:
        raise ValueError('Distribution contains unexpected files')


def manifest(directory: Path) -> None:
    sha = exact_sha()
    records = []
    for platform in (*PLATFORMS, 'server'):
        metadata = json.loads((directory / f'{platform}.json').read_text(encoding='utf-8'))
        if metadata.get('source_sha') != sha or metadata.get('platform') != platform:
            raise ValueError('Platform archive metadata has the wrong source SHA or platform')
        records.append(metadata['asset'])
    source = complete_source(directory)
    records.append(asset_record(source))
    result = {'schema': 1, 'source_sha': sha, 'application_runtime': 'retained-python',
              'release_channel': 'verified-build-prerelease', 'platforms': list(PLATFORMS),
              'assets': records}
    verify_manifest(directory, result, sha, platform_metadata=True)
    write_json(directory / 'distribution.json', result)
    (directory / 'SHA256SUMS').write_text(''.join(f"{item['sha256']}  {item['name']}\n"
                                               for item in records), encoding='utf-8')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('stage-wheel')
    sub.add_parser('clear-wheel-staging')
    pack = sub.add_parser('package')
    pack.add_argument('--stage', type=Path, required=True)
    pack.add_argument('--platform', choices=(*PLATFORMS, 'server'), required=True)
    pack.add_argument('--output', type=Path, required=True)
    collect = sub.add_parser('manifest')
    collect.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    with run_log('distribution') as logger:
        logger.info('Packaging operation=%s source_sha=%s', args.command, exact_sha())
        if args.command == 'stage-wheel':
            stage_wheel()
        elif args.command == 'clear-wheel-staging':
            clear_wheel_staging()
        elif args.command == 'package':
            package(args.stage.resolve(), args.platform, args.output.resolve())
        else:
            manifest(args.directory.resolve())
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
