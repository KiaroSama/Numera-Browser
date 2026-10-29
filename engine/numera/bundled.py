"""Resolve portable browser executables without sharing backend caches."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import stat
import uuid

from . import paths


def _seed_geoip(source: Path, destination: Path) -> Path:
    for original in source.rglob('*'):
        if not original.is_file():
            continue
        target = destination / original.relative_to(source)
        if target.exists():
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        temporary = target.with_name(f'{target.name}.seed-{uuid.uuid4().hex}')
        try:
            shutil.copy2(original, temporary)
            temporary.chmod(temporary.stat().st_mode | stat.S_IWUSR)
            os.replace(temporary, target)
        finally:
            temporary.unlink(missing_ok=True)
    return destination


def launch_options(engine: str, data_dir=None) -> dict:
    if engine in ('playwright', 'patchright') and os.environ.get('PLAYWRIGHT_BROWSERS_PATH'):
        return {}
    root = paths.bundled_browsers()
    if root is None or not (root / 'manifest.json').is_file():
        return {}
    manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8'))
    geoip = manifest.get('_geoip', {}).get(engine)
    if geoip:
        source = paths.safe_join(root, geoip)
        if not source.is_dir():
            raise FileNotFoundError(f'The bundled {engine} GeoIP data is missing')
        database = Path(data_dir or paths.resolve_data_dir()) / 'browser-cache' / engine
        _seed_geoip(source, database)
        if engine == 'cloak':
            os.environ.setdefault('CLOAKBROWSER_CACHE_DIR', str(database))
        elif engine == 'camoufox':
            from camoufox import geolocation
            geolocation.GEOIP_DIR = database
            geolocation.MMDB_DIR = database / 'mmdb'
            geolocation.GEOIP_CONFIG = database / 'config.yml'
    entry = manifest.get(engine)
    if entry is None:
        # The proprietary binary is excluded from public builds. The separate
        # acquisition route requires consent before any vendor download.
        if engine == 'cloak':
            return {}
        raise FileNotFoundError(f'The portable bundle has no {engine} browser')
    executable = paths.safe_join(root, entry['executable'])
    if not executable.is_file():
        raise FileNotFoundError(f'The bundled {engine} executable is missing: {executable}')
    if engine == 'cloak':
        override = os.environ.get('CLOAKBROWSER_BINARY_PATH')
        if (override and override != str(executable)) or os.environ.get('CLOAKBROWSER_LICENSE_KEY'):
            return {}
        os.environ['CLOAKBROWSER_BINARY_PATH'] = str(executable)
        return {'browser_version': entry['version']}
    options = {'executable_path': str(executable)}
    if engine == 'camoufox':
        from camoufox.addons import DefaultAddons
        # Camoufox otherwise reads the user's selected install for its Firefox
        # version even when given an explicit executable from this bundle.
        options['ff_version'] = entry['version'].split('.', 1)[0]
        addons = [paths.safe_join(root, value) for value in entry['addons']]
        for addon in addons:
            if not (addon / 'manifest.json').is_file():
                raise FileNotFoundError(f'The bundled Camoufox addon is incomplete: {addon}')
        if len(addons) != len(DefaultAddons):
            raise ValueError('The portable bundle is missing a default Camoufox addon')
        options.update(addons=[str(addon) for addon in addons], exclude_addons=list(DefaultAddons))
    return options
