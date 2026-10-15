"""Vendor download is never an implicit side effect of opening a profile."""
import os
import subprocess
import sys
from types import SimpleNamespace

import pytest

pytestmark = pytest.mark.timeout(10)


@pytest.fixture
def vendor(tmp_path, monkeypatch):
    from numera import cloak_acquisition as acquisition
    version = '146.0.7680.177.5'
    binary = tmp_path / 'vendor-cache/chrome'
    calls = []

    def ensure_binary(**kwargs):
        calls.append(kwargs)
        binary.parent.mkdir(exist_ok=True)
        binary.write_bytes(b'controlled SDK output')
        binary.chmod(0o700)
        return str(binary)

    config = SimpleNamespace(
        get_cache_dir=lambda: binary.parent,
        get_platform_tag=lambda: 'windows-x64',
        get_chromium_version=lambda: version,
        get_effective_version=lambda **kwargs: version if not kwargs.get('pro') else None,
        get_binary_path=lambda *args, **kwargs: binary,
        normalize_requested_version=lambda value=None: value or os.environ.get('CLOAKBROWSER_VERSION'),
    )
    download = SimpleNamespace(ensure_binary=ensure_binary)
    monkeypatch.setattr(acquisition, '_sdk', lambda: (config, download, '0.5.12'))
    monkeypatch.setattr(acquisition, '_license_key', lambda: os.environ.get('CLOAKBROWSER_LICENSE_KEY'))
    for key in ('CLOAKBROWSER_BINARY_PATH', 'CLOAKBROWSER_LICENSE_KEY',
                'CLOAKBROWSER_DOWNLOAD_URL', 'CLOAKBROWSER_VERSION'):
        monkeypatch.delenv(key, raising=False)
    return acquisition, binary, calls


def test_missing_binary_refuses_launch_without_vendor_network(vendor):
    acquisition, binary, calls = vendor
    with pytest.raises(acquisition.ConsentRequired):
        acquisition.launch_options()
    assert not calls and not binary.exists()
    assert os.environ['CLOAKBROWSER_AUTO_UPDATE'] == 'false'


def test_approval_is_bound_to_shown_exact_offer_and_not_persisted(vendor):
    acquisition, binary, calls = vendor
    offer = acquisition.offer()
    assert offer['version'] == '146.0.7680.177.5'
    assert not calls
    with pytest.raises(acquisition.ConsentRequired):
        acquisition.acquire(offer['offer_id'], approved=False)
    with pytest.raises(acquisition.ConsentRequired):
        acquisition.acquire('0' * 64, approved=True)
    assert not calls
    result = acquisition.acquire(offer['offer_id'], approved=True)
    assert result['installed'] and binary.is_file()
    assert calls == [{'browser_version': '146.0.7680.177.5'}]
    assert acquisition.launch_options() == {'browser_version': '146.0.7680.177.5'}
    assert os.environ['CLOAKBROWSER_BINARY_PATH'] == str(binary)
    assert len(calls) == 1, 'opening a cached browser must not acquire a newer one'


def test_explicit_binary_override_is_preserved_without_acquisition(vendor, tmp_path, monkeypatch):
    acquisition, _binary, calls = vendor
    override = tmp_path / 'external-browser'
    override.write_bytes(b'user-owned-binary')
    monkeypatch.setenv('CLOAKBROWSER_BINARY_PATH', str(override))
    assert acquisition.launch_options() == {}
    offer = acquisition.offer()
    assert offer['installed'] and not offer['available']
    with pytest.raises(acquisition.ConsentRequired):
        acquisition.acquire(offer['offer_id'], approved=True)
    assert not calls
    assert override.read_bytes() == b'user-owned-binary'


