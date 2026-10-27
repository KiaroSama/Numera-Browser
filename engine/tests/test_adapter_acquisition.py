"""SDK-only Cloak calls cannot acquire browsers or enable background updates."""
import builtins
import importlib.util
from pathlib import Path
from types import SimpleNamespace

import pytest

pytestmark = pytest.mark.timeout(10)


def adapter(legacy=None):
    source = Path(__file__).resolve().parents[2] / 'scripts/browser-adapter/adapter.py'
    spec = importlib.util.spec_from_file_location('consent_sdk_adapter', source)
    module = importlib.util.module_from_spec(spec)
    if legacy:
        text = source.read_text(encoding='utf-8')
        before, after = {
            'poll': ('frames.poll(.025)', 'frames.poll()'),
            'pump': ('wait_for_timeout(0)', 'wait_for_timeout(25)'),
            'fragment-wait': ('frames.poll(.025)', 'frames.poll(.025 if context is not None else 0)'),
        }[legacy]
        assert before in text
        text = text.replace(before, after)
        if legacy == 'fragment-wait':
            before = "raise TimeoutError('Input deadline exceeded')"
            assert before in text
            text = text.replace(before, before + '\n                    closed.wait(min(0.025, remaining))')
        exec(compile(text, str(source), 'exec'), module.__dict__)
    else:
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


@pytest.mark.parametrize('failure,idle,fragments,legacy', [
    (False, False, 0, None), (False, True, 0, None), (True, False, 0, None),
    (False, False, 0, 'poll'), (False, True, 0, 'pump'),
    (False, False, 7, None), (True, False, 7, None),
    (False, False, 7, 'fragment-wait'),
])
def test_sdk_operation_timing_never_logs_payloads(tmp_path, monkeypatch, failure, idle, fragments, legacy):
    import io
    import sys
    module = adapter(legacy)
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
    monkeypatch.setattr(module, 'time', SimpleNamespace(
        perf_counter_ns=lambda: next(clock), monotonic=lambda: 0))
    log = SimpleNamespace(info=lambda message, *args: events.append(message % args),
                          error=lambda message, *args: events.append(message % args), handlers=[])
    monkeypatch.setattr(module, 'logger', lambda: log)
    monkeypatch.setattr(module, 'available', lambda: {'fixture': True})
    frames = iter([
        (True, {'id': 1, 'protocol': 1, 'operation': 'hello'}),
        *[(False, None)] * fragments,
        (True, {'id': 2, 'protocol': 1, 'operation': 'open', 'backend': 'cloak',
                'options': {'user_data_dir': 'private-profile', 'password': 'private-value'}}),
        *([(False, None)] if idle else []),
        (True, {'id': 3, 'protocol': 1, 'operation': 'close'}),
    ])
    waits, pumps, fragment_waits = [], [], []
    monkeypatch.setattr(module, 'threading', SimpleNamespace(Event=lambda: SimpleNamespace(
        is_set=lambda: False, set=lambda: None, wait=lambda delay: fragment_waits.append(delay))))

    def poll(timeout=0):
        waits.append(timeout)
        return next(frames)

    monkeypatch.setitem(sys.modules, 'frame_input', SimpleNamespace(
        FrameInput=lambda *args: SimpleNamespace(poll=poll)))
    page = SimpleNamespace(wait_for_timeout=lambda delay: pumps.append(delay))
    context = SimpleNamespace(on=lambda *args: None, pages=[page], close=lambda: None)

    def opened(*args):
        if failure:
            raise RuntimeError('private-value private-profile')
        return None, context, None

    monkeypatch.setattr(module, 'open_context', opened)
    assert module.main() == int(failure)

    def scheduling():
        assert not fragment_waits, 'Partial input must not add fixed fragment waits'
        assert waits == [.025] * (2 + fragments + (0 if failure else 2 if idle else 1)), 'SDK commands must wait on input readiness'
        assert pumps == ([0] if idle and not failure else []), 'Idle SDK dispatch must not add a fixed command delay'

    if legacy:
        symptom = {'poll': 'input readiness', 'pump': 'fixed command delay',
                   'fragment-wait': 'fixed fragment waits'}[legacy]
        with pytest.raises(AssertionError, match=symptom):
            scheduling()
    else:
        scheduling()
    timings = [event for event in events if event.startswith('SDK timing ')]
    assert timings == [
        'SDK timing operation=hello elapsed_ns=10',
        'SDK timing operation=open elapsed_ns=10',
        *([] if failure else ['SDK timing operation=close elapsed_ns=10']),
        'SDK timing operation=cleanup elapsed_ns=10',
    ]
    assert not any('private-' in event for event in events)
    assert output.closed


