"""Exercise actual detached native entry, nested receipts and fail-closed pruning."""
from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera import proctree, unit_report
from native_entry_acceptance import execute
from run_logging import run_log


def wait_for(observe, seconds, description):
    end = time.monotonic() + seconds
    while True:
        result = observe()
        if result:
            return result
        if time.monotonic() >= end:
            raise TimeoutError(description)
        time.sleep(min(0.02, max(0, end - time.monotonic())))


def sdk_units(root, job, reference):
    report = unit_report.read(root / 'jobs' / f'{job["id"]}.units')
    assert report.complete, 'Detached ownership receipt is incomplete or malformed'
    command = proctree.Tree.from_record(job)
    assert command in report.trees, 'Detached command ownership identity was not recorded'
    claim = json.loads((root / 'sessions' / f'{reference}.json').read_text(encoding='utf-8'))
    sdk = proctree.Tree.from_record({**claim, 'pidToken': claim['token']})
    assert sdk.pid != command.pid and sdk in report.trees, 'Session SDK ownership identity was not recorded separately'
    assert proctree.alive_tree(sdk), 'Recorded session SDK tree was not actually live'
    return report.trees


def main(candidate, log):
    temporary = tempfile.TemporaryDirectory(prefix='native-jobs-', dir=ROOT / '.ci-work/migration-temp')
    parent = Path(temporary.name)
    root = parent / 'store'
    root.mkdir()
    env = os.environ.copy()
    for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR', 'NUMERA_NATIVE_SUPERVISE',
                'NUMERA_NATIVE_JOB_GATE', 'NUMERA_HELPER_UNITS', 'NUMERA_HELPER_SCOPE', 'NUMERA_PARENT_PID'):
        env.pop(key, None)
    home = parent / 'home'
    home.mkdir()
    env.update(PYTHONUTF8='1', HOME=str(home), USERPROFILE=str(home), NUMERA_APP_DIR=str(parent),
               NUMERA_LOG_DIR=str(parent / 'logs'), NUMERA_BROWSER_ADAPTER=str(parent / 'absent.py'),
               NUMERA_SDK_PYTHON=sys.executable)
    supervisors = []

    def cli(*argv):
        code, stdout, stderr = execute([str(candidate), '--data-dir', str(root), *argv, '--json'], env)
        if not stdout.strip():
            raise AssertionError(f'Native command produced no JSON: argv={argv!r} exit={code} stderr={stderr!r}')
        return code, json.loads(stdout), stderr

    def adopt(record):
        if record.get('supervisorPid'):
            supervisors.append(proctree.Tree(pid=record['supervisorPid'], token=record['supervisorToken'],
                                            group=record['supervisorPid'], kind='pgid'))

    try:
        if sys.platform == 'linux':
            import selectors
            import subprocess
            process, tree = proctree.spawn([str(candidate), '--data-dir', str(root), 'mcp'], env=env,
                                           stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            reader = selectors.DefaultSelector()
            idle_job = None
            try:
                reader.register(process.stdout, selectors.EVENT_READ)

                def call(message):
                    process.stdin.write((json.dumps(message) + '\n').encode('utf-8'))
                    process.stdin.flush()
                    assert reader.select(timeout=5), 'Idle MCP response did not become readable'
                    response = json.loads(process.stdout.readline())
                    assert response['id'] == message['id'], response
                    return response['result']

                result = call({'id': 1, 'method': 'tools/call', 'params': {
                    'name': 'list', 'arguments': {'detach': True}}})
                idle_job = result['structuredContent']
                adopt(idle_job)
                assert result['_meta']['exitCode'] == 0, result
                outcome = root / 'jobs' / f'{idle_job["id"]}.exit'
                wait_for(outcome.is_file, 10, 'Idle MCP detached list did not finish')
                assert json.loads(outcome.read_text(encoding='utf-8'))['exit'] == 0
                # A zombie has no live tree but still exists in /proc until its
                # actual parent waits. Keep MCP stdin open and send no input.
                supervisor = Path('/proc') / str(idle_job['supervisorPid'])
                wait_for(lambda: not supervisor.exists(), 5, 'Idle MCP did not reap its exited supervisor')
                assert process.poll() is None, 'MCP ended instead of reaping while idle'
                assert call({'id': 2, 'method': 'ping'}) == {}
                rejected = call({'id': 3, 'method': 'tools/call', 'params': {
                    'name': 'show', 'arguments': {'detach': True}}})
                assert rejected['isError'] and 'structuredContent' not in rejected and '_meta' not in rejected
                assert json.loads(rejected['content'][0]['text'])['exit'] == 2
                process.stdin.close()
                process.stdin = None
                stdout, stderr = process.communicate(timeout=5)
                assert (process.returncode, stdout, stderr) == (0, b'', b'')
                code, cleared, stderr = cli('jobs', 'clear')
                assert (code, cleared, stderr) == (0, {'cleared': 1}, ''), (code, cleared, stderr)
                log.info('Open idle MCP reaped actual supervisor and preserved parser-failure verdict')
            finally:
                reader.close()
                try:
                    if not proctree.end_tree(tree, grace=1, kill_grace=3):
                        raise RuntimeError('Idle MCP acceptance tree did not settle')
                    process.wait(timeout=5)
                finally:
                    for pipe in (process.stdin, process.stdout, process.stderr):
                        if pipe is not None:
                            pipe.close()
        # A detached native tool already has a durable supervisor owner. It must
        # not publish a transient capture child into the MCP caller's ancestor.
        ancestor = root / 'jobs' / 'mcp-ancestor.units'
        ancestor.parent.mkdir(exist_ok=True)
        header = b'{"version":2,"event":"header"}\n'
        ancestor.write_bytes(header)
        message = b'{"id":1,"method":"tools/call","params":{"name":"list","arguments":{"detach":true}}}\n'
        code, stdout, stderr = execute([str(candidate), '--data-dir', str(root), 'mcp'],
                                      {**env, 'NUMERA_HELPER_UNITS': str(ancestor)}, message)
        assert code == 0 and not stderr, (code, stderr)
        response = json.loads(stdout)['result']
        job = response['structuredContent']
        adopt(job)
        assert ancestor.read_bytes() == header, 'Detached MCP published an unnecessary captured command tree'
        if os.name == 'nt' and response['_meta']['exitCode'] != 0:
            assert 'breakaway' in job.get('error', '').lower(), response
            assert not list((root / 'jobs').glob('*.json')), 'Denied MCP spawn left a runnable record'
            log.info('Detached MCP shares durable owner; foreign restrictive job still refuses breakaway')
        else:
            assert response['_meta']['exitCode'] == 0 and job['command'] == 'list', response
            outcome = root / 'jobs' / f'{job["id"]}.exit'
            wait_for(outcome.is_file, 10, 'Detached MCP list did not finish')
            assert json.loads(outcome.read_text(encoding='utf-8'))['exit'] == 0
            wait_for(lambda: all(not proctree.alive_tree(tree) for tree in supervisors),
                     5, 'Detached MCP supervisor survived its command')
            code, cleared, stderr = cli('jobs', 'clear')
            assert (code, cleared, stderr) == (0, {'cleared': 1}, ''), (code, cleared, stderr)
            log.info('Detached MCP durable outcome and supervisor settlement passed')
        ancestor.unlink()
        code, job, stderr = cli('list', '--detach')
        if os.name == 'nt' and code != 0:
            assert 'breakaway' in job.get('error', '').lower(), (code, job, stderr)
            assert list((root / 'jobs').glob('*.json')) == [], 'Denied spawn left a runnable record'
            log.info('Windows outer-job breakaway refused before child startup; detach acceptance remains blocked')
            return 0
        assert code == 0 and not stderr, (code, job, stderr)
        adopt(job)
        jid = job['id']

        def completed():
            code, value, stderr = cli('jobs', 'show', jid)
            assert code == 0 and not stderr, (code, value, stderr)
            return value if not value['running'] and value['state'] in ('done', 'failed', 'stopped') else None

        finished = wait_for(completed, 10, 'Detached list never completed')
        assert finished['state'] == 'done' and finished['exit'] == 0, finished
        code, foreground, stderr = cli('list')
        assert code == 0 and not stderr, (code, foreground, stderr)
        assert json.loads('\n'.join(finished['tail'])) == foreground, finished
        wait_for(lambda: all(not proctree.alive_tree(tree) for tree in supervisors), 5, 'Supervisor survived completed command')
        code, value, stderr = cli('jobs', 'clear')
        assert (code, value, stderr) == (0, {'cleared': 1}, ''), (code, value, stderr)
        assert not list((root / 'jobs').glob(f'{jid}.*')), 'Completed job artifacts survived prune'
        log.info('Detached list start/poll/outcome/log/clear passed')

        code, job, stderr = cli('mcp', '--detach')
        assert code == 0 and not stderr, (code, job, stderr)
        adopt(job)
        jid = job['id']
        finished = wait_for(completed, 10, 'Detached MCP did not finish on EOF')
        assert finished['state'] == 'done' and finished['exit'] == 0, finished
        log.info('Detached MCP admission and stdin EOF passed')

        code, job, stderr = cli('serve', '--host', '127.0.0.1', '--port', '0', '--detach')
        assert code == 0 and not stderr, (code, job, stderr)
        adopt(job)
        jid = job['id']
        # JSON mode deliberately suppresses human stdout; observe the server's own readiness log.
        wait_for(lambda: any('Readiness published' in path.read_text(encoding='utf-8')
                             for path in (parent / 'logs').glob('native-http_*.log')),
                 10, 'Detached server never published readiness')
        code, stopped, stderr = cli('jobs', 'stop', jid)
        assert code == 0 and not stderr and not stopped['running'] and stopped['state'] == 'stopped', (code, stopped, stderr)
        wait_for(lambda: all(not proctree.alive_tree(tree) for tree in supervisors), 5, 'Stopped supervisor survived')
        log.info('Detached loopback server readiness/stop and supervisor settlement passed')

        # An interrupted transfer must stay visible even after the leader/outcome says done.
        units = root / 'jobs' / f'{jid}.units'
        with units.open('a', encoding='utf-8') as output:
            output.write(json.dumps({'event': 'pending', 'attempt': 'acceptance-interrupted'}) + '\n')
        code, damaged, stderr = cli('jobs', 'show', jid)
        assert code == 0 and damaged['running'] and damaged['orphaned'], (code, damaged, stderr)
        code, cleared, stderr = cli('jobs', 'clear')
        assert code == 0 and cleared == {'cleared': 1} and units.exists(), (code, cleared, stderr)
        code, refused, stderr = cli('jobs', 'stop', jid)
        assert code == 1 and 'incomplete' in refused['error'], (code, refused, stderr)
        log.info('Interrupted nested ownership receipt remains visible; stop/prune refused completion')
        # No process was created for this deliberate fixture transfer; resolve after observed tree emptiness.
        assert all(not proctree.alive_tree(tree) for tree in supervisors)
        with units.open('a', encoding='utf-8') as output:
            output.write(json.dumps({'event': 'cancelled', 'attempt': 'acceptance-interrupted'}) + '\n')
        code, cleared, stderr = cli('jobs', 'clear')
        assert (code, cleared, stderr) == (0, {'cleared': 1}, ''), (code, cleared, stderr)

        # This protocol stand-in proves nested native ownership, not a real browser SDK.
        adapter = parent / 'sdk.py'
        marker = parent / 'sdk-open'
        adapter.write_text('''import json,pathlib,sys\ndef available():\n return {name:{"installed":name=="playwright"} for name in ("playwright","patchright","camoufox","cloak")}\nif __name__=="__main__":\n for line in sys.stdin:\n  request=json.loads(line); op=request["operation"]\n  if op=="hello": result={"backends":available()}\n  elif op=="open":\n   result={"opened":True,"backend":request["backend"]}\n   pathlib.Path(__file__).with_name("sdk-open").write_text("ready",encoding="utf-8")\n  else: result={"closing":True}\n  print(json.dumps({"protocol":1,"id":request["id"],"ok":True,"result":result}),flush=True)\n  if op=="close": break\n''', encoding='utf-8')
        env['NUMERA_BROWSER_ADAPTER'] = str(adapter)
        code, profile, stderr = cli('create', 'Ownership fixture', '--engine', 'playwright', '--seed', '42')
        assert code == 0 and not stderr, (code, profile, stderr)
        reference = profile['profile']['id']
        for crash in (False, True):
            marker.unlink(missing_ok=True)
            code, job, stderr = cli('launch', reference, '--headless', '--detach')
            assert code == 0 and not stderr, (code, job, stderr)
            adopt(job)
            jid = job['id']
            wait_for(marker.is_file, 10, 'Detached SDK stand-in did not open')
            units = sdk_units(root, job, reference)
            if crash:
                import signal
                assert proctree.alive(job['pid'], job['pidToken']), 'Command identity changed before crash fixture'
                os.kill(job['pid'], signal.SIGKILL)
                finished = wait_for(completed, 10, 'Crashed leader left unobserved SDK ownership')
                assert finished['state'] == 'failed' and finished['exit'] != 0, finished
            else:
                code, stopped, stderr = cli('jobs', 'stop', jid)
                assert code == 0 and not stderr and not stopped['running'] and stopped['state'] == 'stopped', (code, stopped, stderr)
            wait_for(lambda: all(not proctree.alive_tree(tree) for tree in [*units, *supervisors]),
                     5, 'SDK or supervisor survived leader termination')
            code, cleared, stderr = cli('jobs', 'clear')
            assert (code, cleared, stderr) == (0, {'cleared': 1}, ''), (code, cleared, stderr)
            log.info('Detached nested SDK receipts/empty proof passed leader_crash=%s', crash)
        return 0
    finally:
        failures = []
        for tree in supervisors:
            if not proctree.end_tree(tree, grace=1, kill_grace=3):
                failures.append(tree.pid)
        for receipt in (root / 'jobs').glob('*.units'):
            for line in receipt.read_text(encoding='utf-8').splitlines():
                value = json.loads(line)
                if value.get('event') == 'unit' and not proctree.end_tree(proctree.Tree.from_record(value), grace=1, kill_grace=3):
                    failures.append(value['pid'])
        for record in (root / 'jobs').glob('*.json'):
            value = json.loads(record.read_text(encoding='utf-8'))
            if value.get('pid') and not proctree.end_tree(proctree.Tree.from_record(value), grace=1, kill_grace=3):
                failures.append(value['pid'])
        if failures:
            temporary._finalizer.detach()
            raise RuntimeError(f'Native job acceptance retained owned PIDs {failures}')
        temporary.cleanup()
        log.info('Every captured detached job and supervisor tree settled')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-job-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
