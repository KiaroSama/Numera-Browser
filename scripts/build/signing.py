"""Sign exact release metadata and native owned executables without exporting secrets."""
from __future__ import annotations

import argparse
import base64
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log

OWNER = 'KiaroSama'
REPOSITORY = 'KiaroSama/Numera-Browser'
PRODUCT = 'Numera Browser'
MAX_MANIFEST = 1024 * 1024
MAX_BUILD_MANIFEST = 8 * 1024 * 1024


def execute(command: list[str], *, env=None, data: bytes | None = None, seconds: int = 60) -> bytes:
    child_env = dict(os.environ if env is None else env)
    for name in ('NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX', 'NUMERA_CODE_SIGNING_PASSWORD'):
        if not (name == 'NUMERA_CODE_SIGNING_PASSWORD' and env is not None
                and Path(command[0]).name in {'pwsh', 'pwsh.exe'}):
            child_env.pop(name, None)
    try:
        result = subprocess.run(command, input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                env=child_env, timeout=seconds, check=False)
    except subprocess.TimeoutExpired as error:
        raise ValueError(f'Signing tool exceeded its deadline: {Path(command[0]).name}') from None
    if result.returncode:
        # Native diagnostics can include imported key/certificate details. Only
        # the tool identity and exit code cross the signing boundary.
        raise ValueError(f'Signing tool failed: {Path(command[0]).name} exit={result.returncode}')
    return result.stdout


@contextmanager
def private_directory():
    root = Path(os.environ.get('RUNNER_TEMP', str(ROOT / '.ci-work')))
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='numera-sign-', dir=root) as name:
        path = Path(name)
        path.chmod(0o700)
        if os.name == 'nt':
            identity = execute(['whoami']).decode('utf-8', errors='strict').strip()
            execute(['icacls', str(path), '/inheritance:r', '/grant:r', identity + ':(OI)(CI)F'])
        yield path


def fixture_ed25519(path: Path) -> None:
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

    key = Ed25519PrivateKey.generate()
    os.environ['NUMERA_RELEASE_SIGNING_KEY'] = key.private_bytes(
        serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption()).decode('ascii')
    path.write_bytes(key.public_key().public_bytes(serialization.Encoding.PEM,
                                                   serialization.PublicFormat.SubjectPublicKeyInfo))


def public_ed25519(path: Path):
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

    key = serialization.load_pem_public_key(path.read_bytes())
    if not isinstance(key, Ed25519PublicKey):
        raise ValueError('Release verification key must be Ed25519')
    return key


def verify_manifest(manifest: Path, signature: Path, public_key: Path) -> None:
    from cryptography.exceptions import InvalidSignature

    message, proof = manifest.read_bytes(), signature.read_bytes()
    limit = MAX_BUILD_MANIFEST if manifest.name == 'BUILD.json' else MAX_MANIFEST
    if not message or len(message) > limit or len(proof) != 64:
        raise ValueError('Invalid release manifest or raw signature size')
    try:
        public_ed25519(public_key).verify(proof, message)
    except InvalidSignature as error:
        raise ValueError('Release signature does not match the pinned signer and exact manifest') from error


def require_owner_context() -> None:
    if os.environ.get('GITHUB_ACTIONS') == 'true':
        if (os.environ.get('GITHUB_EVENT_NAME') != 'workflow_dispatch' or os.environ.get('GITHUB_ACTOR') != OWNER
                or os.environ.get('GITHUB_REF') not in {'refs/heads/main', 'refs/heads/migration/native-runtime-checkpoint-20261012'}
                or os.environ.get('NUMERA_TRUSTED_SHA') != os.environ.get('GITHUB_SHA')):
            raise ValueError('Owner signing requires the exact approved manual source revision')


