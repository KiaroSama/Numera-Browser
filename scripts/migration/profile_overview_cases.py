"""Original profile health/collision HTTP outcomes, read-only isolated inputs."""
from __future__ import annotations

import copy
import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch
from fastapi.testclient import TestClient
from numera.server import create_app


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    variants = [
        ('empty', [], [], None),
        ('ordinary', [{'id':'a','name':'Fixture','seed':42}], [], None),
        ('collision', [{'id':'a','name':'First','seed':42}, {'id':'b','name':'Second','seed':43}], [], None),
        ('identical', [{'id':'a','name':'First','seed':42}, {'id':'b','name':'Second','seed':42}], [], None),
        ('distinct', [{'id':'a','name':'First','seed':42}, {'id':'b','name':'Second','seed':43,'screen':'1280x720'}], [], None),
        ('damaged-view', [{'id':'a','name':123,'seed':42,'proxy':{'type':'INVALID','host':'fixture.invalid'}}], [], None),
        ('incoherent', [{'id':'a','name':'Fixture','seed':42,'locale':'en-US','timezone':'Europe/London'}], [], None),
        ('cached-grade', [{'id':'a','name':'Fixture','seed':42,'_trust':{'grade':'C','score':60}}], [], None),
        ('null-proxy', [{'id':'a','name':'Fixture','seed':42,'proxy':None}], [], None),
        ('proxy', [{'id':'a','name':'Fixture','seed':42,'proxy':{'type':'HTTP','host':'fixture.invalid','country':'US'}}], [], None),
        ('schedule', [{'id':'a','name':'Fixture','seed':42}], [
            {'id':'slow','profileId':'a','everyHours':24,'preset':'general'},
            {'id':'fast','profileId':'a','everyHours':2,'preset':'social','enabled':False}], None),
        ('unreadable-claim', [{'id':'a','name':'Fixture','seed':42}], [], '{'),
        ('recent-activity', [{'id':'a','name':'Fixture','seed':42,'_last_active':1700000000.0}], [], None),
    ]
    found = []
    for name, profiles, schedules, claim in variants:
        for route in ('health', 'collisions'):
            with tempfile.TemporaryDirectory(prefix='profile-overview-oracle-', dir=parent) as directory:
                root = Path(directory)
                with patch.dict(os.environ, {'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}):
                    app = create_app(data_dir=directory, start_scheduler=False)
                    (root/'dashboard').mkdir(exist_ok=True)
                    for profile in profiles:
                        (root/'dashboard'/f'{profile["id"]}.json').write_text(json.dumps(profile), encoding='utf-8')
                    (root/'schedules.json').write_text(json.dumps(schedules), encoding='utf-8')
                    if claim is not None:
                        (root/'sessions').mkdir(exist_ok=True)
                        (root/'sessions/a.json').write_text(claim, encoding='utf-8')
                    before = {path.name:path.read_bytes() for path in (root/'dashboard').glob('*.json')}
                    with patch('numera.convert.time.time', return_value=1700000030.0), TestClient(app, raise_server_exceptions=False) as client:
                        response = client.get(f'/api/profiles/{route}', headers={'authorization':'Bearer fixture-api'})
                found.append({'name':f'{name}-{route}','url':f'/api/profiles/{route}',
                              'profiles':copy.deepcopy(profiles),'schedules':schedules,'claim':claim,
                              'status':response.status_code,'body':response.json() if response.headers.get('content-type','').startswith('application/json') else response.text,
                              'unchanged':all(path.read_bytes()==before[path.name] for path in (root/'dashboard').glob('*.json'))})
    return found