def frame_input():
    source = Path(__file__).resolve().parents[2] / 'scripts/browser-adapter/frame_input.py'
    spec = importlib.util.spec_from_file_location('sdk_frame_fixture', source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('mode', ['ready', 'idle', 'buffered', 'partial', 'eof',
                                 'partial-eof', 'oversized', 'invalid-json', 'nonobject', 'read-error', 'would-block'])
def test_frame_input_preserves_readiness_framing_and_eof(monkeypatch, mode):
    import sys
    module = frame_input()
    frame = module.FrameInput.__new__(module.FrameInput)
    frame.fd, frame.limit, frame.pending = 9, 64, bytearray()
    calls = []
    if mode == 'buffered':
        frame.pending.extend(b'{"first":1}\n{"second":2}\n')
    elif mode in ('partial', 'partial-eof'):
        frame.pending.extend(b'{"first":')
    values = {'partial': b'1}\n', 'eof': b'', 'partial-eof': b'',
              'oversized': b'x' * 64, 'invalid-json': b'\xff\n', 'nonobject': b'[]\n'}

    def select(readers, writers, errors, timeout):
        calls.append(('select', timeout))
        assert readers == [9] and writers == errors == []
        return ([] if mode == 'idle' else [9]), [], []

    def read(fd, count):
        calls.append(('read', count))
        assert fd == 9 and 0 < count <= 65
        if mode == 'read-error':
            raise OSError('Synthetic pipe error')
        if mode == 'would-block':
            raise BlockingIOError
        return values.get(mode, b'{"first":1}\n')

    monkeypatch.setattr(module, 'os', SimpleNamespace(name='posix', read=read))
    monkeypatch.setitem(sys.modules, 'select', SimpleNamespace(select=select))
    if mode in ('partial-eof', 'oversized', 'invalid-json', 'nonobject', 'read-error'):
        with pytest.raises((ValueError, OSError)):
            frame.poll(.025)
    else:
        assert frame.poll(.025) == ((False, None) if mode in ('idle', 'would-block') else
                                   (True, None) if mode == 'eof' else (True, {'first': 1}))
    if mode == 'buffered':
        assert frame.poll(.025) == (True, {'second': 2}) and calls == []
    else:
        assert calls[0] == ('select', .025), 'Input readiness must own the finite wait'
        assert (len(calls) == 1) == (mode == 'idle')


@pytest.mark.parametrize('mode', ['ready', 'arrival', 'idle', 'eof', 'error'])
def test_windows_frame_wait_stops_on_readiness_or_its_deadline(monkeypatch, mode):
    module = frame_input()
    frame = module.FrameInput.__new__(module.FrameInput)
    frame.fd, frame.limit, frame.pending = 9, 64, bytearray()
    frame.handle, frame.available = 10, SimpleNamespace(value=0)
    clock, sleeps = [0.0], []
    frame.ctypes = SimpleNamespace(byref=lambda value: value,
                                  get_last_error=lambda: 109 if mode == 'eof' else 5,
                                  WinError=lambda code: OSError('Synthetic pipe error'))

    def peek(handle, buffer, size, read, available, left):
        assert handle == 10 and available is frame.available
        available.value = 12 if mode == 'ready' or mode == 'arrival' and clock[0] >= .002 else 0
        return mode not in ('eof', 'error')

    def sleep(delay):
        assert 0 < delay <= .001
        sleeps.append(delay)
        clock[0] += delay

    frame.peek = peek
    monkeypatch.setattr(module, 'os', SimpleNamespace(name='nt', read=lambda *args: b'{"first":1}\n'))
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic=lambda: clock[0], sleep=sleep))
    if mode == 'error':
        with pytest.raises(OSError, match='Synthetic pipe'):
            frame.poll(.025)
    else:
        assert frame.poll(.025) == ((False, None) if mode == 'idle' else
                                   (True, None) if mode == 'eof' else (True, {'first': 1}))
    assert clock[0] == pytest.approx(.025 if mode == 'idle' else .002 if mode == 'arrival' else 0)
    assert len(sleeps) == (25 if mode == 'idle' else 2 if mode == 'arrival' else 0)


@pytest.mark.parametrize('timeout', [-1, .026, float('inf'), float('nan')])
def test_frame_input_rejects_invalid_wait_before_pipe_access(timeout):
    module = frame_input()
    frame = module.FrameInput.__new__(module.FrameInput)
    with pytest.raises(ValueError, match='Input wait'):
        frame.poll(timeout)
