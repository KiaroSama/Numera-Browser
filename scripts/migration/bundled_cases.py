"""Observe portable selection with inert files and an SDK enum boundary only."""
from __future__ import annotations
import json
import os
import tempfile
from enum import Enum
from pathlib import Path
from types import ModuleType
from unittest.mock import patch
from baseline_source import activate
activate()


def cases(parent):
    from numera import bundled, paths
    parent = Path(parent)
    parent.mkdir(parents=True, exist_ok=True)
    class DefaultAddons(Enum):
        UBO = 'https://example.invalid/addon'
    addons = ModuleType('camoufox.addons')
    addons.DefaultAddons = DefaultAddons
    rows = []
    for engine, environment, missing in [
        ('playwright', {}, False), ('patchright', {}, False),
        ('camoufox', {}, False), ('cloak', {}, False),
        ('playwright', {'PLAYWRIGHT_BROWSERS_PATH': '/explicit/cache'}, False),
        ('cloak', {'CLOAKBROWSER_BINARY_PATH': '/explicit/browser'}, False),
        ('cloak', {}, True),
    ]:
        with tempfile.TemporaryDirectory(prefix='bundled-source-', dir=parent) as directory:
            root = Path(directory)
            browsers = root / 'browsers'
            browsers.mkdir()
            manifest = {}
            for backend in ('playwright', 'patchright', 'camoufox', 'cloak'):
                executable = browsers / backend / 'browser.bin'
                executable.parent.mkdir()
                executable.write_bytes(backend.encode('utf-8'))
                manifest[backend] = {'executable': f'{backend}/browser.bin'}
            addon = browsers / 'camoufox' / 'addons' / 'UBO'
            addon.mkdir(parents=True)
            (addon / 'manifest.json').write_text('{}', encoding='utf-8')
            manifest['camoufox'].update(version='152.0.4', addons=['camoufox/addons/UBO'])
            manifest['cloak']['version'] = '146.0.7680.177.5'
            if missing: del manifest['cloak']
            (browsers / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
            owned = ('PLAYWRIGHT_BROWSERS_PATH', 'CLOAKBROWSER_BINARY_PATH', 'CLOAKBROWSER_LICENSE_KEY', 'CLOAKBROWSER_CACHE_DIR')
            with patch.dict(os.environ, {}, clear=False), patch.object(paths, 'bundled_browsers', return_value=browsers), patch.dict('sys.modules', {'camoufox.addons': addons}):
                for key in owned: os.environ.pop(key, None)
                os.environ.update(environment)
                options = bundled.launch_options(engine, root / 'data')
                options = {key: [value.name if isinstance(value, Enum) else value.replace(str(root), '$ROOT') for value in values]
                           if isinstance(values, list) else values.replace(str(root), '$ROOT') if isinstance(values, str) else values
                           for key, values in options.items()}
                changes = {key: os.environ[key].replace(str(root), '$ROOT') for key in owned
                           if key in os.environ and os.environ.get(key) != environment.get(key)}
            rows.append({'engine': engine, 'environment': environment, 'manifest': manifest,
                         'options': options, 'changes': changes})
    return rows
