"""Observe retained launch options at the SDK boundary without opening a browser."""
from __future__ import annotations

import json
import sys
import types
from contextlib import ExitStack, nullcontext
from pathlib import Path
from unittest.mock import patch

from numera import drivers
from numera.fingerprint import generate
from numera.models import Profile, Proxy


class Captured(Exception):
    pass


class Store:
    root = Path('fixture-root')

    def user_data_path(self, profile_id):
        return Path('user-data') / profile_id


def cases():
    result = []
    for backend in ('playwright', 'patchright', 'camoufox', 'cloak'):
        for os in ('windows', 'macos', 'linux', 'android', 'ios'):
            for headless in (False, True):
                profile = Profile(id='a', name='fixture', engine=backend,
                                  created_at=1000, updated_at=1000,
                                  fingerprint=generate(seed=42, os=os, locale='en-US'))
                profile.dns = '9.9.9.9'
                profile.switched_off = ['googleServices', 'unknown', 'systemExtensions']
                profile.launch_args = ['--mute-audio']
                profile.fingerprint.geolocation = 'allow' if headless else 'block'
                profile.fingerprint.do_not_track = headless
                if headless:
                    profile.proxy = Proxy(server='http://127.0.0.1:8080', country='DE')
                    profile.ip_based = ['timezone']
                seen = {}

                def capture(**options):
                    seen.update(options)
                    raise Captured()

                chromium = types.SimpleNamespace(launch_persistent_context=capture)
                owner = types.SimpleNamespace(chromium=chromium, stop=lambda: None)
                sync = lambda: types.SimpleNamespace(start=lambda: owner)
                with ExitStack() as stack:
                    stack.enter_context(patch.object(drivers.bundled, 'launch_options', lambda *_: {}))
                    stack.enter_context(patch.object(drivers.paths_mod, 'use_bundled_browsers', lambda: None))
                    stack.enter_context(patch.object(drivers, '_own_driver_tree', lambda *_: nullcontext()))
                    stack.enter_context(patch.object(drivers, '_clear_unkept', lambda *_: None))
                    stack.enter_context(patch.object(drivers, '_seed_bookmarks', lambda *_: None))
                    stack.enter_context(patch.object(drivers, '_resolve_ip_based', lambda value: value))
                    if backend == 'camoufox':
                        seen, relay = drivers._camoufox_kwargs(profile, Store(), headless)
                        if relay is not None:
                            raise ValueError('Fixture unexpectedly created proxy relay')
                    else:
                        try:
                            if backend == 'cloak':
                                sdk = types.SimpleNamespace(launch_persistent_context=capture)
                                stack.enter_context(patch.dict(sys.modules, {'cloakbrowser': sdk}))
                                stack.enter_context(patch.object(drivers, '_cloak_programs', lambda *_: []))
                                drivers._driver_cloak(profile, Store(), headless, extra_args=['--fixture-extra'])
                            else:
                                drivers._launch_playwright_like(sync, profile, Store(), headless, True,
                                                                extra_args=['--fixture-extra'], backend=backend)
                        except Captured:
                            pass
                if not seen:
                    raise ValueError('Original launch options were not observed')
                seen['user_data_dir'] = seen['user_data_dir'].replace('\\', '/')
                result.append({'name': f'{backend}-{os}-{headless}', 'backend': backend,
                               'headless': headless, 'input': json.loads(json.dumps(profile.to_dict())),
                               'extra_args': ['--fixture-extra'], 'expected': seen})
    return result
