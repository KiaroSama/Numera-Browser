"""Retain exact GPL/MPL source offers without redistributing Cloak's binary."""
from __future__ import annotations

import hashlib
import importlib.metadata
import importlib.util
import json
import logging
import os
import platform
import sys
from pathlib import Path
import re
import tarfile
import time
from urllib.parse import quote, urlparse
from urllib.request import Request, urlopen

PUBLIC_HOSTS = {'api.github.com', 'codeload.github.com', 'raw.githubusercontent.com',
                'archive.mozilla.org'}
MAX_SOURCE_BYTES = 512 * 1024 * 1024


def read_url(url: str, *, limit: int = 4 * 1024 * 1024) -> bytes:
    parsed = urlparse(url)
    if parsed.scheme != 'https' or parsed.hostname not in PUBLIC_HOSTS or parsed.username:
        raise ValueError('Untrusted corresponding-source URL')
    headers = {'Accept': 'application/vnd.github+json', 'User-Agent': 'Numera-build-source'}
    token = os.environ.get('GITHUB_TOKEN')
    if token and parsed.hostname == 'api.github.com':
        headers['Authorization'] = f'Bearer {token}'
    request = Request(url, headers=headers)
    with urlopen(request, timeout=30) as response:
        if urlparse(response.url).hostname not in PUBLIC_HOSTS:
            raise ValueError('Untrusted corresponding-source redirect')
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError('Corresponding-source response exceeds its limit')
    return data


def api(path: str) -> dict:
    return json.loads(read_url(f'https://api.github.com/repos/{path}').decode('utf-8'))


def source_archive(repository: str, tag: str, destination: Path) -> dict:
    if repository not in {'daijro/camoufox', 'camoufox/camoufox', 'gorhill/uBlock'}:
        raise ValueError('Unsupported corresponding-source repository')
    ref = api(f'{repository}/git/ref/tags/{quote(tag, safe="")}')['object']
    for _ in range(4):
        if ref['type'] == 'commit':
            break
        if ref['type'] != 'tag':
            raise ValueError('Source tag does not resolve to a commit')
        ref = api(f'{repository}/git/tags/{ref["sha"]}')['object']
    else:
        raise ValueError('Source tag nesting exceeds its limit')
    sha = ref['sha']
    if not re.fullmatch(r'[0-9a-f]{40}', sha):
        raise ValueError('Invalid corresponding-source commit')
    url = f'https://codeload.github.com/{repository}/tar.gz/{sha}'
    filename = f'{repository.split("/")[-1]}-{sha}.tar.gz'
    path = destination / filename
    if path.exists():
        raise ValueError('Corresponding-source archive already exists')
    logging.info('Downloading corresponding source repository=%s tag=%s commit=%s', repository, tag, sha)
    deadline = time.monotonic() + 180
    total = 0
    hasher = hashlib.sha256()
    try:
        with urlopen(Request(url, headers={'User-Agent': 'Numera-build-source'}), timeout=30) as response:
            if urlparse(response.url).hostname != 'codeload.github.com':
                raise ValueError('Untrusted source archive redirect')
            with path.open('xb') as output:
                while block := response.read(1024 * 1024):
                    total += len(block)
                    if total > MAX_SOURCE_BYTES or time.monotonic() >= deadline:
                        raise TimeoutError('Corresponding-source archive exceeds size/time limit')
                    hasher.update(block)
                    output.write(block)
        with tarfile.open(path, 'r:gz') as source:
            names = source.getnames()
            from pathlib import PurePosixPath
            for member in source.getmembers():
                name = PurePosixPath(member.name)
                # Upstream's public developer docs are not our private assistant
                # state. Preserve complete public source but reject traversal,
                # credential files and local working-state directories.
                if (name.is_absolute() or '..' in name.parts or chr(92) in member.name
                        or any(part in {'.ai', '.git', '.env', 'secrets.md', '.ignoreme'} for part in name.parts)
                        or name.name.startswith('.env.')):
                    raise ValueError('Corresponding-source archive contains an unsafe private path')
                if member.issym() or member.islnk():
                    raise ValueError('Corresponding-source archive has an unchecked link')
            if not any(name.endswith('/LICENSE') or name.endswith('/LICENSE.txt') for name in names):
                raise ValueError('Corresponding-source archive is missing its license')
            if repository == 'gorhill/uBlock' and not any(name.endswith('/tools/make-firefox.sh') for name in names):
                raise ValueError('uBlock corresponding source is missing its build instructions')
            if repository != 'gorhill/uBlock' and not any(name.endswith('/Makefile') for name in names):
                raise ValueError('Camoufox corresponding source is missing its patch/build instructions')
    except BaseException:
        path.unlink(missing_ok=True)
        raise
    return {'repository': repository, 'tag': tag, 'commit': sha, 'source_url': url,
            'archive': filename, 'sha256': hasher.hexdigest(), 'bytes': total}


