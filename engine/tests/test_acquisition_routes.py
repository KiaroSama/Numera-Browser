"""Cloak metadata is local; only an exact approved offer starts an owned helper."""
import json

from fastapi.testclient import TestClient
import pytest

pytestmark = pytest.mark.timeout(10)


def test_http_offer_decline_stale_and_approval_use_exact_owned_cli(tmp_path, monkeypatch):
    from numera import cloak_acquisition
    from numera.server import create_app
    from numera.server import routes_browser
    from subprocess import CompletedProcess

    identity = {'engine': 'cloak', 'offer_id': '1' * 64, 'available': True,
                'installed': False, 'version': '146.0.7680.177.5', 'reason': None}
    monkeypatch.setattr('numera.bundled.launch_options', lambda *args: {})
    monkeypatch.setattr(cloak_acquisition, 'offer', lambda: identity)
    calls = []

    def run(cli, **kwargs):
        calls.append((cli, kwargs))
        return CompletedProcess(cli, 0, json.dumps({'installed': True}), '')

    monkeypatch.setattr(routes_browser, 'run_helper', run)
    app = create_app(str(tmp_path), start_scheduler=False)
    with TestClient(app) as client:
        assert client.get('/api/engines/cloak/acquisition').json() == identity
        assert client.post('/api/engines/cloak/acquisition', json={
            'offer_id': identity['offer_id'], 'approve': False}).status_code == 400
        assert client.post('/api/engines/cloak/acquisition', json={
            'offer_id': '2' * 64, 'approve': True}).status_code == 409
        assert not calls
        response = client.post('/api/engines/cloak/acquisition', json={
            'offer_id': identity['offer_id'], 'approve': True})
        assert response.status_code == 200, response.text
    assert len(calls) == 1
    argv, options = calls[0]
    assert argv[-5:] == ['engines', '--acquire-cloak', identity['offer_id'], '--approve', '--json']
    assert options['manager'] is app.state.helpers
    assert options['manager'].root == tmp_path
