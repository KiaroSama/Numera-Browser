"""Retained IP-marked identity resolution with a captured country lookup only."""
from __future__ import annotations
from itertools import combinations
from unittest.mock import patch
from numera import launch_options
from numera.fingerprint import generate
from numera.models import Profile, Proxy


def cases():
    result = []
    signals = ['timezone','languages','geolocation']
    choices = [list(choice) for count in range(4) for choice in combinations(signals,count)]
    for country in ('DE','TR','ZZ',None):
        for wanted in choices:
            profile = Profile(id='a',name='fixture',created_at=1000.0,updated_at=1000.0,
                              fingerprint=generate(seed=42,os='windows',locale='en-US'),
                              proxy=Proxy(server='http://127.0.0.1:8080',country='US'),ip_based=wanted)
            before = profile.to_dict()
            calls = []
            def lookup(_proxy):
                calls.append(True)
                return country
            with patch('numera.proxypool.exit_country',lookup):
                resolved = launch_options._resolve_ip_based(profile)
            result.append({'name':f'{country}-{wanted}','input':before,'country':country,
                           'expected':resolved.to_dict(),'lookups':len(calls)})
    profile = Profile(id='a',name='fixture',created_at=1000.0,updated_at=1000.0,
                      fingerprint=generate(seed=42,os='windows',locale='en-US'),ip_based=signals)
    with patch('numera.proxypool.exit_country',side_effect=AssertionError('No-proxy lookup')):
        resolved = launch_options._resolve_ip_based(profile)
    result.append({'name':'no-proxy','input':profile.to_dict(),'country':'DE',
                   'expected':resolved.to_dict(),'lookups':0})
    return result
