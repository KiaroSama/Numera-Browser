"""HTTP replay sends explicit UTF-8 bytes and retains bounded response handling."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    import baseline_source
    monkeypatch.setattr(baseline_source, 'source_root', lambda: ROOT)
    spec = importlib.util.spec_from_file_location(
        'http_replay_fixture', ROOT / 'scripts/migration/native_http_acceptance.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('payload', [None, {'notes': 'Fixture α 日本語', '_rev': 3}])
def test_request_retains_auth_headers_and_sends_json_as_utf8_bytes(monkeypatch, payload):
    helper = load(monkeypatch)
    calls = []

    class Response:
        status = 200

        def read(self, size):
            assert size == 65537
            return b'{"ok":true}'

        def getheaders(self):
            return [('Content-Type', 'application/json')]

    class Connection:
        def __init__(self, host, port, timeout):
            assert (host, port, timeout) == ('127.0.0.1', 43210, 2)

        def request(self, method, path, body=None, headers=None):
            calls.append((method, path, body, headers))

        def getresponse(self):
            return Response()

        def close(self):
            calls.append('closed')

    monkeypatch.setattr(helper.http.client, 'HTTPConnection', Connection)
    assert helper.request(43210, 'PUT', '/api/profiles/fixture', 'fixture-token', body=payload) == (
        200, {'content-type': 'application/json'}, {'ok': True})
    method, path, body, headers = calls[0]
    assert (method, path) == ('PUT', '/api/profiles/fixture')
    assert headers['Authorization'] == 'Bearer fixture-token'
    if payload is None:
        assert body is None and 'Content-Type' not in headers
    else:
        assert isinstance(body, bytes) and json.loads(body.decode('utf-8')) == payload
        assert headers['Content-Type'] == 'application/json'
    assert calls[1:] == ['closed']


@pytest.mark.parametrize('failure', ['send', 'oversize', 'malformed'])
def test_request_failures_close_the_connection_without_accepting_output(monkeypatch, failure):
    helper = load(monkeypatch)
    closed = []

    class Connection:
        status = 200

        def __init__(self, *args, **kwargs):
            pass

        def request(self, *args, **kwargs):
            if failure == 'send':
                raise OSError('Synthetic request failure')

        def getresponse(self):
            return self

        def read(self, size):
            assert size == 65537
            return b'x' * size if failure == 'oversize' else b'{'

        def getheaders(self):
            return [('Content-Type', 'application/json')]

        def close(self):
            closed.append(True)

    monkeypatch.setattr(helper.http.client, 'HTTPConnection', Connection)
    with pytest.raises((OSError, ValueError)):
        helper.request(43210, 'PUT', '/api/profiles/fixture', body={'notes': 'fixture'})
    assert closed == [True]


@pytest.mark.parametrize('damage', ['none', 'identity', 'secret', 'fingerprint', 'unknown',
                                   'created-before', 'created-after', 'updated-bool', 'updated-nonfinite'])
def test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps(monkeypatch, damage):
    import copy

    helper = load(monkeypatch)
    expected = {'id': 'fixture', 'notes': 'kept', 'created_at': 1000, 'updated_at': 1001,
                'fingerprint': {'seed': 42}, 'proxy': {'password': 'synthetic'},
                'future': [42, True]}
    observed = copy.deepcopy({**expected, 'created_at': 1002, 'updated_at': 1003})
    if damage == 'identity':
        observed['id'] = 'other'
    elif damage == 'secret':
        observed['proxy']['password'] = None
    elif damage == 'fingerprint':
        observed['fingerprint']['seed'] = 43
    elif damage == 'unknown':
        observed.pop('future')
    elif damage == 'created-before':
        observed['created_at'] = 1000
    elif damage == 'created-after':
        observed['created_at'] = 1005
    elif damage == 'updated-bool':
        observed['updated_at'] = True
    elif damage == 'updated-nonfinite':
        observed['updated_at'] = float('nan')
    if damage == 'none':
        helper.require_http_profile(observed, expected, 1001, 1004)
    else:
        with pytest.raises(AssertionError):
            helper.require_http_profile(observed, expected, 1001, 1004)
    assert expected['created_at'] == 1000 and expected['updated_at'] == 1001


@pytest.mark.parametrize('failure', [False, True])
def test_main_removes_only_successful_fixtures_and_retains_failed_owned_state(tmp_path, monkeypatch, failure):
    import logging

    helper = load(monkeypatch)
    temporary = tmp_path / 'owned-http'
    temporary.mkdir()
    marker = temporary / 'retained-owner.json'
    marker.write_bytes(b'owned fixture evidence')
    monkeypatch.setattr(helper.tempfile, 'mkdtemp', lambda **kwargs: str(temporary))
    monkeypatch.setattr(helper, 'observe', lambda *args, **kwargs: ([], []))

    def replay(*args):
        if failure:
            raise RuntimeError('Synthetic unsettled owner')

    monkeypatch.setattr(helper, 'shared_store_replay', replay)
    logger = logging.getLogger('http-retention-fixture')
    if failure:
        with pytest.raises(RuntimeError, match='Synthetic unsettled owner'):
            helper.main(tmp_path / 'inert-candidate', logger)
        assert marker.read_bytes() == b'owned fixture evidence'
    else:
        assert helper.main(tmp_path / 'inert-candidate', logger) == 0
        assert not temporary.exists()