def sign_manifest(manifest: Path, signature: Path, public_key: Path) -> None:
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

    message = manifest.read_bytes()
    limit = MAX_BUILD_MANIFEST if manifest.name == 'BUILD.json' else MAX_MANIFEST
    if not message or len(message) > limit or signature.exists():
        raise ValueError('Manifest must be bounded and signature destination new')
    if public_key.resolve() == (ROOT / 'docs/release-signing-key.pem').resolve():
        require_owner_context()
    secret = os.environ.get('NUMERA_RELEASE_SIGNING_KEY', '')
    if not secret:
        raise ValueError('Protected release signing input is missing')
    try:
        key = serialization.load_pem_private_key(secret.encode('utf-8'), password=None)
    except (ValueError, TypeError):
        raise ValueError('Protected release key could not be decoded') from None
    if not isinstance(key, Ed25519PrivateKey):
        raise ValueError('Release signing input must be Ed25519 PKCS8 PEM')
    raw = serialization.Encoding.Raw
    public_format = serialization.PublicFormat.Raw
    if key.public_key().public_bytes(raw, public_format) != public_ed25519(public_key).public_bytes(raw, public_format):
        raise ValueError('Protected release key differs from pinned public signer')
    with private_directory() as private:
        key_path = private / 'release-key.pem'
        key_path.write_text(secret, encoding='utf-8')
        key_path.chmod(0o600)
        env = os.environ.copy()
        for name in ('NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX', 'NUMERA_CODE_SIGNING_PASSWORD'):
            env.pop(name, None)
        execute(['openssl', 'pkeyutl', '-sign', '-rawin', '-inkey', str(key_path),
                 '-in', str(manifest), '-out', str(signature)], env=env)
    verify_manifest(manifest, signature, public_key)


def certificate(path: Path):
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes
    from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

    cert = x509.load_pem_x509_certificate(path.read_bytes())
    names = cert.subject.get_attributes_for_oid(NameOID.COMMON_NAME)
    if len(names) != 1 or names[0].value != OWNER:
        raise ValueError('Code signing certificate owner differs from approved identity')
    now = datetime.now(timezone.utc)
    if not cert.not_valid_before_utc <= now <= cert.not_valid_after_utc:
        raise ValueError('Code signing certificate is not currently valid')
    if ExtendedKeyUsageOID.CODE_SIGNING not in cert.extensions.get_extension_for_class(x509.ExtendedKeyUsage).value:
        raise ValueError('Code signing certificate lacks its required usage')
    return cert, cert.fingerprint(hashes.SHA256()).hex(), cert.fingerprint(hashes.SHA1()).hex()


def native_paths(stage: Path, platform: str) -> list[Path]:
    names = ('numera.exe', 'engine/numera-engine.exe') if platform == 'windows-x64' else ('numera', 'engine/numera-engine')
    paths = [stage / name for name in names]
    if any(not path.is_file() or path.is_symlink() for path in paths):
        raise ValueError('Owned shell or frozen engine signing input is missing or linked')
    return paths


def fixture_certificate(path: Path, key_path: Path, password: bytes) -> bytes:
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    from cryptography.hazmat.primitives.serialization import pkcs12
    from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

    key = rsa.generate_private_key(public_exponent=65537, key_size=3072)
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, OWNER)])
    now = datetime.now(timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
            .serial_number(x509.random_serial_number()).not_valid_before(now - timedelta(minutes=1))
            .not_valid_after(now + timedelta(days=1))
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CODE_SIGNING]), critical=False)
            .sign(key, hashes.SHA256()))
    path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
    payload = pkcs12.serialize_key_and_certificates(b'Numera CI fixture', key, cert, None,
                                                   serialization.BestAvailableEncryption(password))
    key_path.write_bytes(payload)
    key_path.chmod(0o600)
    return payload


