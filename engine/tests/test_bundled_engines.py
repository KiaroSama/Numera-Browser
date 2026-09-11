"""Portable backend selection must never substitute one Chromium for another."""
import json
from enum import Enum
import sys
from types import ModuleType

import pytest

from numera import paths


class DefaultAddons(Enum):
    UBO = 'https://example.invalid/addon.xpi'


@pytest.fixture
def bundle(tmp_path, monkeypatch):
    root = tmp_path / 'portable' / 'browsers'
    root.mkdir(parents=True)
    monkeypatch.setattr(paths, 'bundled_browsers', lambda: root)
    manifest = {}
    for name in ('playwright', 'patchright', 'camoufox', 'cloak'):
        exe = root / name / 'browser.exe'
        exe.parent.mkdir()
        exe.write_bytes(name.encode())
        manifest[name] = {'executable': f'{name}/browser.exe'}
    addon = root / 'camoufox' / 'addons' / 'UBO'
    addon.mkdir(parents=True)
    (addon / 'manifest.json').write_text('{}', encoding='utf-8')
    manifest['camoufox']['addons'] = ['camoufox/addons/UBO']
    manifest['camoufox']['version'] = '152.0.4'
    manifest['cloak']['version'] = '146.0.7680.177.5'
    (root / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
    # The resolver mutates these directly. Register their original state even
    # when absent, so teardown cannot retain a fixture's browser/cache path.
    for name in ('CLOAKBROWSER_BINARY_PATH', 'CLOAKBROWSER_CACHE_DIR'):
        monkeypatch.setenv(name, '')
        monkeypatch.delenv(name)
    # The contract test needs the wrapper's enum shape, not a network-capable
    # optional desktop installation in the lean server CI job.
    addons = ModuleType('camoufox.addons')
    addons.DefaultAddons = DefaultAddons
    monkeypatch.setitem(sys.modules, 'camoufox.addons', addons)
    return root


def test_chromium_backends_select_distinct_bundled_executables(bundle):
    from numera.bundled import launch_options
    stock = launch_options('playwright')['executable_path']
    patched = launch_options('patchright')['executable_path']
    assert stock != patched
    assert stock == str(bundle / 'playwright/browser.exe')
    assert patched == str(bundle / 'patchright/browser.exe')


@pytest.mark.parametrize('engine', ['playwright', 'patchright'])
def test_explicit_chromium_cache_override_wins(bundle, monkeypatch, engine):
    from numera.bundled import launch_options
    monkeypatch.setenv('PLAYWRIGHT_BROWSERS_PATH', '/explicit/cache')
    assert launch_options(engine) == {}


def test_camoufox_uses_packaged_resources_and_default_addons(bundle):
    from numera.bundled import launch_options
    options = launch_options('camoufox')
    assert options['executable_path'] == str(bundle / 'camoufox/browser.exe')
    assert options['ff_version'] == '152'
    assert options['addons'] == [str(bundle / 'camoufox/addons/UBO')]
    assert options['exclude_addons'] == list(DefaultAddons)


def test_internal_cloak_binary_is_explicit_and_user_override_wins(bundle, monkeypatch):
    import os
    from numera.bundled import launch_options
    assert launch_options('cloak') == {'browser_version': '146.0.7680.177.5'}
    assert os.environ['CLOAKBROWSER_BINARY_PATH'] == str(bundle / 'cloak/browser.exe')
    assert launch_options('cloak') == {'browser_version': '146.0.7680.177.5'}
    monkeypatch.setenv('CLOAKBROWSER_BINARY_PATH', '/user/chrome')
    assert launch_options('cloak') == {}
    assert os.environ['CLOAKBROWSER_BINARY_PATH'] == '/user/chrome'


def test_distribution_cloak_has_no_bundled_binary_or_override(bundle):
    import os
    from numera.bundled import launch_options
    manifest = json.loads((bundle / 'manifest.json').read_text(encoding='utf-8'))
    del manifest['cloak']
    (bundle / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
    assert launch_options('cloak') == {}
    assert 'CLOAKBROWSER_BINARY_PATH' not in os.environ


def test_geoip_is_seeded_into_writable_data_without_overwriting_refreshes(bundle, tmp_path):
    import os
    from numera.bundled import launch_options
    source = bundle / 'cloak-data/geoip/GeoLite2-City.mmdb'
    source.parent.mkdir(parents=True)
    source.write_bytes(b'fixture-database')
    os.utime(source, (1700000000, 1700000000))
    source.chmod(0o444)
    manifest = json.loads((bundle / 'manifest.json').read_text(encoding='utf-8'))
    del manifest['cloak']
    manifest['_geoip'] = {'cloak': 'cloak-data'}
    (bundle / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
    data = tmp_path / 'writable-data'
    assert launch_options('cloak', data) == {}
    cached = data / 'browser-cache/cloak/geoip/GeoLite2-City.mmdb'
    assert cached.read_bytes() == source.read_bytes()
    assert cached.stat().st_mtime == source.stat().st_mtime
    assert os.environ['CLOAKBROWSER_CACHE_DIR'] == str(data / 'browser-cache/cloak')
    cached.write_bytes(b'vendor-refreshed-database')
    launch_options('cloak', data)
    assert cached.read_bytes() == b'vendor-refreshed-database'
    source.chmod(0o644)


@pytest.mark.parametrize('damage', ['missing', 'escape', 'missing-addon'])
def test_broken_bundle_refuses_fallback_to_a_personal_cache(bundle, damage):
    from numera.bundled import launch_options
    manifest = json.loads((bundle / 'manifest.json').read_text(encoding='utf-8'))
    engine = 'patchright'
    if damage == 'missing':
        (bundle / 'patchright/browser.exe').unlink()
    elif damage == 'escape':
        manifest[engine]['executable'] = '../browser.exe'
    else:
        engine = 'camoufox'
        (bundle / 'camoufox/addons/UBO/manifest.json').unlink()
    (bundle / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
    with pytest.raises((ValueError, FileNotFoundError)):
        launch_options(engine)
