"""Verify native MCP cancellation at idle stdin and during a nested SDK call."""
from __future__ import annotations
import argparse
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera import proctree, sessions
from native_entry_acceptance import execute
from native_job_acceptance import wait_for
from run_logging import run_log


def main(candidate, log):
    if os.name != 'posix':
        raise RuntimeError('This signal fixture requires POSIX; Windows console acceptance is separate')
    with tempfile.TemporaryDirectory(prefix='native-signals-', dir=ROOT / '.ci-work/migration-temp') as temporary:
        parent = Path(temporary)
        root = parent / 'store'
        root.mkdir()
        home = parent / 'home'
        home.mkdir()
        env = os.environ.copy()
        for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR', 'NUMERA_NATIVE_SUPERVISE',
                    'NUMERA_NATIVE_JOB_GATE', 'NUMERA_HELPER_UNITS', 'NUMERA_HELPER_SCOPE', 'NUMERA_PARENT_PID'):
            env.pop(key, None)
        env.update(PYTHONUTF8='1', HOME=str(home), NUMERA_APP_DIR=str(parent),
                   NUMERA_LOG_DIR=str(parent / 'logs'), NUMERA_SDK_PYTHON=sys.executable)
        adapter = parent / 'sdk.py'
        adapter.write_text('''import json,pathlib,sys,threading\ndef available():\n return {name:{"installed":name=="playwright"} for name in ("playwright","patchright","camoufox","cloak")}\nif __name__=="__main__":\n for line in sys.stdin:\n  request=json.loads(line); op=request["operation"]\n  if op=="hello": result={"backends":available()}\n  elif op=="open":\n   result={"opened":True,"backend":request["backend"]}\n   pathlib.Path(__file__).with_name("sdk-open").write_text("ready",encoding="utf-8")\n  else: result={"closing":True}\n  print(json.dumps({"protocol":1,"id":request["id"],"ok":True,"result":result}),flush=True)\n  if op=="open": threading.Event().wait(15)\n  if op=="close": break\n''', encoding='utf-8')
        env['NUMERA_BROWSER_ADAPTER'] = str(adapter)
        code, stdout, stderr = execute([str(candidate), '--data-dir', str(root), 'create',
                                       'Signal fixture', '--engine', 'playwright', '--seed', '42', '--json'], env)
        assert code == 0 and not stderr, (code, stdout, stderr)
        reference = json.loads(stdout)['profile']['id']
        for active, inherited in ((False, False), (True, False), (True, True)):
            marker = parent / 'sdk-open'
            marker.unlink(missing_ok=True)
            outer = root / 'jobs' / 'outer.units'
            if inherited:
                outer.parent.mkdir(exist_ok=True)
                outer.write_text(json.dumps({'version': 2, 'event': 'header'}) + '\n'
                                 + json.dumps({'event': 'pending', 'attempt': 'ancestor'}) + '\n', encoding='utf-8')
                env['NUMERA_HELPER_UNITS'] = str(outer)
            process, tree = proctree.spawn([str(candidate), '--data-dir', str(root), 'mcp'], env=env,
                                           stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            nested = None
            try:
                if active:
                    request = {'id': 1, 'method': 'tools/call', 'params': {
                        'name': 'launch', 'arguments': {'ref': reference, 'headless': True}}}
                else:
                    request = {'id': 1, 'method': 'ping'}
                process.stdin.write((json.dumps(request) + '\n').encode('utf-8'))
                process.stdin.flush()
                if active:
                    wait_for((parent / 'sdk-open').is_file, 10, 'MCP tool never opened nested SDK fixture')
                    claim = sessions.read(root, reference)
                    assert isinstance(claim, dict), claim
                    nested = proctree.Tree(pid=claim['pid'], token=claim['token'],
                                           group=claim['tree']['group'], kind=claim['tree']['kind'])
                    assert proctree.alive_tree(nested), 'Nested SDK was not live before cancellation'
                    # A stuck SDK does not read EOF when its native caller is killed.
                    # Separate groups must still be accounted for by the MCP owner.
                else:
                    import selectors
                    ready = selectors.DefaultSelector()
                    try:
                        ready.register(process.stdout, selectors.EVENT_READ)
                        assert ready.select(timeout=5), 'MCP ping did not become readable'
                        assert json.loads(process.stdout.readline())['id'] == 1
                    finally:
                        ready.close()
                started = time.monotonic()
                os.kill(process.pid, signal.SIGTERM)
                # Keep stdin open: successful cancellation cannot be explained by EOF.
                process.wait(timeout=5)
                stdout, stderr = process.communicate(timeout=2)
                stdout = stdout.decode('utf-8')
                stderr = stderr.decode('utf-8')
                assert process.returncode == 130, (active, process.returncode, stdout, stderr)
                assert time.monotonic() - started < 5
                if nested:
                    assert not proctree.alive_tree(nested), 'Cancelled MCP left a live separately-owned SDK'
                    assert sessions.state(sessions.read(root, reference)) == 'free', 'Cancelled MCP left a busy claim'
                if inherited:
                    records = [json.loads(line) for line in outer.read_text(encoding='utf-8').splitlines()]
                    assert any(value.get('event') == 'unit' and value.get('pid') == nested.pid for value in records), 'Nested SDK was hidden from outer supervisor'
                    assert any(value.get('event') == 'pending' and value.get('attempt') == 'ancestor' for value in records)
                    assert not any(value.get('event') == 'cancelled' and value.get('attempt') == 'ancestor' for value in records), 'Inner call resolved an ancestor it does not own'
                log.info('MCP SIGTERM cancellation/130/owned cleanup passed active_tool=%s inherited_report=%s', active, inherited)
            finally:
                if nested and not proctree.end_tree(nested, grace=1, kill_grace=3):
                    raise RuntimeError('Signal fixture nested SDK cleanup failed')
                if not proctree.end_tree(tree, grace=1, kill_grace=3):
                    raise RuntimeError('Signal fixture MCP tree cleanup failed')
                process.wait(timeout=5)
                for pipe in (process.stdin, process.stdout, process.stderr):
                    pipe.close()
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-signal-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
