"""Consent-controlled vendor acquisition, owned as a finite CLI helper."""
from __future__ import annotations

import json


def register(app, store, run_cli):
    from fastapi import HTTPException
    from .. import bundled, cloak_acquisition

    def local_offer():
        # This also applies the portable GeoIP/cache selection before the SDK
        # reads its configured local installation. It never acquires a binary.
        bundled.launch_options('cloak', store.root)
        return cloak_acquisition.offer()

    @app.get('/api/engines/cloak/acquisition')
    def get_offer():
        return local_offer()

    @app.post('/api/engines/cloak/acquisition')
    def acquire(body: dict):
        if set(body) != {'offer_id', 'approve'} or body['approve'] is not True:
            raise HTTPException(400, 'approve the exact displayed browser acquisition offer')
        current = local_offer()
        if not current['available']:
            raise HTTPException(409, current['reason'])
        if not isinstance(body['offer_id'], str) or body['offer_id'] != current['offer_id']:
            raise HTTPException(409, 'browser acquisition offer changed; review it before approving again')
        return json.loads(run_cli('engines', '--acquire-cloak', body['offer_id'],
                                  '--approve', '--json'))
