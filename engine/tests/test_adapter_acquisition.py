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
