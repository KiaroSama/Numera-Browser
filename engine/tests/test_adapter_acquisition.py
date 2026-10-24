"""SDK-only Cloak calls cannot acquire browsers or enable background updates."""
import builtins
import importlib.util
from pathlib import Path
from types import SimpleNamespace

import pytest

pytestmark = pytest.mark.timeout(10)


def adapter():
    source = Path(__file__).resolve().parents[2] / 'scripts/browser-adapter/adapter.py'
    spec = importlib.util.spec_from_file_location('consent_sdk_adapter', source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('selection', ['absent', 'empty', 'missing', 'directory'])
def test_missing_selected_binary_refuses_before_vendor_import(tmp_path, monkeypatch, selection):
    module = adapter()
    monkeypatch.setenv('CLOAKBROWSER_AUTO_UPDATE', 'true')
    if selection == 'absent':
        monkeypatch.delenv('CLOAKBROWSER_BINARY_PATH', raising=False)
    else:
        value = '' if selection == 'empty' else str(tmp_path if selection == 'directory'
                                                  else tmp_path / 'missing-browser')
        monkeypatch.setenv('CLOAKBROWSER_BINARY_PATH', value)
    imported = []
    real_import = builtins.__import__

    def vendor_import(name, *args, **kwargs):
        if name == 'cloakbrowser':
            imported.append(name)
            raise AssertionError('unapproved acquisition reached vendor import')
        return real_import(name, *args, **kwargs)

    monkeypatch.setattr(builtins, '__import__', vendor_import)
    with pytest.raises(RuntimeError, match='existing native-selected browser binary'):
        module.open_context('cloak', {'user_data_dir': str(tmp_path / 'profile')})
    assert not imported
    assert module.os.environ['CLOAKBROWSER_AUTO_UPDATE'] == 'false'
    assert not (tmp_path / 'profile').exists()


def test_existing_native_selection_disables_updates_before_import_and_preserves_sdk_options(
        tmp_path, monkeypatch):
    module = adapter()
    executable = tmp_path / 'external selected browser'
    executable.write_bytes(b'fixture executable; never executed')
    monkeypatch.setenv('CLOAKBROWSER_BINARY_PATH', str(executable))
    monkeypatch.setenv('CLOAKBROWSER_AUTO_UPDATE', 'true')
    options = {'user_data_dir': str(tmp_path / 'profile'), 'headless': True,
               'browser_version': '146.0.7680.177.5'}
    context = object()
    calls = []
    imports = []
    real_import = builtins.__import__

    def launch(**received):
        calls.append(received)
        assert module.os.environ['CLOAKBROWSER_BINARY_PATH'] == str(executable)
        assert module.os.environ['CLOAKBROWSER_AUTO_UPDATE'] == 'false'
        return context

    vendor = SimpleNamespace(launch_persistent_context=launch)

    def vendor_import(name, *args, **kwargs):
        if name == 'cloakbrowser':
            assert module.os.environ['CLOAKBROWSER_AUTO_UPDATE'] == 'false'
            imports.append(name)
            return vendor
        return real_import(name, *args, **kwargs)

    monkeypatch.setattr(builtins, '__import__', vendor_import)
    assert module.open_context('cloak', options) == (None, context, None)
    assert imports == ['cloakbrowser'] and calls == [options]
    assert executable.read_bytes() == b'fixture executable; never executed'
    assert options == {'user_data_dir': str(tmp_path / 'profile'), 'headless': True,
                       'browser_version': '146.0.7680.177.5'}


@pytest.mark.parametrize('failure', [False, True])
def test_sdk_operation_timing_never_logs_payloads(tmp_path, monkeypatch, failure):
    import io
    import sys
    module = adapter()
    monkeypatch.delenv('NUMERA_ADAPTER_FIRST_PAGE', raising=False)
    output = io.BytesIO()
    events = []
    clock = iter([100, 110, 130, 140, 170, 180, 200, 210])
    monkeypatch.setattr(module.os, 'dup', lambda fd: 99)
    monkeypatch.setattr(module.os, 'set_inheritable', lambda *args: None)
    monkeypatch.setattr(module.os, 'fdopen', lambda *args: output)
    monkeypatch.setattr(module.os, 'dup2', lambda *args, **kwargs: None)
    monkeypatch.setattr(module.sys, 'stdout', SimpleNamespace(fileno=lambda: 1))
    monkeypatch.setattr(module.sys, 'stdin', SimpleNamespace(buffer=object()))
    monkeypatch.setattr(module.time, 'perf_counter_ns', lambda: next(clock))
    log = SimpleNamespace(info=lambda message, *args: events.append(message % args),
                          error=lambda message, *args: events.append(message % args), handlers=[])
    monkeypatch.setattr(module, 'logger', lambda: log)
    monkeypatch.setattr(module, 'available', lambda: {'fixture': True})
    frames = iter([
        (True, {'id': 1, 'protocol': 1, 'operation': 'hello'}),
        (True, {'id': 2, 'protocol': 1, 'operation': 'open', 'backend': 'cloak',
                'options': {'user_data_dir': 'private-profile', 'password': 'private-value'}}),
        (True, {'id': 3, 'protocol': 1, 'operation': 'close'}),
    ])
    monkeypatch.setitem(sys.modules, 'frame_input', SimpleNamespace(
        FrameInput=lambda *args: SimpleNamespace(poll=lambda: next(frames))))
    context = SimpleNamespace(on=lambda *args: None, pages=[object()], close=lambda: None)

    def opened(*args):
        if failure:
            raise RuntimeError('private-value private-profile')
        return None, context, None

    monkeypatch.setattr(module, 'open_context', opened)
    assert module.main() == int(failure)
    timings = [event for event in events if event.startswith('SDK timing ')]
    assert timings == [
        'SDK timing operation=hello elapsed_ns=10',
        'SDK timing operation=open elapsed_ns=10',
        *([] if failure else ['SDK timing operation=close elapsed_ns=10']),
        'SDK timing operation=cleanup elapsed_ns=10',
    ]
    assert not any('private-' in event for event in events)
    assert output.closed
