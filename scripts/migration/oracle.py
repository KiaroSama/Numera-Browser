"""Generate deterministic model fixtures by executing the retained source API.

No browser or user-data access. Output contains only deliberately non-sensitive
inputs; this is the model seam, not full application migration acceptance.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import logging
import platform
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASELINE = '13b6d9c6973281fe123353eaec05b29d65781a2d'
sys.path.insert(0, str(ROOT / 'engine'))
from numera.fingerprint import generate, ua_metadata, client_hint_headers, validate  # noqa: E402
from numera import devices  # noqa: E402
from numera.models import Fingerprint, Profile, Proxy  # noqa: E402
from run_logging import run_log  # noqa: E402

LOG = logging.getLogger('numera.migration-oracle')


def model_cases() -> list[dict]:
    cases = []
    for os_key in ('windows', 'macos', 'linux', 'android', 'ios'):
        fp = generate(seed=42, os=os_key, locale='en-US').to_dict()
        cases.append({'name': f'fingerprint-{os_key}', 'operation': 'fingerprint',
                      'input': fp, 'expected': Fingerprint.from_dict(fp).to_dict()})
        legacy = dict(fp)
        for key in ('fonts', 'cameras', 'microphones', 'speakers', 'canvas', 'audio',
                    'font_mode', 'geolocation', 'do_not_track', 'client_rects',
                    'webgl_image', 'media_mode', 'os_version', 'device_model',
                    'max_touch_points', 'device_pixel_ratio', 'webrtc'):
            legacy.pop(key, None)
        legacy['future_field'] = 'ignored in storage view'
        cases.append({'name': f'legacy-fingerprint-{os_key}', 'operation': 'fingerprint',
                      'input': legacy, 'expected': Fingerprint.from_dict(legacy).to_dict()})
    profile = {'id': 'fixture00001', 'name': 'Unicode α فارسی',
               'fingerprint': generate(seed=42, os='windows').to_dict(),
               'created_at': 1700000000.0, 'updated_at': 1700000001.0,
               'proxy': {'server': 'http://127.0.0.1:8080', 'username': 'fixture-user',
                         'password': 'fixture-value', 'country': 'US', 'future_field': True}}
    cases.append({'name': 'legacy-profile-defaults', 'operation': 'profile',
                  'input': profile, 'expected': Profile.from_dict(profile).to_dict()})
    for stored in (False, True):
        for initial in (None, '', 'fixture-value'):
            for assignment in ('untouched', None, '', 'fixture-replacement'):
                data = {'server': 'socks5://127.0.0.1:1080', 'username': 'fixture-user',
                        'password': initial, 'country': 'US'}
                proxy = Proxy.from_dict(data) if stored else Proxy(**data)
                if assignment != 'untouched':
                    proxy.password = assignment
                cases.append({'name': f'proxy-{len(cases)}', 'operation': 'proxy',
                              'input': data, 'stored': stored, 'assignment': assignment,
                              'expected': {'record': proxy.to_dict(), 'intent': proxy.password_intent,
                                           'playwright': proxy.to_playwright()}})
    return cases


def document(seam: str = 'models') -> dict:
    files = ('engine/numera/models.py', 'engine/numera/fingerprint.py', 'engine/numera/devices.py')
    result = {'schema': 1, 'source_commit': BASELINE, 'source_runtime': platform.python_version(),
              'source_hashes': {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in files}}
    if seam == 'models':
        result.update(seam='models-and-proxy-intent', cases=model_cases())
    elif seam == 'devices':
        result.update(seam='device-catalog', catalog={
            key: getattr(devices, key) for key in (
                'CHROME_VERSIONS', 'WEBGL', 'SCREENS', 'HARDWARE', 'OS_UA_TOKEN',
                'NAV_PLATFORM', 'CH_PLATFORM', 'OS_VERSIONS', 'ANDROID_MODELS',
                'TOUCH_POINTS', 'PIXEL_RATIOS', 'LOCALES', 'COUNTRY_TO_LOCALE',
                'COUNTRY_GEO', 'LOCALE_TO_COUNTRY', 'FONTS', 'DESKTOP_OS',
                'MOBILE_OS', 'ALL_OS', 'WEIGHTS', 'TZ_ALIASES')})
    elif seam == 'pool-digits':
        from pool_parse_cases import digit_ranges, decimal_ranges
        result.update(seam='unicode-digit-ranges', ranges=digit_ranges(), decimal_ranges=decimal_ranges())
    elif seam == 'profile-read':
        from profile_read_cases import cases
        result.update(seam='http-profile-read', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_profiles.py', 'engine/numera/server/__init__.py', 'engine/numera/write_contract.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'fingerprint-routes':
        from fingerprint_route_cases import cases
        result.update(seam='http-fingerprint-validation', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_profiles.py', 'engine/numera/convert.py', 'engine/numera/fingerprint.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'proxy-routes':
        from proxy_route_cases import cases
        result.update(seam='http-proxy-persistence', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_proxies.py', 'engine/numera/proxypool.py', 'engine/numera/server/stores.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'extension-routes':
        from extension_route_cases import cases
        result.update(seam='http-installed-extensions', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_config.py', 'engine/numera/webstore.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'config-routes':
        from config_route_cases import cases
        result.update(seam='http-settings-routes', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_config.py', 'engine/numera/config_store.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'application-routes':
        from application_route_cases import cases
        result.update(seam='http-application-composition', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/__init__.py', 'engine/numera/server/stores.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'schedule-routes':
        from schedule_route_cases import cases
        result.update(seam='http-schedule-routes', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_schedules.py', 'engine/numera/scheduler.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'folder-routes':
        from folder_route_cases import cases
        result.update(seam='http-folder-transactions', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_config.py', 'engine/numera/folders.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'token-routes':
        from token_route_cases import cases
        result.update(seam='http-token-routes', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/routes_config.py', 'engine/numera/server/stores.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'diagnostic-routes':
        from diagnostic_route_cases import cases
        result.update(seam='http-diagnostic-routes', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/__init__.py', 'engine/numera/server/routes_config.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'cors':
        from cors_cases import cases
        from importlib.metadata import version
        result.update(seam='http-cors-requests', source_dependencies={name: version(name) for name in ('fastapi', 'starlette')}, cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        result['source_hashes']['engine/numera/server/__init__.py'] = hashlib.sha256((ROOT / 'engine/numera/server/__init__.py').read_bytes()).hexdigest()
    elif seam == 'bind-policy':
        from bind_policy_cases import cases
        result.update(seam='http-bind-policy', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/__init__.py', 'engine/numera/server/stores.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'schedule-write':
        from schedule_write_cases import cases
        result.update(seam='schedule-write-validation', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/scheduler.py', 'engine/numera/warmup.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'schedule-store':
        from schedule_store_cases import cases
        result.update(seam='schedule-occurrence-bookkeeping', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        result['source_hashes']['engine/numera/scheduler.py'] = hashlib.sha256((ROOT / 'engine/numera/scheduler.py').read_bytes()).hexdigest()
    elif seam == 'dashboard-revision':
        from dashboard_revision_cases import cases
        result.update(seam='dashboard-revision-coercion', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        result['source_hashes']['engine/numera/server/stores.py'] = hashlib.sha256((ROOT / 'engine/numera/server/stores.py').read_bytes()).hexdigest()
    elif seam == 'dashboard-meta':
        from dashboard_meta_cases import cases
        result.update(seam='dashboard-metadata-delete', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/stores.py', 'engine/numera/repo.py', 'engine/numera/paths.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'dashboard-batch':
        from dashboard_batch_cases import cases
        result.update(seam='dashboard-batch-transaction', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/stores.py', 'engine/numera/server/convert.py', 'engine/numera/repo.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'dashboard-vault':
        from dashboard_vault_cases import cases
        result.update(seam='dashboard-vault-intent', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/stores.py', 'engine/numera/vault.py', 'engine/numera/repo.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'dashboard-store':
        from dashboard_store_cases import cases
        result.update(seam='dashboard-dual-store', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/stores.py', 'engine/numera/server/convert.py', 'engine/numera/repo.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'request-auth':
        from request_auth_cases import cases
        result.update(seam='request-auth-boundary', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/__init__.py', 'engine/numera/server/stores.py', 'engine/numera/server/auth_policy.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'token-create':
        from token_create_cases import cases
        result.update(seam='named-token-create', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/server/stores.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'token-store':
        from token_store_cases import cases
        result.update(seam='sticky-token-store', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/stores.py', 'engine/numera/server/auth_policy.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'extension-install':
        from extension_install_cases import cases
        result.update(seam='extension-install-mutation', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/webstore.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'extension-packages':
        from extension_package_cases import cases
        result.update(seam='extension-crx-extraction', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/webstore.py', 'engine/numera/paths.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'extension-ids':
        from extension_id_cases import cases
        result.update(seam='extension-id-recognition', cases=cases())
        name = 'engine/numera/webstore.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'extension-resources':
        from extension_resource_cases import cases
        result.update(seam='extension-manifest-resources', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/webstore.py', 'engine/numera/paths.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'profile-restore':
        from profile_restore_cases import cases
        result.update(seam='staged-profile-restore', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/archive.py', 'engine/numera/store.py', 'engine/numera/repo.py', 'engine/numera/paths.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'profile-backup':
        from profile_backup_cases import cases
        result.update(seam='lease-aware-profile-backup', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/archive.py', 'engine/numera/store.py', 'engine/numera/sessions.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'profile-import':
        from profile_import_cases import cases
        result.update(seam='dual-store-profile-import', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/archive.py', 'engine/numera/store.py', 'engine/numera/repo.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'profile-export':
        from profile_export_cases import cases
        result.update(seam='credential-aware-profile-export', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/archive.py', 'engine/numera/store.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'pool-vault':
        from pool_vault_cases import cases
        result.update(seam='proxy-pool-vault', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/server/stores.py', 'engine/numera/vault.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'pool-store':
        from pool_store_cases import cases
        result.update(seam='proxy-pool-mutation', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/server/stores.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'pool-normalize':
        from pool_normalize_cases import cases
        result.update(seam='proxy-structured-validation', cases=cases())
        name = 'engine/numera/proxypool.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'pool-parse':
        from pool_parse_cases import cases
        result.update(seam='proxy-paste-roundtrip', cases=cases())
        name = 'engine/numera/proxypool.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'first-run':
        from first_run_cases import cases
        result.update(seam='first-run-copy-merge', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/firstrun.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'data-root':
        from data_root_cases import cases
        result.update(seam='data-root-precedence', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/paths.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'repo-delete':
        from repo_delete_cases import cases
        result.update(seam='dual-store-delete', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/repo.py', 'engine/numera/store.py', 'engine/numera/sessions.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'repo-lookup':
        from repo_lookup_cases import cases
        result.update(seam='dual-store-lookup', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/repo.py', 'engine/numera/store.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'repo-batch':
        from repo_batch_cases import cases
        result.update(seam='dual-store-batch', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/repo.py', 'engine/numera/store.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'folders':
        from folder_cases import cases
        result.update(seam='folder-transactions', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/folders.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'rotation-recovery':
        from rotation_recovery_cases import cases
        result.update(seam='interrupted-vault-rotation', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/vault.py', 'engine/numera/vault_recovery.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'vault-rotate':
        from vault_rotate_cases import cases
        result.update(seam='ordinary-vault-rotation', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/store.py', 'engine/numera/vault.py', 'engine/numera/vault_recovery.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'vault-enable':
        from vault_enable_cases import cases
        result.update(seam='ordinary-vault-enable', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/store.py', 'engine/numera/vault.py', 'engine/numera/vault_recovery.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'claims':
        from claim_cases import cases
        result.update(seam='durable-claim-uncertainty', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/sessions.py', 'engine/numera/proctree.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'recovery':
        from recovery_cases import cases
        result.update(seam='interrupted-vault-enable', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/vault_recovery.py', 'engine/numera/vault.py', 'engine/numera/vault_active_descriptor.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'migration':
        from migration_cases import cases
        result.update(seam='secret-file-migration', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/vault.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'repo-write':
        from repo_write_cases import cases
        result.update(seam='dual-store-write', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/repo.py', 'engine/numera/store.py', 'engine/numera/vault.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'completion':
        from completion_cases import cases
        result.update(seam='authenticated-vault-completion', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/vault_recovery.py', 'engine/numera/vault.py', 'engine/numera/vault_problems.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'repo':
        from repo_cases import cases
        result.update(seam='dual-store-reconciliation', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        for name in ('engine/numera/repo.py', 'engine/numera/convert.py', 'engine/numera/store.py'):
            result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'secrets':
        from secret_cases import cases
        result.update(seam='secret-write-policy', cases=cases())
        name = 'engine/numera/vault.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'config':
        from config_cases import cases
        result.update(seam='config-read-merge', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        name = 'engine/numera/config_store.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'paths':
        from path_cases import cases
        result.update(seam='safe-profile-id', cases=cases())
        name = 'engine/numera/paths.py'
        result['source_hashes'][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    elif seam == 'journal':
        from journal_cases import cases
        result.update(seam='vault-journal-classification', cases=cases(ROOT / '.ci-work' / 'migration-temp'))
        result['source_hashes'].update({name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
                                       for name in ('engine/numera/vault_recovery.py', 'engine/numera/vault_active_descriptor.py')})
    elif seam == 'conversion':
        from conversion_cases import cases
        result.update(seam='profile-conversion', cases=cases())
        result['source_hashes'].update({name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
                                       for name in ('engine/numera/convert.py', 'engine/numera/repo.py')})
    elif seam == 'vault':
        from vault_cases import cases
        result.update(seam='vault-envelopes', cases=cases())
        result['source_hashes']['engine/numera/crypto.py'] = hashlib.sha256((ROOT / 'engine/numera/crypto.py').read_bytes()).hexdigest()
    elif seam in ('validation', 'field-policy'):
        from validation_cases import cases, policy
        if seam == 'validation':
            result.update(seam='write-validation', cases=cases())
        else:
            result.update(seam='field-policy', policy=policy())
        result['source_hashes'].update({name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
                                       for name in ('engine/numera/field_policy.py', 'engine/numera/write_contract.py')})
    elif seam == 'identity':
        cases = []
        for os_key in devices.ALL_OS:
            for seed in (0, 42, 2147483647):
                original = generate(seed=seed, os=os_key)
                for changes in ({}, {'viewport_width': 9000}, {'timezone': 'unknown'},
                                {'languages': []}, {'webgl_renderer': ''}, {'fonts': ['alien']},
                                {'max_touch_points': -1}, {'device_memory': 3.5}, {'is_mobile': not original.is_mobile}):
                    fp = Fingerprint.from_dict({**original.to_dict(), **changes})
                    cases.append({'name': f'identity-{len(cases)}', 'input': fp.to_dict(),
                                  'expected': {'metadata': ua_metadata(fp), 'headers': client_hint_headers(fp),
                                               'issues': validate(fp)}})
        result.update(seam='identity-contract', cases=cases)
    else:
        cases = []
        for seed in (0, 1, 42, 2147483647, -1, 2**63 - 1):
            for os_key in (None, *devices.ALL_OS):
                for locale in (None, 'en-US', 'unknown'):
                    data = {'seed': seed, 'os': os_key, 'locale': locale}
                    cases.append({'name': f'generated-{len(cases)}', 'input': data,
                                  'expected': generate(**data).to_dict()})
        for country in ('US', 'gb', 'PK', 'unknown'):
            data = {'seed': 42, 'mobile': True, 'country': country}
            cases.append({'name': f'proxy-country-{country}', 'input': data,
                          'expected': generate(seed=42, mobile=True,
                                               proxy=Proxy(server='http://127.0.0.1:8080', country=country)).to_dict()})
        result.update(seam='seeded-generation', cases=cases)
    return result


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--seam', choices=('models', 'devices', 'generation', 'identity', 'validation', 'field-policy', 'vault', 'conversion', 'journal', 'paths', 'config', 'secrets', 'repo', 'completion', 'repo-write', 'migration', 'recovery', 'claims', 'vault-enable', 'vault-rotate', 'rotation-recovery', 'folders', 'repo-batch', 'repo-lookup', 'repo-delete', 'data-root', 'first-run', 'pool-parse', 'pool-digits', 'pool-normalize', 'pool-store', 'pool-vault', 'profile-export', 'profile-import', 'profile-backup', 'profile-restore', 'extension-resources', 'extension-ids', 'extension-packages', 'extension-install', 'token-store', 'token-create', 'request-auth', 'dashboard-store', 'dashboard-vault', 'dashboard-batch', 'dashboard-meta', 'dashboard-revision', 'schedule-store', 'schedule-write', 'bind-policy', 'cors', 'diagnostic-routes', 'token-routes', 'folder-routes', 'schedule-routes', 'application-routes', 'config-routes', 'extension-routes', 'proxy-routes', 'fingerprint-routes', 'profile-read'), default='models')
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--log-dir', type=Path, default=ROOT / 'scripts' / 'logs')
    args = parser.parse_args(argv)
    started = time.monotonic()
    try:
        with run_log('oracle', args.log_dir):
            LOG.info('Oracle starting runtime=%s seam=%s', platform.python_version(), args.seam)
            result = document(args.seam)
            if args.check:
                current = json.loads(args.output.read_text(encoding='utf-8'))
                # Fixture behavior must be runtime-portable; interpreter version
                # is provenance, not permission to normalize expected values.
                result['source_runtime'] = current['source_runtime']
                if current != result:
                    raise ValueError('Model oracle fixture drift')
            else:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                # Never overwrite a previously recorded oracle implicitly.
                with args.output.open('x', encoding='utf-8', newline='\n') as stream:
                    stream.write(json.dumps(result, ensure_ascii=True, separators=(',', ':')) + '\n')
            count = len(result.get('cases', result.get('catalog', result.get('policy', result.get('ranges', {})))))
            LOG.info('Oracle completed entries=%s exit=0 duration=%.3fs', count, time.monotonic() - started)
        print(f'Oracle verified: {count} non-sensitive entries ({args.seam})')
        return 0
    except Exception as error:
        # No raw application payload or credential-shaped exception text.
        print(f'Model oracle failed ({type(error).__name__}); no migration acceptance', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
