"""Startup joins update exclusion before stores can recover or adopt state."""
import pytest

from numera.update_admission import Admission, UpdateBusy

pytestmark = pytest.mark.timeout(15)


def test_ordinary_create_app_reserves_store_initialization(tmp_path, monkeypatch):
    from numera import server
    original = server.ProfileStore
    observed = []

    def store(*args, **kwargs):
        with pytest.raises(UpdateBusy, match='admitted work'):
            Admission(tmp_path).quiesce()
        observed.append(True)
        return original(*args, **kwargs)

    monkeypatch.setattr(server, 'ProfileStore', store)
    server.create_app(str(tmp_path), start_scheduler=False)
    assert observed == [True]
    ticket = Admission(tmp_path).quiesce()
    Admission(tmp_path).resume(ticket)


def test_closed_gate_safe_restart_only_serves_ready_until_ticket_resume(tmp_path, monkeypatch):
    from fastapi.testclient import TestClient
    from numera.server import create_app

    create_app(str(tmp_path), start_scheduler=False)
    ticket = Admission(tmp_path).quiesce()
    monkeypatch.setenv('NUMERA_UPDATE_TICKET', ticket)
    monkeypatch.setenv('NUMERA_UPDATE_TOKEN', 'fixture-new-engine-private-token')
    before = {path.relative_to(tmp_path): path.read_bytes()
              for path in tmp_path.rglob('*') if path.is_file()}
    with TestClient(create_app(str(tmp_path), start_scheduler=False)) as client:
        assert client.get('/api/ready').status_code == 200
        assert client.get('/api/profiles').status_code == 409
        assert {path.relative_to(tmp_path): path.read_bytes()
                for path in tmp_path.rglob('*') if path.is_file()} == before
        response = client.post('/api/updates/resume', json={'ticket': ticket}, headers={
            'Authorization': 'Bearer fixture-new-engine-private-token'})
        assert response.status_code == 200, response.text
        assert client.get('/api/profiles').status_code == 200


@pytest.mark.parametrize('damage', ['vault', 'adoption', 'directory', 'ticket'])
def test_unsafe_recovery_startup_refuses_without_store_or_data_changes(tmp_path, monkeypatch, damage):
    from numera import server
    server.create_app(str(tmp_path), start_scheduler=False)
    ticket = Admission(tmp_path).quiesce()
    monkeypatch.setenv('NUMERA_UPDATE_TICKET', ticket if damage != 'ticket' else '0' * 64)
    if damage == 'vault':
        (tmp_path / 'vault-migration.json').write_text('{"unfinished":true}', encoding='utf-8')
    elif damage == 'adoption':
        (tmp_path / 'tokens.json').write_text('[{"hash":"fixture"}]', encoding='utf-8')
    elif damage == 'directory':
        (tmp_path / 'dashboard').rmdir()
    before = {path.relative_to(tmp_path): path.read_bytes()
              for path in tmp_path.rglob('*') if path.is_file()}
    monkeypatch.setattr(server, 'ProfileStore', lambda *args, **kwargs:
                        pytest.fail('unsafe recovery reached a mutating constructor'))
    with pytest.raises(UpdateBusy, match='recovery startup'):
        server.create_app(str(tmp_path), start_scheduler=False)
    assert {path.relative_to(tmp_path): path.read_bytes()
            for path in tmp_path.rglob('*') if path.is_file()} == before


def test_runtime_serve_safe_recovery_skips_import_and_unneeded_adoption(tmp_path, monkeypatch):
    import sys
    from types import SimpleNamespace
    from numera import firstrun, logs, paths, server
    from numera.server import runtime

    server.create_app(str(tmp_path), start_scheduler=False)
    (tmp_path / '.imported').write_text('nothing to import\n', encoding='utf-8')
    ticket = Admission(tmp_path).quiesce()
    monkeypatch.setenv('NUMERA_UPDATE_TICKET', ticket)
    monkeypatch.setenv('NUMERA_UPDATE_TOKEN', 'fixture-private-startup-token')
    monkeypatch.setattr(firstrun, 'run_if_needed', lambda root:
                        pytest.fail('recovery startup invoked legacy import'))
    monkeypatch.setattr(logs, 'setup', lambda *args, **kwargs: None)
    monkeypatch.setattr(paths, 'watch_parent', lambda **kwargs: None)
    observed = []

    def serving(app, **kwargs):
        observed.append(app)
        assert app.state.update_admission.gate.exists()
        assert list((tmp_path / '.updates/work').glob('*.json')) == []

    original_create_app = server.create_app
    monkeypatch.setattr(server, 'create_app', lambda data_dir, password=None:
                        original_create_app(data_dir, start_scheduler=False, password=password))
    monkeypatch.setitem(sys.modules, 'uvicorn', SimpleNamespace(run=serving))
    before = {path.relative_to(tmp_path): path.read_bytes()
              for path in tmp_path.rglob('*') if path.is_file()}
    runtime.serve(data_dir=str(tmp_path))
    assert len(observed) == 1
    assert {path.relative_to(tmp_path): path.read_bytes()
            for path in tmp_path.rglob('*') if path.is_file()} == before
    Admission(tmp_path).resume(ticket)


def test_actual_cli_serve_refuses_pending_import_before_opening_store(tmp_path, monkeypatch):
    from numera import cli
    from numera.server import create_app

    create_app(str(tmp_path), start_scheduler=False)
    ticket = Admission(tmp_path).quiesce()
    monkeypatch.setenv('NUMERA_UPDATE_TICKET', ticket)
    monkeypatch.setattr(cli, '_open_store', lambda args:
                        pytest.fail('closed-gate CLI startup reached store initialization'))
    assert cli.main(['--data-dir', str(tmp_path), 'serve', '--json']) == 1
    assert not (tmp_path / '.imported').exists()
    Admission(tmp_path).resume(ticket)
