"""Source conversion cases with a fixed boundary clock, not normalized outputs."""
from __future__ import annotations

from unittest.mock import patch
from numera import convert, repo
from numera.fingerprint import generate
from numera.models import Profile, Proxy


def cases() -> list[dict]:
    found = []
    original = convert.Profile
    def clocked_profile(*args, **kwargs):
        # Dataclass factories retain the original time function object, so
        # patching the module attribute alone cannot freeze these defaults.
        kwargs.setdefault('created_at', 1700000000.0)
        kwargs.setdefault('updated_at', 1700000000.0)
        return original(*args, **kwargs)
    with patch('numera.convert.Profile', side_effect=clocked_profile):
        for os_key in ('windows', 'macos', 'linux', 'android', 'ios'):
            profile = Profile(name='fixture', fingerprint=generate(seed=42, os=os_key),
                              id='fixture00001', created_at=1700000000.0, updated_at=1700000000.0,
                              proxy=Proxy(server='socks4://127.0.0.1:1080',username='fixture-user',password='fixture-value'))
            for operation, data in [('to-dashboard',profile.to_dict()),
                                    ('to-engine',convert.to_dashboard_profile(profile))]:
                expected = convert.to_dashboard_profile(profile) if operation == 'to-dashboard' else convert.to_engine_profile(data).to_dict()
                found.append({'name':f'convert-{len(found)}','operation':operation,'input':data,'expected':expected})
        for changes in ({}, {'screen':'potato'}, {'screen':'0x0'}, {'memory':0.25}, {'cores':1.5},
                        {'startupUrls':'a.test,b.test\nhttps://example.invalid/path?q=a,b'},
                        {'startupUrls':['https://example.invalid/path?a,b','about:blank']},
                        {'proxy':{'type':'SOCKS4','host':'::1','port':'1080'}},
                        {'fontList':17,'ipBased':17,'tags':[{},'text']},
                        {'canvas':None,'seed':0,'id':'stable-fixture'},
                        {'os':'iOS','osVersion':'18.5','deviceModel':'','memory':8}):
            data = {'id':'fixture00001','name':'fixture',**changes}
            found.append({'name':f'dashboard-{len(found)}','operation':'to-engine',
                          'input':data,'expected':convert.to_engine_profile(data).to_dict()})
        for record in ({}, {'proxy':None}, {'proxy':{}}, {'proxy':{'pass':None}},
                       {'proxy':{'pass':''}}, {'proxy':{'pass':'fixture-value'}}, {'proxy':[]}, None):
            found.append({'name':f'intent-{len(found)}','operation':'secret-intent',
                          'input':record,'expected':repo.secret_intent(record)})
    return found