def browser_sources(stage: Path) -> list[dict]:
    root = stage / 'browsers'
    manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8'))
    if 'cloak' in manifest or (root / 'cloak').exists():
        raise ValueError('Cloak binary redistribution is not authorized')
    destination = stage / 'sources/third-party'
    destination.mkdir(parents=True, exist_ok=True)
    metadata = json.loads((root / 'camoufox/version.json').read_text(encoding='utf-8'))
    version, build = metadata.get('version'), metadata.get('build', metadata.get('release'))
    if not re.fullmatch(r'\d+(?:\.\d+){1,3}', str(version)) or not re.fullmatch(r'[a-z0-9.]+', str(build)):
        raise ValueError('Packaged Camoufox has no exact browser version/build')
    tag = f'v{version}-{build}'
    release = api(f'daijro/camoufox/releases/tags/{quote(tag, safe="")}')
    os_name = {'win32': 'win', 'darwin': 'mac', 'linux': 'lin'}[sys.platform]
    machine = platform.machine().lower()
    arch = 'arm64' if machine in {'arm64', 'aarch64'} else 'x86_64'
    name = f'camoufox-{version}-{build}-{os_name}.{arch}.zip'
    asset = next((item for item in release['assets'] if item['name'] == name), None)
    expected = metadata.get('sha256')
    if not asset or not expected or asset.get('digest') != f'sha256:{expected}':
        raise ValueError('Packaged Camoufox lacks matching official release digest evidence')
    camoufox = source_archive('daijro/camoufox', tag, destination)
    camoufox_executable = root / manifest['camoufox']['executable']
    if not camoufox_executable.resolve().is_relative_to(root.resolve()):
        raise ValueError('Packaged Camoufox executable escapes resources')
    from distribution import digest
    if digest(camoufox_executable) != manifest['camoufox']['sha256']:
        raise ValueError('Packaged Camoufox executable differs from its manifest')
    camoufox.update(component='Camoufox', license='MPL-2.0', version=f'{version}-{build}',
                    binary_asset=name, binary_sha256=expected,
                    upstream_source=f'https://archive.mozilla.org/pub/firefox/releases/{version}/source/firefox-{version}.source.tar.xz',
                    instructions='Apply included Camoufox patches to the matching Mozilla Firefox source using the included Makefile.')
    # MPL source can be hosted upstream with clear directions. Verify access now;
    # do not download a multi-gigabyte Firefox tree into each portable archive.
    request = Request(camoufox['upstream_source'], method='HEAD', headers={'User-Agent': 'Numera-build-source'})
    with urlopen(request, timeout=30) as response:
        if response.status != 200 or urlparse(response.url).hostname != 'archive.mozilla.org':
            raise ValueError('Matching Firefox complete-source offer is unavailable')
    offers = [camoufox]
    for relative in manifest['camoufox']['addons']:
        addon = root / relative
        if not addon.resolve().is_relative_to(root.resolve()):
            raise ValueError('Packaged addon source path escapes browser resources')
        data = json.loads((addon / 'manifest.json').read_text(encoding='utf-8'))
        addon_id = data.get('browser_specific_settings', data.get('applications', {})).get('gecko', {}).get('id')
        if addon_id != 'uBlock0@raymondhill.net':
            raise ValueError('Unknown distributed addon needs a corresponding-source policy')
        addon_version = data.get('version', '')
        if not re.fullmatch(r'\d+(?:\.\d+){1,3}(?:b\d+)?', addon_version):
            raise ValueError('Packaged uBlock has no exact source version')
        source = source_archive('gorhill/uBlock', addon_version, destination)
        source.update(component='uBlock Origin', version=addon_version, license='GPL-3.0',
                      packaged_path=relative,
                      instructions='Matching upstream source includes tools/make-firefox.sh; bundled addon retains its assets and independent license.')
        license_url = f'https://raw.githubusercontent.com/gorhill/uBlock/{source["commit"]}/LICENSE.txt'
        (addon / 'LICENSE.txt').write_bytes(read_url(license_url))
        offers.append(source)
    camoufox_license = read_url(f'https://raw.githubusercontent.com/daijro/camoufox/{camoufox["commit"]}/LICENSE')
    (root / 'camoufox/MPL-2.0.txt').write_bytes(camoufox_license)
    for backend in ('playwright', 'patchright'):
        entry = manifest[backend]
        executable = root / entry['executable']
        if not executable.resolve().is_relative_to(root.resolve()) or not executable.is_file():
            raise ValueError('Bundled browser executable is missing or escapes resources')
        if digest(executable) != entry['sha256']:
            raise ValueError('Bundled browser executable hash differs from its manifest')
        package_version = importlib.metadata.version(backend)
        package_root = Path(importlib.util.find_spec(backend).origin).parent
        registry = json.loads((package_root / 'driver/package/browsers.json').read_text(encoding='utf-8'))
        chromium = next(item for item in registry['browsers'] if item['name'] == 'chromium')
        notices = [path for path in (root / backend).rglob('*')
                   if path.is_file() and ('license' in path.name.lower() or 'credits' in path.name.lower())]
        if not notices:
            raise ValueError('Bundled Chromium has no component copyright/license notices')
        offers.append({'component': backend, 'package_version': package_version,
                       'browser_version': chromium.get('browserVersion'), 'browser_revision': chromium['revision'],
                       'chromium_source': 'https://chromium.googlesource.com/chromium/src/+/' + chromium['browserVersion'],
                       'binary_sha256': entry['sha256'], 'license': 'Apache-2.0 and independent Chromium component terms',
                       'source_url': 'https://github.com/microsoft/playwright' if backend == 'playwright' else
                                     'https://github.com/Kaliiiiiiiiii-Vinyzu/patchright',
                       'notices': [path.relative_to(root).as_posix() for path in notices]})
    (destination / 'SOURCE-OFFERS.json').write_text(json.dumps(offers, indent=2) + '\n', encoding='utf-8')
    return offers
