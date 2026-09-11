"""Assemble desktop browser resources from separate vendor download caches.

Cloak's binary is local/internal only unless separately licensed. The default
bundle contains its wrapper and GeoIP data; the wrapper downloads its binary
directly for the end user. Never pass --include-cloak-binary in release CI.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import logging
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import uuid


def chromium_executable(backend: str, cache: Path) -> Path:
    package = Path(importlib.util.find_spec(backend).origin).parent
    registry = json.loads((package / 'driver/package/browsers.json').read_text(encoding='utf-8'))
    revision = next(item['revision'] for item in registry['browsers'] if item['name'] == 'chromium')
    folder = cache / backend / f'chromium-{revision}'
    names = {'chrome.exe'} if sys.platform == 'win32' else {'chrome', 'Chromium'}
    found = [path for path in folder.rglob('*') if path.is_file() and path.name in names]
    if len(found) != 1:
        raise FileNotFoundError(f'{backend} cache must contain exactly one Chromium executable')
    return found[0]


def camoufox_cache(root: Path):
    from camoufox import addons, geolocation, multiversion, pkgman, utils
    # Camoufox exposes no cache environment option on Windows; platformdirs
    # uses the known-folder API. Route its documented version manager's paths
    # together inside this build-only process, before importing its CLI.
    for module in (addons, multiversion, pkgman, utils):
        module.INSTALL_DIR = root
    for name, suffix in {'BROWSERS_DIR': 'browsers', 'CONFIG_FILE': 'config.json',
                         'REPO_CACHE_FILE': 'repo_cache.json', 'COMPAT_FLAG': '.0.5_FLAG'}.items():
        setattr(multiversion, name, root / suffix)
    addons.ADDONS_DIR = root / 'addons'
    geolocation.GEOIP_DIR = root / 'geoip'
    geolocation.MMDB_DIR = root / 'geoip/mmdb'
    geolocation.GEOIP_CONFIG = root / 'geoip/config.yml'
    return addons, geolocation, pkgman


def run(*args, env=None, timeout=600):
    from numera import proctree
    child, tree = proctree.spawn(list(args), env=env, stdin=subprocess.DEVNULL)
    try:
        code = child.wait(timeout=timeout)
        if code:
            raise RuntimeError(f'{Path(args[0]).name} failed with exit code {code}')
    finally:
        proctree.end_session(child, tree)


def copy_folder(source: Path, destination: Path):
    # Each destination is a newly created staging child; nothing is merged with
    # old vendor files or an application's portable data directory.
    shutil.copytree(source, destination)


def assemble(output: Path, cache: Path, include_cloak_binary=False):
    for directory in (output, *output.absolute().parents):
        if directory.exists():
            attributes = getattr(directory.lstat(), 'st_file_attributes', 0)
            if directory.is_symlink() or attributes & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0):
                raise ValueError(f'Refusing a browser output through a link or junction: {directory}')
    output = output.resolve()
    cache = cache.resolve()
    if output == cache or output in cache.parents or cache in output.parents:
        raise ValueError('Browser output and download cache must be separate directories')
    output.parent.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix='browser-bundle-', dir=output.parent))
    manifest = {}
    try:
        for backend in ('playwright', 'patchright'):
            logging.info('Downloading %s into its own cache', backend)
            env = {**os.environ, 'PLAYWRIGHT_BROWSERS_PATH': str(cache / backend)}
            run(sys.executable, '-m', backend, 'install', 'chromium', '--no-shell', env=env)
            executable = chromium_executable(backend, cache)
            if not executable.is_file():
                raise FileNotFoundError(f'{backend} download has no executable')
            relative = executable.relative_to(cache / backend)
            for directory in (cache / backend).iterdir():
                if directory.is_dir() and not directory.name.startswith('.'):
                    copy_folder(directory, staging / backend / directory.name)
            manifest[backend] = {'executable': (Path(backend) / relative).as_posix()}

        logging.info('Preparing Camoufox resources and default addons')
        run(sys.executable, str(Path(__file__).resolve()), '--fetch-camoufox', '--cache', str(cache))
        addons, geolocation, pkgman = camoufox_cache(cache / 'camoufox')
        browser_root = Path(pkgman.camoufox_path())
        executable = Path(pkgman.launch_path(browser_root))
        copy_folder(browser_root, staging / 'camoufox')
        addon_paths = []
        addons.add_default_addons(addon_paths)
        if len(addon_paths) != len(addons.DefaultAddons):
            raise RuntimeError('Camoufox default addons did not finish downloading')
        packaged_addons = []
        for addon in addon_paths:
            source = Path(addon)
            if not (source / 'manifest.json').is_file():
                raise FileNotFoundError('Camoufox default addon has no manifest')
            relative = Path('camoufox') / 'addons' / source.name
            copy_folder(source, staging / relative)
            packaged_addons.append(relative.as_posix())
        manifest['camoufox'] = {
            'executable': (Path('camoufox') / executable.relative_to(browser_root)).as_posix(),
            'version': pkgman.Version.from_path(browser_root).version,
            'addons': packaged_addons,
        }
        if geolocation.needs_update():
            geolocation.download_mmdb()
        copy_folder(geolocation.GEOIP_DIR, staging / 'camoufox/geoip')

        os.environ['CLOAKBROWSER_CACHE_DIR'] = str(cache / 'cloak')
        from cloakbrowser import config, download, geoip
        database = geoip._ensure_geoip_db()
        if database is None or not Path(database).is_file():
            raise RuntimeError('Cloak GeoIP database download failed')
        copy_folder(Path(database).parent, staging / 'cloak-data/geoip')
        manifest['_geoip'] = {'camoufox': 'camoufox/geoip', 'cloak': 'cloak-data'}
        if include_cloak_binary:
            # No custom URLs/keys: this local bundle must use the vendor's
            # unmodified public binary and its normal authenticity checks.
            for key in ('CLOAKBROWSER_BINARY_PATH', 'CLOAKBROWSER_DOWNLOAD_URL',
                        'CLOAKBROWSER_LICENSE_KEY', 'CLOAKBROWSER_SKIP_CHECKSUM',
                        'CLOAKBROWSER_VERSION', 'CLOAKBROWSER_RELEASE_CHANNEL'):
                if os.environ.get(key):
                    raise ValueError(f'Unset {key} before preparing the internal Cloak bundle')
            executable = Path(download.ensure_binary())
            browser_root = config.get_binary_dir()
            copy_folder(browser_root, staging / 'cloak')
            manifest['cloak'] = {
                'executable': (Path('cloak') / executable.relative_to(browser_root)).as_posix(),
                'version': config.get_chromium_version(),
            }
        for engine in ('playwright', 'patchright', 'camoufox', 'cloak'):
            if engine in manifest:
                executable = staging / manifest[engine]['executable']
                digest = hashlib.sha256()
                with executable.open('rb') as stream:
                    for block in iter(lambda: stream.read(1024 * 1024), b''):
                        digest.update(block)
                manifest[engine]['sha256'] = digest.hexdigest()
        (staging / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
        if output.exists():
            if output.is_symlink() or not (output / 'manifest.json').is_file():
                raise ValueError(f'Refusing to replace an unowned bundle directory: {output}')
            shutil.rmtree(output)
        staging.rename(output)
        logging.info('Prepared browser bundle: %s', output)
    finally:
        if staging.exists():
            shutil.rmtree(staging)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--include-cloak-binary', action='store_true')
    parser.add_argument('--fetch-camoufox', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    logs = args.cache.resolve() / 'logs'
    logs.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime('bundle-browsers_%Y-%m-%d_%H-%M-%S_UTC', time.gmtime())
    log = logs / f'{stamp}-{uuid.uuid4().hex[:8]}.log'
    handler = logging.FileHandler(log, mode='x', encoding='utf-8')
    formatter = logging.Formatter('[%(asctime)s UTC] [%(levelname)s] [bundle] %(message)s', '%Y-%m-%d %H:%M:%S')
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logging.basicConfig(level=logging.INFO, handlers=[handler, logging.StreamHandler()])
    # HTTP libraries otherwise log signed temporary redirect URLs verbatim.
    logging.getLogger('httpx').setLevel(logging.WARNING)
    logging.getLogger('httpcore').setLevel(logging.WARNING)
    from numera.logs import RedactingFilter
    for sink in logging.getLogger().handlers:
        sink.addFilter(RedactingFilter())
    try:
        if args.fetch_camoufox:
            camoufox_cache(args.cache.resolve() / 'camoufox')
            from camoufox.__main__ import cli
            cli(['fetch'], standalone_mode=False)
        elif args.output is None:
            parser.error('--output is required when assembling a bundle')
        else:
            assemble(args.output, args.cache, args.include_cloak_binary)
    except Exception:
        logging.exception('Browser bundle preparation failed')
        return 1
    finally:
        logging.shutdown()
        logging.getLogger().handlers.clear()
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
