"""Observe original cookie HTTP contracts at an explicit finite-helper boundary."""
from __future__ import annotations

import contextlib
import io
import json
import os
import tempfile
from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import patch
from fastapi.testclient import TestClient
from baseline_source import activate
activate()

from numera import cookies
from numera.cli.browser import cmd_cookies_import, cmd_cookies_export
from numera.cli.out import set_json_mode
from types import SimpleNamespace
from numera.server import create_app, routes_browser


def cases(parent):
    parent = Path(parent)
    parent.mkdir(parents=True, exist_ok=True)
    results = []
    row = {'name':'fixture', 'value':'synthetic', 'domain':'.fixture.test'}
    for name, method, body, exists, claim, authorized in [
        ('missing', 'GET', None, False, None, True),
        ('foreign', 'GET', None, True, '{', True),
        ('unauthorized', 'GET', None, True, None, False),
        ('empty', 'POST', {}, True, None, True),
        ('empty-clear', 'POST', {'cookies':[], 'clear':True}, True, None, True),
        ('invalid-clear', 'POST', {'cookies':[{'name':'incomplete'}], 'clear':True}, True, None, True),
        ('rows', 'POST', {'cookies':[row]}, True, None, True),
        ('text-replace', 'POST', {'text':json.dumps([row]), 'clear':True}, True, None, True),
        ('export', 'GET', None, True, None, True),
    ]:
        with tempfile.TemporaryDirectory(prefix='cookie-http-source-', dir=parent) as directory:
            root = Path(directory)
            seen = []
            def helper(cli, **kwargs):
                offset = cli.index('cookies')
                operation, _profile, file = cli[offset+1:offset+4]
                seen.append(operation)
                args = SimpleNamespace(ref=_profile, file=file, engine=None,
                                       format='json', clear='--clear' in cli)
                output = io.StringIO()
                errors = io.StringIO()
                set_json_mode(False)
                with contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors), \
                        patch('numera.cli.browser._resolve_or_fail', return_value=SimpleNamespace(name='Fixture')), \
                        patch.object(cookies, 'read', return_value=[cookies.normalize(row)]), \
                        patch.object(cookies, 'write', side_effect=lambda _profile, _store, jar, **_kw: len(jar)):
                    handler = cmd_cookies_export if operation == 'export' else cmd_cookies_import
                    code = handler(args, None)
                return CompletedProcess(cli, code, stdout=output.getvalue(), stderr=errors.getvalue())
            with patch.dict(os.environ, {'NUMERA_API_TOKEN':'fixture-api', 'NUMERA_ALLOWED_ORIGINS':''}), patch.object(routes_browser, 'run_helper', helper):
                app = create_app(data_dir=directory, start_scheduler=False)
                record = {'id':'a', 'name':'Fixture', 'seed':42}
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps(record), encoding='utf-8')
                if claim is not None:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim, encoding='utf-8')
                with TestClient(app, raise_server_exceptions=False) as client:
                    response = client.request(method, '/api/profiles/a/cookies', json=body,
                        headers={'authorization':'Bearer fixture-api'} if authorized else {})
            results.append({'name':name, 'method':method, 'request':body,
                            'record':record if exists else None, 'claim':claim,
                            'authorized':authorized, 'status':response.status_code,
                            'body':response.json(), 'helpers':seen})
    for result in results:
        result['path'] = '/api/profiles/a/cookies'
    return results + probe_cases(parent)