def native_sign(stage: Path, platform: str, public_certificate: Path, fixture: bool = False) -> None:
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.serialization import pkcs12

    if platform == 'linux-x64':
        return  # Linux has no embedded raw ELF signing format; signed metadata binds exact bytes.
    with private_directory() as private:
        pfx = private / 'identity.pfx'
        env = os.environ.copy()
        cert_path = public_certificate
        if fixture:
            password = base64.b64encode(os.urandom(24)).decode('ascii')
            cert_path = private / 'fixture-cert.pem'
            fixture_certificate(cert_path, pfx, password.encode('utf-8'))
        else:
            require_owner_context()
            password = env.get('NUMERA_CODE_SIGNING_PASSWORD', '')
            payload = env.get('NUMERA_CODE_SIGNING_PFX', '')
            if not password or not payload:
                raise ValueError('Protected native signing inputs are missing')
            pfx.write_bytes(base64.b64decode(payload, validate=True))
            pfx.chmod(0o600)
        cert, fingerprint, sha1 = certificate(cert_path)
        try:
            key, actual, chain = pkcs12.load_key_and_certificates(pfx.read_bytes(), password.encode('utf-8'))
        except (ValueError, TypeError):
            raise ValueError('Protected native signing input could not be decoded') from None
        if key is None or actual is None or actual.fingerprint(hashes.SHA256()).hex() != fingerprint:
            raise ValueError('Protected certificate differs from pinned approved code signer')
        paths = native_paths(stage, platform)
        for name in ('NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX'):
            env.pop(name, None)
        if platform == 'windows-x64':
            from identity import identity
            env.update(NUMERA_CODE_SIGNING_PASSWORD=password, NUMERA_SIGN_PFX=str(pfx),
                       NUMERA_SIGN_FINGERPRINT=fingerprint, NUMERA_SIGN_STAGE=str(stage),
                       NUMERA_SIGN_VERSION=identity()['version'])
            execute(['pwsh', '-NoProfile', '-NonInteractive', '-File', str(ROOT / 'scripts/build/windows_signing.ps1')], env=env, seconds=180)
        elif platform == 'macos-arm64':
            # Private transport has an empty password inside a mode0700 directory;
            # the real protected PFX password never appears in process arguments.
            transport = private / 'transport.p12'
            transport.write_bytes(pkcs12.serialize_key_and_certificates(b'KiaroSama', key, actual, chain,
                                                                        serialization.NoEncryption()))
            transport.chmod(0o600)
            keychain = private / 'signing.keychain-db'
            execute(['security', 'create-keychain', '-p', '', str(keychain)])
            try:
                execute(['security', 'unlock-keychain', '-p', '', str(keychain)])
                execute(['security', 'import', str(transport), '-k', str(keychain), '-P', '', '-T', '/usr/bin/codesign'])
                execute(['security', 'set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:', '-s', '-k', '', str(keychain)])
                for path in paths:
                    execute(['codesign', '--force', '--sign', sha1, '--keychain', str(keychain), '--timestamp=none', str(path)])
                    execute(['codesign', '--verify', '--strict', '--requirement', f'certificate leaf = H"{sha1}"', str(path)])
                for index, path in enumerate(paths):
                    tampered = private / f'tampered-{index}'
                    shutil.copyfile(path, tampered)
                    with tampered.open('r+b') as stream:
                        stream.seek(64)
                        original = stream.read(1)
                        if not original:
                            raise ValueError('Native executable is too small for a tamper check')
                        stream.seek(64)
                        stream.write(bytes([original[0] ^ 1]))
                    rejected = False
                    try:
                        execute(['codesign', '--verify', '--strict', '--requirement', f'certificate leaf = H"{sha1}"', str(tampered)])
                    except ValueError:
                        rejected = True
                    if not rejected:
                        raise ValueError('Tampered native executable was accepted')
            finally:
                execute(['security', 'delete-keychain', str(keychain)])
        else:
            raise ValueError('Native signing platform is unsupported')
        metadata = {'owner': OWNER, 'certificate_sha256': fingerprint, 'platform': platform,
                    'trust': 'ephemeral-validation-only' if fixture else 'owner-self-signed-private-trust',
                    'notarized': False, 'publicly_trusted': False}
        (stage / 'SIGNING.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')


