"""Consent-only vendor acquisition; opening a browser never updates its binary."""
from __future__ import annotations

import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import re

from . import logs


class ConsentRequired(RuntimeError):
    """The vendor browser is missing or cannot honor the shown exact target."""


def _sdk():
    try:
        from cloakbrowser import config, download
        return config, download, importlib.metadata.version('cloakbrowser')
    except (ImportError, importlib.metadata.PackageNotFoundError) as error:
        raise ConsentRequired('the CloakBrowser SDK must be installed before browser acquisition') from error


def _license_key():
    from cloakbrowser.license import resolve_license_key
    return resolve_license_key(None)


def _license_info(key):
    from cloakbrowser.license import validate_license
    return validate_license(key)


def _version(value):
    if not isinstance(value, str) or not re.fullmatch(r'[0-9]+(?:\.[0-9]+){3,4}', value):
        raise ConsentRequired('Cloak browser version metadata is invalid')
    return value


def offer():
    """Only local metadata; no provider request, download, or license validation."""
    try:
        config, _download, sdk = _sdk()
        version = _version(config.normalize_requested_version() or config.get_chromium_version())
        keyed = bool(_license_key())
        identity = {'engine': 'cloak', 'sdk': sdk, 'version': version,
                    'platform': config.get_platform_tag(), 'provider': 'CloakHQ/CloakBrowser',
                    'keyed': keyed}
        offer_id = hashlib.sha256(json.dumps(identity, sort_keys=True).encode('utf-8')).hexdigest()
        unavailable = None
        if os.environ.get('CLOAKBROWSER_DOWNLOAD_URL'):
            unavailable = 'custom vendor mirrors are not supported by consent-controlled acquisition'
        elif keyed and not config.normalize_requested_version():
            unavailable = ('keyed acquisition needs an explicit exact CLOAKBROWSER_VERSION; '
                           'free-key latest-only downloads use the manual vendor route')
        override = os.environ.get('CLOAKBROWSER_BINARY_PATH')
        if override:
            installed = Path(override).is_file()
            unavailable = ('an explicit browser override is user-managed; vendor acquisition is disabled'
                           if installed else 'the explicit Cloak browser binary is missing; repair or remove its override')
        else:
            installed = config.get_binary_path(version, pro=keyed).is_file()
        return {**identity, 'offer_id': offer_id, 'installed': installed,
                'available': unavailable is None, 'reason': unavailable,
                'notice': 'Vendor download, not a redistributed Numera binary; exact SDK compatibility is not certified.'}
    except ConsentRequired:
        return {'engine': 'cloak', 'available': False, 'installed': False,
                'reason': 'The Cloak SDK or its version metadata is unavailable', 'offer_id': None}
    except (ValueError, OSError):
        return {'engine': 'cloak', 'available': False, 'installed': False,
                'reason': 'Cloak browser metadata could not be read safely', 'offer_id': None}


def acquire(offer_id: str, *, approved: bool):
    """Called by an owned finite helper after consent to this exact SDK target."""
    current = offer()
    if (approved is not True or not current['available']
            or not isinstance(offer_id, str) or offer_id != current['offer_id']):
        raise ConsentRequired('affirmative consent to the current exact Cloak acquisition offer is required')
    config, download, _sdk_version = _sdk()
    from .paths import SessionBusy, session_lease
    lease = session_lease(Path(config.get_cache_dir()).resolve(), 'cloak-acquisition')
    try:
        lease.__enter__()
    except SessionBusy as error:
        raise ConsentRequired('a Cloak acquisition is already in progress for this vendor cache') from error
    try:
        # This lease is cache-wide, not data-root-wide; two engines configured
        # for one vendor cache cannot concurrently delete/extract its targets.
        current = offer()
        if not current['available'] or offer_id != current['offer_id']:
            raise ConsentRequired('Cloak acquisition offer changed; review it before approving again')
        return _acquire_locked(current, offer_id, config, download)
    finally:
        lease.__exit__(None, None, None)


def _acquire_locked(current, offer_id, config, download):
    key = _license_key()
    if key:
        info = _license_info(key)
        if not info or not info.valid or info.plan == 'free':
            raise ConsentRequired('this vendor license cannot honor an exact pinned acquisition; use the manual vendor route')
    os.environ['CLOAKBROWSER_AUTO_UPDATE'] = 'false'
    logs.get('updates').info('approved Cloak browser acquisition started version=%s', current['version'])
    options = {'browser_version': current['version']}
    if key:
        options['license_key'] = key
    binary = Path(download.ensure_binary(**options))
    if binary.resolve() != Path(config.get_binary_path(current['version'], pro=bool(key))).resolve() or not binary.is_file():
        raise ConsentRequired('vendor acquisition did not produce the approved browser target')
    logs.get('updates').info('approved Cloak browser acquisition completed version=%s', current['version'])
    return {'engine': 'cloak', 'installed': True, 'version': current['version'],
            'offer_id': offer_id}


def launch_options():
    """Pin an existing binary so vendor launch cannot fetch or update anything."""
    os.environ['CLOAKBROWSER_AUTO_UPDATE'] = 'false'
    override = os.environ.get('CLOAKBROWSER_BINARY_PATH')
    if override:
        if not Path(override).is_file():
            raise ConsentRequired('the explicit Cloak browser binary is missing')
        return {}
    config, _download, _sdk_version = _sdk()
    requested = config.normalize_requested_version()
    candidates = []
    keyed = bool(_license_key())
    if requested:
        candidates.append((requested, keyed))
    elif keyed:
        version = config.get_effective_version(pro=True)
        if version:
            candidates.append((version, True))
    else:
        candidates.append((config.get_effective_version(), False))
    for version, pro in candidates:
        version = _version(version)
        binary = Path(config.get_binary_path(version, pro=pro))
        if binary.is_file() and os.access(binary, os.X_OK):
            os.environ['CLOAKBROWSER_BINARY_PATH'] = str(binary)
            return {'browser_version': version}
    raise ConsentRequired('Cloak browser acquisition requires explicit approval in Engines before first launch')
