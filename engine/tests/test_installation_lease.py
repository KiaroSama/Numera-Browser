"""Portable processes sharing an installation exclude replacement across data roots."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import pytest

pytestmark = pytest.mark.timeout(10)


def test_source_checkout_does_not_claim_an_installation(monkeypatch):
    from numera_installation import runtime_lease
    monkeypatch.delattr(sys, 'frozen', raising=False)
    with runtime_lease():
        pass


def test_entry_checks_activation_before_importing_cli(tmp_path):
    installation = tmp_path / 'portable'
    engine = installation / 'engine'
    state = installation / '.updates'
    engine.mkdir(parents=True)
    state.mkdir()
    (state / 'activation.json').write_text('{"schema":1,"nonce_sha256":"mismatch"}', encoding='utf-8')
    entry = Path(__file__).resolve().parents[2] / 'scripts/engine_entry.py'
    source_root = Path(__file__).resolve().parents[1]
    code = '''import runpy,sys\nsys.path.insert(0, sys.argv[1])\nsys.frozen=True\nsys.executable=sys.argv[2]\nclass RefuseCli:\n def find_spec(self, fullname, *args):\n  if fullname.startswith("numera.") or fullname == "numera":\n   raise AssertionError("package imported before installation activation check")\nsys.meta_path.insert(0,RefuseCli())\nrunpy.run_path(sys.argv[3],run_name="__main__")\n'''
    result = subprocess.run([sys.executable, '-I', '-c', code, str(source_root),
                             str(engine / 'numera-engine.exe'), str(entry)],
                            capture_output=True, text=True, encoding='utf-8', timeout=5,
                            env={k: v for k, v in os.environ.items()
                                 if k != 'NUMERA_UPDATE_ACTIVATION'})
    assert result.returncode == 1
    assert 'recovery owns this installation' in result.stderr
    assert 'imported before' not in result.stderr


def test_shared_lease_refuses_exclusive_writer_and_activation_requires_nonce(tmp_path, monkeypatch):
    from numera_installation import runtime_lease, InstallationBusy
    from numera.data_locks import _DataLock

    installation = tmp_path / 'portable with spaces'
    (installation / 'engine').mkdir(parents=True)
    monkeypatch.setattr(sys, 'frozen', True, raising=False)
    monkeypatch.setattr(sys, 'executable', str(installation / 'engine/numera-engine.exe'))
    monkeypatch.setenv('NUMERA_LOCK_TIMEOUT', '0')
    lock = installation / '.updates/runtime.lock'
    with runtime_lease():
        with pytest.raises(RuntimeError):
            with _DataLock(lock):
                pytest.fail('exclusive activation passed a live portable process')
    marker = installation / '.updates/activation.json'
    nonce = 'fixture-activation-nonce'
    marker.write_text(json.dumps({'schema': 1, 'nonce_sha256':
        hashlib.sha256(nonce.encode('utf-8')).hexdigest()}), encoding='utf-8')
    monkeypatch.delenv('NUMERA_UPDATE_ACTIVATION', raising=False)
    with pytest.raises(InstallationBusy):
        with runtime_lease():
            pytest.fail('ordinary startup bypassed activation')
    monkeypatch.setenv('NUMERA_UPDATE_ACTIVATION', nonce)
    with runtime_lease():
        pass
    marker.write_text('{', encoding='utf-8')
    with pytest.raises(InstallationBusy):
        with runtime_lease():
            pytest.fail('corrupt activation evidence was treated as permission')