def installed_manifest(stage: Path, platform: str, source_sha: str) -> None:
    from distribution import dependencies, digest, public_path, update_state

    generation = int(os.environ.get('GITHUB_RUN_ID', '0'))
    if generation <= 0:
        raise ValueError('Installed build generation requires the actual hosted run identity')
    if platform in ('windows-x64', 'macos-arm64'):
        native = json.loads((stage / 'SIGNING.json').read_text(encoding='utf-8'))
        expected = ('ephemeral-validation-only' if os.environ.get('NUMERA_SIGNING_SCOPE') == 'validation-only'
                    else 'owner-self-signed-private-trust')
        if native['trust'] != expected:
            raise ValueError('Native signing trust scope differs from the requested build')
    files = []
    for path in sorted(stage.rglob('*')):
        relative = path.relative_to(stage).as_posix()
        if update_state(relative.split('/')[0]):
            continue
        if relative in {'BUILD.json', 'BUILD.sig'} or (path.is_dir() and not path.is_symlink()):
            continue
        safe = '/'.join('sdk-runtime' if part == 'node_modules' else part for part in Path(relative).parts)
        public_path(safe)
        if path.is_symlink():
            target = os.readlink(path)
            if Path(target).is_absolute() or not path.resolve(strict=True).is_relative_to(stage.resolve()):
                raise ValueError('Immutable build link escapes its owned installation root')
            files.append({'path': relative, 'link': target})
        elif path.is_file():
            files.append({'path': relative, 'size': path.stat().st_size, 'sha256': digest(path),
                          'executable': bool(path.stat().st_mode & 0o111)})
        else:
            raise ValueError('Immutable build contains a non-file resource')
    browsers = stage / 'browsers/manifest.json'
    result = {'schema': 1, 'owner': OWNER, 'product': PRODUCT,
              'version': json.loads((ROOT / 'dashboard/package.json').read_text(encoding='utf-8'))['version'],
              'source_sha': source_sha, 'generation': generation, 'platform': platform,
              'minimum_protocol': 1,
              'components': {'sdk': dependencies(),
                             'browsers': json.loads(browsers.read_text(encoding='utf-8')) if browsers.is_file() else {}},
              'files': files}
    content = (json.dumps(result, separators=(',', ':'), ensure_ascii=False) + '\n').encode('utf-8')
    if len(content) > MAX_BUILD_MANIFEST:
        raise ValueError('Installed immutable file inventory exceeds its signed manifest bound')
    (stage / 'BUILD.json').write_bytes(content)


def validate_archive_build(archive: Path, metadata: dict, platform: str, generation: int, source_sha: str) -> None:
    import tarfile
    import zipfile

    wanted = {PRODUCT + '/BUILD.json': MAX_BUILD_MANIFEST, PRODUCT + '/BUILD.sig': 64}
    contents = {}
    expanded = members = 0
    if archive.suffix == '.zip':
        with zipfile.ZipFile(archive) as packed:
            members = len(packed.infolist())
            expanded = sum(entry.file_size for entry in packed.infolist())
            for name, limit in wanted.items():
                matches = [entry for entry in packed.infolist() if entry.filename == name]
                if len(matches) != 1 or matches[0].file_size > limit:
                    raise ValueError('Packaged signed inventory is missing, duplicated or oversized')
                contents[name] = packed.read(matches[0])
    else:
        with tarfile.open(archive, 'r:gz') as packed:
            for entry in packed:
                members += 1
                if entry.isfile():
                    expanded += entry.size
                if entry.name not in wanted:
                    continue
                if entry.name in contents or not entry.isfile() or entry.size > wanted[entry.name]:
                    raise ValueError('Packaged signed inventory is duplicated, linked or oversized')
                stream = packed.extractfile(entry)
                if stream is None:
                    raise ValueError('Packaged signed inventory cannot be read')
                with stream:
                    contents[entry.name] = stream.read(wanted[entry.name] + 1)
    if members > metadata['max_members'] or expanded > metadata['expanded_size']:
        raise ValueError('Packaged archive exceeds its signed expansion bounds')
    if set(contents) != set(wanted):
        raise ValueError('Packaged signed inventory is missing')
    message, signature = contents[PRODUCT + '/BUILD.json'], contents[PRODUCT + '/BUILD.sig']
    if len(signature) != 64 or hashlib.sha256(message).hexdigest() != metadata['build_manifest_sha256']:
        raise ValueError('Packaged build inventory hash/signature differs from verified metadata')
    try:
        public_ed25519(ROOT / 'docs/release-signing-key.pem').verify(signature, message)
    except Exception as error:
        raise ValueError('Packaged build inventory is not authenticated by the owner') from None
    build = json.loads(message.decode('utf-8'))
    if (build['generation'] != generation or build['source_sha'] != source_sha
            or build['platform'] != platform or build['components'] != metadata['components']):
        raise ValueError('Packaged build generation, source or SDK compatibility differs')


