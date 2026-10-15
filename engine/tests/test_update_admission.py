"""Feature005: one durable gate serializes actual work and update activation."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading

import pytest

from numera import proctree, sessions

pytestmark = pytest.mark.timeout(15)
TOKEN = "fixture-private-update-token"


def test_gate_blocks_real_session_claim_and_cli_until_exact_ticket_resume(tmp_path):
    from numera.update_admission import Admission, UpdateBusy

    admission = Admission(tmp_path)
    ticket = admission.quiesce()
    record = json.loads((tmp_path / '.updates/admission.json').read_text(encoding='utf-8'))
    assert record['ticket_sha256'] == hashlib.sha256(ticket.encode('ascii')).hexdigest()
    assert ticket not in json.dumps(record)
    tree = proctree.Tree(os.getpid(), proctree.started_at(os.getpid()), 12345, 'pgid')
    with pytest.raises(UpdateBusy):
        sessions.take(tmp_path, 'profile-one', tree)
    result = subprocess.run(
        [sys.executable, '-m', 'numera', '--data-dir', str(tmp_path), 'list', '--json'],
        capture_output=True, text=True, encoding='utf-8', timeout=5,
        env={**os.environ, 'NUMERA_UPDATE_TOKEN': TOKEN},
    )
    assert result.returncode == 1
    assert 'update' in result.stdout.lower()
    with pytest.raises(UpdateBusy):
        admission.resume('0' * 64)
    admission.resume(ticket)
    with admission.work('fixture'):
        pass
    assert not (tmp_path / '.updates/admission.json').exists()


def test_precommitted_ticket_recovers_after_quiesce_response_is_lost(tmp_path):
    from numera.update_admission import Admission

    ticket = 'b' * 64
    Admission(tmp_path).quiesce(ticket=ticket)
    # No returned receipt is needed: the initiating shell already owns the ticket.
    Admission(tmp_path).resume(ticket)
    assert not (tmp_path / '.updates/admission.json').exists()


def test_resume_receipt_is_idempotent_but_never_authorizes_another_gate(tmp_path):
    from numera.update_admission import Admission, UpdateBusy

    admission = Admission(tmp_path)
    ticket = admission.quiesce()
    admission.resume(ticket)
    admission.resume(ticket)  # Recovery after the successful HTTP response was lost.
    with pytest.raises(UpdateBusy):
        admission.resume('0' * 64)
    next_ticket = admission.quiesce()
    with pytest.raises(UpdateBusy):
        admission.resume(ticket)
    assert admission.gate.exists()
    admission.resume(next_ticket)


def test_resume_receipt_failure_keeps_admission_closed(tmp_path, monkeypatch):
    import numera.update_admission as module

    admission = module.Admission(tmp_path)
    ticket = admission.quiesce()
    original = module.write_json

    def write(path, value):
        if Path(path).name == 'resumed.json':
            raise OSError('controlled receipt failure')
        return original(path, value)

    monkeypatch.setattr(module, 'write_json', write)
    with pytest.raises(OSError, match='controlled receipt'):
        admission.resume(ticket)
    assert admission.gate.exists(), 'admission opened before durable recovery evidence'
    monkeypatch.setattr(module, 'write_json', original)
    admission.resume(ticket)


def test_inflight_work_refuses_quiesce_without_closing_admission(tmp_path):
    from numera.update_admission import Admission, UpdateBusy

    admission = Admission(tmp_path)
    with admission.work('finite helper'):
        with pytest.raises(UpdateBusy, match='active|uncertain'):
            admission.quiesce()
    with admission.work('still available'):
        pass
    assert not (tmp_path / '.updates/admission.json').exists()


def test_unsettled_helper_reservation_survives_its_parent_scope(tmp_path):
    from numera.update_admission import Admission, UpdateBusy
    admission = Admission(tmp_path)
    with admission.work('stranded helper', settled=lambda: False):
        pass
    with pytest.raises(UpdateBusy):
        admission.quiesce()
    assert len(list((tmp_path / '.updates/work').glob('*.json'))) == 1


def test_unknown_claim_or_gate_never_expires_into_available(tmp_path):
    from numera.update_admission import Admission, UpdateBusy

    admission = Admission(tmp_path)
    claims = tmp_path / 'sessions'
    claims.mkdir()
    (claims / 'uncertain.json').write_text('{', encoding='utf-8')
    with pytest.raises(UpdateBusy):
        admission.quiesce()
    assert not (tmp_path / '.updates/admission.json').exists()
    (claims / 'uncertain.json').unlink()
    gate = tmp_path / '.updates/admission.json'
    gate.write_text('{"created_at":0}', encoding='utf-8')
    with pytest.raises(UpdateBusy):
        with admission.work('must not run'):
            pytest.fail('corrupt old gate admitted work')


def test_thread_race_has_no_successful_quiesce_beside_admitted_work(tmp_path):
    from numera.update_admission import Admission, UpdateBusy

    admission = Admission(tmp_path)
    ready, finish = threading.Event(), threading.Event()
    failures = []

    def work():
        try:
            with admission.work('concurrent CLI'):
                ready.set()
                assert finish.wait(3)
        except BaseException as error:
            failures.append(error)

    thread = threading.Thread(target=work)
    thread.start()
    try:
        assert ready.wait(3)
        with pytest.raises(UpdateBusy):
            admission.quiesce()
    finally:
        finish.set()
        thread.join(3)
    assert not thread.is_alive() and not failures
    ticket = admission.quiesce()
    admission.resume(ticket)


def test_actual_session_claim_defers_then_proved_stop_allows_quiesce(tmp_path):
    from numera.update_admission import Admission, UpdateBusy
    proc, tree = proctree.spawn([sys.executable, '-c',
        'import threading; threading.Event().wait()'], stdin=subprocess.DEVNULL)
    claim = None
    try:
        claim = sessions.take(tmp_path, 'real-session', tree)
        assert isinstance(claim['session'], str) and claim['session']
        with pytest.raises(UpdateBusy, match='browser session'):
            Admission(tmp_path).quiesce()
        proctree.end_session(proc, tree, grace=2)
        # Exercise state(record) on the actual still-present stopped claim,
        # not merely an empty sessions directory after metadata cleanup.
        assert sessions.read(tmp_path, 'real-session')['session'] == claim['session']
        ticket = Admission(tmp_path).quiesce()
        Admission(tmp_path).resume(ticket)
        assert sessions.settle(tmp_path, 'real-session')
    finally:
        if proc.poll() is None or proctree.alive_tree(tree):
            proctree.end_session(proc, tree, grace=2)
        if claim:
            sessions.release(tmp_path, 'real-session', claim['session'], tree=tree)


def test_cancelled_http_await_does_not_release_still_executing_handler(tmp_path, monkeypatch):
    import asyncio
    from fastapi.routing import APIRoute
    from numera.server import create_app
    from numera.update_admission import Admission, UpdateBusy

    app = create_app(str(tmp_path), start_scheduler=False)
    entered, finish = threading.Event(), threading.Event()

    @app.get('/api/fixture-slow')
    def slow():
        entered.set()
        assert finish.wait(3)
        return {'ok': True}

    route = next(route for route in app.routes
                 if isinstance(route, APIRoute) and route.path == '/api/fixture-slow')
    async def canceled_client():
        # asyncio.to_thread deliberately abandons its await on cancellation;
        # the actual FastAPI-bound sync handler must own the durable guard.
        task = asyncio.create_task(asyncio.to_thread(route.dependant.call))
        try:
            assert await asyncio.to_thread(entered.wait, 3)
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
            with pytest.raises(UpdateBusy):
                Admission(tmp_path).quiesce()
        finally:
            finish.set()

    asyncio.run(canceled_client())
    ticket = Admission(tmp_path).quiesce()
    Admission(tmp_path).resume(ticket)


def test_private_http_token_is_not_public_api_token_and_survives_restart(tmp_path, monkeypatch):
    from fastapi.testclient import TestClient
    from numera.server import create_app

    monkeypatch.setenv('NUMERA_UPDATE_TOKEN', TOKEN)
    monkeypatch.setenv('NUMERA_API_TOKEN', 'fixture-public-api-token')
    app = create_app(str(tmp_path), start_scheduler=False)
    private = {'Authorization': 'Bearer ' + TOKEN}
    with TestClient(app) as client:
        assert client.post('/api/updates/quiesce', json={}).status_code == 401
        assert client.post('/api/updates/quiesce', json={}, headers={
            'Authorization': 'Bearer fixture-public-api-token'}).status_code == 401
        assert client.post('/api/updates/prepare', content=b' ' * 4097, headers=private).status_code == 400
        prepared = client.post('/api/updates/prepare', json={}, headers=private)
        assert prepared.status_code == 200 and Path(prepared.json()['root']) == tmp_path.resolve()
        ticket = 'a' * 64  # Represents the shell's already durable private journal.
        response = client.post('/api/updates/quiesce', json={'ticket': ticket}, headers=private)
        assert response.status_code == 200, response.text
        assert response.json()['ticket'] == ticket
        assert Path(response.json()['root']) == tmp_path.resolve()
        assert client.get('/api/ready').status_code == 200
        assert client.get('/api/profiles', headers={
            'Authorization': 'Bearer fixture-public-api-token'}).status_code == 409
    # Engine exit never opens the durable gate. New engine private token differs.
    monkeypatch.setenv('NUMERA_UPDATE_TOKEN', 'fixture-next-engine-token')
    monkeypatch.setenv('NUMERA_UPDATE_TICKET', ticket)
    with TestClient(create_app(str(tmp_path), start_scheduler=False)) as client:
        assert client.post('/api/updates/resume', json={'ticket': ticket},
                           headers=private).status_code == 401
        response = client.post('/api/updates/resume', json={'ticket': ticket}, headers={
            'Authorization': 'Bearer fixture-next-engine-token'})
        assert response.status_code == 200, response.text
        assert client.get('/api/profiles', headers={
            'Authorization': 'Bearer fixture-public-api-token'}).status_code == 200
        schema = client.get('/openapi.json').json()['paths']
        assert '/api/updates/quiesce' not in schema and '/api/updates/prepare' not in schema


def test_closed_gate_refuses_named_token_auth_before_last_used_write(tmp_path, monkeypatch):
    from fastapi.testclient import TestClient
    from numera.server import create_app
    from numera.server.stores import TokenStore
    from numera.update_admission import Admission

    monkeypatch.delenv('NUMERA_API_TOKEN', raising=False)
    tokens = TokenStore(tmp_path)
    token = tokens.create('fixture-update-auth')['token']
    app = create_app(str(tmp_path), start_scheduler=False)
    original = tokens.path.read_bytes()
    ticket = Admission(tmp_path).quiesce()
    try:
        with TestClient(app) as client:
            result = client.get('/api/profiles', headers={'Authorization': 'Bearer ' + token})
            assert result.status_code == 409, result.text
            assert tokens.path.read_bytes() == original, 'closed admission still stamped named-token usage'
    finally:
        Admission(tmp_path).resume(ticket)


def test_abandoned_named_auth_worker_retains_admission_until_last_used_settles(tmp_path):
    import asyncio
    from numera.server.routes_updates import verify_named_token
    from numera.update_admission import Admission, UpdateBusy

    entered, finish = threading.Event(), threading.Event()
    class Tokens:
        def verify(self, supplied):
            assert supplied == 'fixture-token'
            entered.set()
            assert finish.wait(3)
            return {'id': 'fixture'}

    async def canceled_request():
        task = asyncio.create_task(verify_named_token(Admission(tmp_path), Tokens(), 'fixture-token'))
        try:
            assert await asyncio.to_thread(entered.wait, 3)
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
            with pytest.raises(UpdateBusy):
                Admission(tmp_path).quiesce()
        finally:
            finish.set()

    asyncio.run(canceled_request())
    import time
    deadline = time.monotonic() + 3
    while list((tmp_path / '.updates/work').glob('*.json')) and time.monotonic() < deadline:
        time.sleep(0.01)
    assert not list((tmp_path / '.updates/work').glob('*.json'))
    ticket = Admission(tmp_path).quiesce()
    Admission(tmp_path).resume(ticket)


def test_private_token_is_not_inherited_by_browser_helper_children(monkeypatch):
    from numera.paths import child_env

    monkeypatch.setenv('NUMERA_UPDATE_TOKEN', TOKEN)
    monkeypatch.setenv('NUMERA_UPDATE_TICKET', 'fixture-recovery-ticket')
    monkeypatch.setenv('NUMERA_UPDATE_ACTIVATION', 'fixture-activation-nonce')
    env = child_env()
    assert 'NUMERA_UPDATE_TOKEN' not in env
    assert 'NUMERA_UPDATE_TICKET' not in env
    assert 'NUMERA_UPDATE_ACTIVATION' not in env