def probe_cases(parent):
    from numera.cli.browser import cmd_tls, cmd_dns
    from numera.models import Profile
    from numera.fingerprint import generate
    from numera.probe.network import check_tls
    from numera.probe.dns import check_dns
    results = []
    for operation in ['tls', 'dns']:
        for name in ['missing', 'foreign', 'unauthorized', 'healthy', 'unhealthy']:
            with tempfile.TemporaryDirectory(prefix='probe-http-source-', dir=parent) as directory:
                root = Path(directory)
                seen = []
                observation = ({'ja3':None,'ja3Hash':'fixture','ja4':'t13d1516h2_fixture',
                    'http2':None,'grease':True,'greaseKnown':True,'cipherCount':15}
                    if operation == 'tls' else {'rows':[{'type':'dns','country_code':'DE','asn':'AS1'}]})
                if name == 'unhealthy':
                    observation = None if operation == 'tls' else {'rows':[]}
                class Page:
                    def goto(self,*args,**kwargs): pass
                    def evaluate(self,*args): return observation
                context = SimpleNamespace(pages=[Page()])
                def helper(cli, **kwargs):
                    seen.append(operation)
                    assert cli[-3:] == [operation, 'a', '--json']
                    args = SimpleNamespace(ref='a', engine=None)
                    output = io.StringIO()
                    set_json_mode(True)
                    with contextlib.redirect_stdout(output), \
                            patch('numera.cli.browser._resolve_or_fail', return_value=Profile(name='Fixture',fingerprint=generate(seed=42))), \
                            patch('numera.probe.check_tls', side_effect=lambda profile,store,**kw: check_tls(profile,store,context=context)), \
                            patch('numera.probe.check_dns', side_effect=lambda profile,store,**kw: check_dns(profile,store,context=context)):
                        code = (cmd_tls if operation == 'tls' else cmd_dns)(args, None)
                    return CompletedProcess(cli, code, stdout=output.getvalue(), stderr='')
                exists = name != 'missing'
                claim = '{' if name == 'foreign' else None
                authorized = name != 'unauthorized'
                record = {'id':'a', 'name':'Fixture', 'seed':42}
                with patch.dict(os.environ, {'NUMERA_API_TOKEN':'fixture-api', 'NUMERA_ALLOWED_ORIGINS':''}), patch.object(routes_browser, 'run_helper', helper):
                    app = create_app(data_dir=directory, start_scheduler=False)
                    if exists:
                        (root/'dashboard').mkdir(exist_ok=True)
                        (root/'dashboard/a.json').write_text(json.dumps(record), encoding='utf-8')
                    if claim is not None:
                        (root/'sessions').mkdir(exist_ok=True)
                        (root/'sessions/a.json').write_text(claim, encoding='utf-8')
                    path = f'/api/profiles/a/{operation}'
                    with TestClient(app, raise_server_exceptions=False) as client:
                        response = client.post(path, headers={'authorization':'Bearer fixture-api'} if authorized else {})
                results.append({'name':f'{operation}-{name}', 'method':'POST', 'path':path,
                    'request':None, 'record':record if exists else None, 'claim':claim,
                    'authorized':authorized, 'status':response.status_code,
                    'body':response.json(), 'helpers':seen, 'probe':observation})
    return results + network_http_cases(parent) + trust_http_cases(parent) + alignment_http_cases(parent)


def network_http_cases(parent):
    from probe_cases import network_cases
    observations = network_cases()['cases']
    results = []
    for name in ['missing', 'foreign', 'unauthorized', 'healthy', 'unhealthy']:
        case = observations[0 if name == 'healthy' else 3]
        with tempfile.TemporaryDirectory(prefix='network-http-source-', dir=parent) as directory:
            root = Path(directory)
            seen = []
            def helper(cli, **kwargs):
                seen.append('proxy')
                assert cli[-4:] == ['proxy', 'test', 'a', '--json']
                return CompletedProcess(cli, 0 if case['expected']['ok'] else 1,
                                        stdout=json.dumps(case['expected']), stderr='')
            exists, authorized = name != 'missing', name != 'unauthorized'
            claim = '{' if name == 'foreign' else None
            record = {'id':'a', 'name':'Fixture', 'seed':42,'timezone':'Europe/Berlin'}
            if name == 'healthy':
                record['proxy'] = {'type':'HTTP','host':'proxy.fixture.test','port':'8080','country':'DE','user':'','pass':''}
            with patch.dict(os.environ, {'NUMERA_API_TOKEN':'fixture-api', 'NUMERA_ALLOWED_ORIGINS':''}), patch.object(routes_browser, 'run_helper', helper):
                app = create_app(data_dir=directory, start_scheduler=False)
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps(record), encoding='utf-8')
                if claim:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim, encoding='utf-8')
                with TestClient(app, raise_server_exceptions=False) as client:
                    response = client.post('/api/profiles/a/proxy-test',headers={'authorization':'Bearer fixture-api'} if authorized else {})
            results.append({'name':f'proxy-{name}','method':'POST','path':'/api/profiles/a/proxy-test',
                'request':None,'record':record if exists else None,'claim':claim,'authorized':authorized,
                'status':response.status_code,'body':response.json(),'helpers':seen,'network':case['observations']})
    return results