def release_manifest(directory: Path, destination: Path, generation: int, source_sha: str) -> None:
    if generation <= 0 or not re.fullmatch('[0-9a-f]{40}', source_sha):
        raise ValueError('Signed release generation and source identity are invalid')
    distribution = json.loads((directory / 'distribution.json').read_text(encoding='utf-8'))
    if distribution['source_sha'] != source_sha:
        raise ValueError('Distribution and signed metadata source differ')
    from distribution import PLATFORMS, verify_manifest as verify_distribution

    verify_distribution(directory, distribution, source_sha, platform_metadata=True)
    assets, components = [], {}
    for platform in PLATFORMS:
        metadata = json.loads((directory / (platform + '.json')).read_text(encoding='utf-8'))
        if (metadata.get('source_sha') != source_sha or metadata.get('platform') != platform
                or metadata.get('signing_scope') != 'owner'):
            raise ValueError('Update component source/platform identity or owner signing scope differs')
        asset = dict(metadata['asset'])
        expected_asset = next((row for row in distribution['assets'] if row['name'] == asset.get('name')), None)
        if asset != expected_asset:
            raise ValueError('Update archive receipt differs from the exact verified distribution')
        asset.update(platform=platform, archive_root=PRODUCT,
                     expanded_size=metadata['expanded_size'], max_members=metadata['max_members'],
                     build_manifest_sha256=metadata['build_manifest_sha256'])
        assets.append(asset)
        components[platform] = metadata['components']
        validate_archive_build(directory / asset['name'], metadata, platform, generation, source_sha)
    now = datetime.now(timezone.utc).replace(microsecond=0)
    result = {'schema': 1, 'repository': REPOSITORY, 'product': PRODUCT,
              'version': json.loads((ROOT / 'dashboard/package.json').read_text(encoding='utf-8'))['version'],
              'source_sha': source_sha, 'generation': generation,
              'created_at': int(now.timestamp()),
              'expires_at': int((now + timedelta(days=90)).timestamp()),
              'minimum_protocol': 1, 'assets': assets, 'components': components}
    content = (json.dumps(result, separators=(',', ':'), ensure_ascii=False) + '\n').encode('utf-8')
    if len(content) > MAX_MANIFEST:
        raise ValueError('Signed release manifest exceeds its bound')
    destination.write_bytes(content)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='operation', required=True)
    native = sub.add_parser('native')
    native.add_argument('--stage', type=Path, required=True)
    native.add_argument('--platform', choices=('windows-x64', 'linux-x64', 'macos-arm64'), required=True)
    native.add_argument('--fixture', action='store_true')
    for operation in ('sign', 'verify'):
        command = sub.add_parser(operation)
        command.add_argument('--manifest', type=Path, required=True)
        command.add_argument('--signature', type=Path, required=True)
    manifest = sub.add_parser('manifest')
    manifest.add_argument('--directory', type=Path, required=True)
    manifest.add_argument('--destination', type=Path, required=True)
    manifest.add_argument('--generation', type=int, required=True)
    manifest.add_argument('--source-sha', required=True)
    args = parser.parse_args()
    with run_log('release-signing') as logger:
        logger.info('Signing operation=%s owner=%s', args.operation, OWNER)
        if args.operation == 'native':
            native_sign(args.stage.resolve(), args.platform, ROOT / 'docs/code-signing-cert.pem', args.fixture)
        elif args.operation == 'manifest':
            release_manifest(args.directory, args.destination, args.generation, args.source_sha)
        elif args.operation == 'sign':
            sign_manifest(args.manifest, args.signature, ROOT / 'docs/release-signing-key.pem')
        else:
            verify_manifest(args.manifest, args.signature, ROOT / 'docs/release-signing-key.pem')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