def test_unpinned_keyed_or_custom_mirror_acquisition_is_unavailable(vendor, monkeypatch):
    acquisition, binary, calls = vendor
    monkeypatch.setenv('CLOAKBROWSER_LICENSE_KEY', 'fixture-not-a-real-license')
    assert not acquisition.offer()['available']
    with pytest.raises(acquisition.ConsentRequired):
        acquisition.acquire('0' * 64, approved=True)
    assert not calls and not binary.exists()
    monkeypatch.delenv('CLOAKBROWSER_LICENSE_KEY')
    monkeypatch.setenv('CLOAKBROWSER_DOWNLOAD_URL', 'https://example.invalid/custom')
    assert not acquisition.offer()['available']
    assert not calls


def test_exact_paid_pin_is_honored_but_free_latest_never_downloads(vendor, monkeypatch):
    acquisition, binary, calls = vendor
    monkeypatch.setenv('CLOAKBROWSER_LICENSE_KEY', 'fixture-not-a-real-license')
    monkeypatch.setenv('CLOAKBROWSER_VERSION', '146.0.7680.177.5')
    monkeypatch.setattr(acquisition, '_license_info',
                        lambda key: SimpleNamespace(valid=True, plan='free'))
    offer = acquisition.offer()
    assert offer['available'] and offer['keyed']
    with pytest.raises(acquisition.ConsentRequired, match='exact pinned'):
        acquisition.acquire(offer['offer_id'], approved=True)
    assert not calls and not binary.exists()
    monkeypatch.setattr(acquisition, '_license_info',
                        lambda key: SimpleNamespace(valid=True, plan='solo'))
    assert acquisition.acquire(offer['offer_id'], approved=True)['installed']
    assert calls == [{'browser_version': '146.0.7680.177.5',
                      'license_key': 'fixture-not-a-real-license'}]


def test_same_cache_reentrant_acquisition_refuses_before_vendor_call(vendor, monkeypatch):
    acquisition, _binary, calls = vendor
    config, download, _sdk = acquisition._sdk()
    target = acquisition.offer()['offer_id']
    real_download = download.ensure_binary
    overlaps = []

    def downloading(**kwargs):
        with pytest.raises(acquisition.ConsentRequired, match='acquisition.*in progress'):
            acquisition.acquire(target, approved=True)
        overlaps.append(True)
        return real_download(**kwargs)

    monkeypatch.setattr(download, 'ensure_binary', downloading)
    assert acquisition.acquire(target, approved=True)['installed']
    assert overlaps == [True] and len(calls) == 1


def test_shared_vendor_cache_refuses_another_process_before_download(vendor):
    acquisition, binary, calls = vendor
    from numera.paths import session_lease
    cache = binary.parent.resolve()
    child = '''import sys\nfrom types import SimpleNamespace\nfrom numera import cloak_acquisition as a\na._sdk=lambda:(SimpleNamespace(get_cache_dir=lambda:sys.argv[1]),None,None)\na.offer=lambda:{"available":True,"offer_id":"fixture"}\ntry:\n a.acquire("fixture",approved=True)\nexcept a.ConsentRequired as error:\n print(str(error));sys.exit(9)\nraise SystemExit(3)\n'''
    with session_lease(cache, 'cloak-acquisition'):
        result = subprocess.run([sys.executable, '-c', child, str(cache)],
                                capture_output=True, text=True, encoding='utf-8', timeout=5)
    assert result.returncode == 9, result.stderr
    assert 'acquisition' in result.stdout and 'in progress' in result.stdout
    assert not calls and not binary.exists()


def test_failed_acquisition_releases_only_its_cache_lease(vendor, monkeypatch):
    acquisition, _binary, calls = vendor
    _config, download, _sdk = acquisition._sdk()
    target = acquisition.offer()['offer_id']
    real_download = download.ensure_binary

    def failed(**kwargs):
        raise OSError('controlled vendor failure')

    monkeypatch.setattr(download, 'ensure_binary', failed)
    with pytest.raises(OSError, match='controlled vendor failure'):
        acquisition.acquire(target, approved=True)
    monkeypatch.setattr(download, 'ensure_binary', real_download)
    assert acquisition.acquire(target, approved=True)['installed']
    assert len(calls) == 1