def alignment_http_cases(parent):
    from probe_cases import alignment_cases
    from numera.models import Profile
    from numera.store import ProfileStore
    from numera.probe.align import align_to_reality
    from numera.server.convert import to_dashboard_profile
    case = alignment_cases(Path(parent)/'alignment-inputs')['cases'][0]
    results = []
    for name in ['missing','foreign','unauthorized','success']:
        with tempfile.TemporaryDirectory(prefix='align-http-source-',dir=parent) as directory:
            root = Path(directory); seen = []
            def helper(cli,**kwargs):
                seen.append('align')
                assert cli[-3:] == ['align','a','--json']
                store = ProfileStore(root)
                profile = store.get('a')
                def observe(*args,**kwargs):
                    # Both source and native fixtures simulate an unrelated edit.
                    for folder in ['profiles','dashboard']:
                        path=root/folder/'a.json'
                        current=json.loads(path.read_text(encoding='utf-8'))
                        current['notes']='edited during probe'
                        path.write_text(json.dumps(current),encoding='utf-8')
                    return case['observation']
                with patch('numera.probe.trust._observe',observe):
                    output = align_to_reality(profile,store)
                return CompletedProcess(cli,0,stdout=json.dumps(output),stderr='')
            exists,authorized = name != 'missing', name != 'unauthorized'
            claim = '{' if name == 'foreign' else None
            profile = Profile.from_dict(case['profile'])
            if profile.proxy is not None:
                raise AssertionError('Alignment HTTP fixture must not contain proxy credentials')
            record = to_dashboard_profile(profile)
            record['proxy'] = None
            record['id']='a'
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}),patch.object(routes_browser,'run_helper',helper):
                app=create_app(data_dir=directory,start_scheduler=False)
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps(record),encoding='utf-8')
                if claim:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim,encoding='utf-8')
                with TestClient(app,raise_server_exceptions=False) as client:
                    response=client.post('/api/profiles/a/align',headers={'authorization':'Bearer fixture-api'} if authorized else {})
                stored=json.loads((root/'dashboard/a.json').read_text(encoding='utf-8')) if exists else None
            results.append({'name':f'align-{name}','method':'POST','path':'/api/profiles/a/align',
                'request':None,'record':record if exists else None,'claim':claim,'authorized':authorized,
                'status':response.status_code,'body':response.json(),'helpers':seen,'probe':case['observation'],
                'alignment':True,'stored':stored})
    return results


def trust_http_cases(parent):
    from probe_cases import trust_cases
    observations = trust_cases()['cases']
    results = []
    for name in ['missing','foreign','unauthorized','healthy','unhealthy']:
        case = observations[0 if name == 'healthy' else 1]
        with tempfile.TemporaryDirectory(prefix='trust-http-source-',dir=parent) as directory:
            root = Path(directory)
            seen = []
            def helper(cli,**kwargs):
                seen.append('trust')
                assert cli[-3:] == ['trust','a','--json']
                return CompletedProcess(cli,0,stdout=json.dumps(case['expected']),stderr='')
            exists, authorized = name != 'missing', name != 'unauthorized'
            claim = '{' if name == 'foreign' else None
            # Explicit source fingerprint fields keep the materialized view equal
            # to the independent original grade fixture, not a seeded approximation.
            from numera.server.convert import to_dashboard_profile
            from numera.models import Profile
            profile = Profile.from_dict(case['profile'])
            if profile.proxy is not None:
                raise AssertionError('Trust HTTP fixture must not contain proxy credentials')
            record = to_dashboard_profile(profile)
            record['proxy'] = None
            record['id']='a'
            with patch.dict(os.environ,{'NUMERA_API_TOKEN':'fixture-api','NUMERA_ALLOWED_ORIGINS':''}),patch.object(routes_browser,'run_helper',helper):
                app = create_app(data_dir=directory,start_scheduler=False)
                if exists:
                    (root/'dashboard').mkdir(exist_ok=True)
                    (root/'dashboard/a.json').write_text(json.dumps(record),encoding='utf-8')
                if claim:
                    (root/'sessions').mkdir(exist_ok=True)
                    (root/'sessions/a.json').write_text(claim,encoding='utf-8')
                with TestClient(app,raise_server_exceptions=False) as client:
                    response = client.post('/api/profiles/a/trust',headers={'authorization':'Bearer fixture-api'} if authorized else {})
                stored = json.loads((root/'dashboard/a.json').read_text(encoding='utf-8')) if exists else None
            results.append({'name':f'trust-{name}','method':'POST','path':'/api/profiles/a/trust',
                'request':None,'record':record if exists else None,'claim':claim,'authorized':authorized,
                'status':response.status_code,'body':response.json(),'helpers':seen,'probe':case['observation'],
                'trust':stored.get('_trust') if stored else None})
    return results
