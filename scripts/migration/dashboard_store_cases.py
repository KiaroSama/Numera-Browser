"""Retained dashboard writes, revision refusals and richer-field preservation."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from numera.fingerprint import generate
from numera.models import Profile
from numera.store import ProfileStore
from numera.server.convert import to_dashboard_profile
from numera.server.stores import DashboardStore


def cases(parent: Path) -> list[dict]:
    base = to_dashboard_profile(Profile(id='fixture-dashboard',name='Dashboard α',
        fingerprint=generate(seed=42,os='windows'),created_at=1700000000.0,updated_at=1700000000.0))
    if base['proxy'] is not None:
        raise AssertionError('Dashboard storage fixture must not contain proxy credentials')
    base['proxy'] = None
    base.update(id='fixture-dashboard',folder='Work',future={'retained':True})
    parent.mkdir(parents=True,exist_ok=True)
    found=[]
    for variant in ('new','update','stale','retired-folder','duplicate-name','invalid-number'):
        with tempfile.TemporaryDirectory(prefix='dashboard-store-oracle-',dir=parent) as directory:
            root=Path(directory)
            engine=ProfileStore(root)
            dash=DashboardStore(root,engine)
            record=json.loads(json.dumps(base))
            meta={'folders':['Current'],'renamed':{'Work':'Current'},'future':7} if variant=='retired-folder' else {}
            (root/'dashboard-config.json').write_text(json.dumps(meta),encoding='utf-8')
            (root/'config.json').write_text(json.dumps({'future':7}),encoding='utf-8')
            original_profile = Profile
            def converted_profile(**values):
                values.setdefault('created_at',1700000000.0)
                values.setdefault('updated_at',1700000000.0)
                return original_profile(**values)
            with patch('numera.server.stores.time.time',return_value=1700000000.0),patch('numera.convert.Profile',side_effect=converted_profile):
                if variant in ('update','stale','duplicate-name'):
                    existing={**base,'notes':'old notes'}
                    if variant=='duplicate-name':existing['id']='fixture-other'
                    dash.save(existing)
                initial={path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                         for kind in ('profiles','dashboard') for path in sorted((root/kind).glob('*.json'))}
                if variant=='update':record.update(notes='new notes')
                if variant=='invalid-number':record['cores']=1.5
                before={name:(root/name).read_bytes() for name in initial}
                config_before=(root/'config.json').read_bytes()
                expected_revision=0 if variant=='stale' else 1 if variant=='update' else None
                request = json.loads(json.dumps(record))
                try:
                    result=dash.save(record,expect_rev=expected_revision,config_change={'default_engine':'patchright'})
                    error=None
                except (RuntimeError,ValueError) as exc:
                    result,error=None,str(exc)
                written={path.relative_to(root).as_posix():json.loads(path.read_text(encoding='utf-8'))
                         for kind in ('profiles','dashboard') for path in sorted((root/kind).glob('*.json'))}
            found.append({'name':variant,'input':request,'initial':initial,'meta':meta,'expect_rev':expected_revision,
                          'result':result,'error':error,'written':written,
                          'config':json.loads((root/'config.json').read_text(encoding='utf-8')),
                          'config_unchanged':config_before==(root/'config.json').read_bytes(),
                          'original_unchanged':all((root/name).read_bytes()==data for name,data in before.items())})
    return found
