"""Bounded SDK-only browser protocol. No application stores or launch decisions."""
from __future__ import annotations

import importlib.metadata
import importlib.util
import json
import logging
import os
import queue
import sys
import threading
import time
from pathlib import Path

PROTOCOL = 1
MAX_FRAME = 1_048_576
PACKAGES = {'playwright':'playwright','patchright':'patchright','camoufox':'camoufox','cloak':'cloakbrowser'}


def logger():
    log = logging.getLogger('numera.browser-adapter')
    log.setLevel(logging.INFO)
    log.propagate = False
    directory = Path(os.environ.get('NUMERA_ADAPTER_LOG_DIR') or Path(__file__).resolve().parent/'logs')
    try:
        directory.mkdir(parents=True, exist_ok=True)
        stamp = time.strftime('%Y-%m-%d_%H-%M-%S', time.gmtime())
        for suffix in range(1000):
            path = directory/f'browser-adapter_{stamp}_UTC{f".{suffix}" if suffix else ""}.log'
            try:
                handler = logging.FileHandler(path, mode='x', encoding='utf-8')
                break
            except FileExistsError:
                continue
        else:
            raise OSError('Log namespace exhausted')
        formatter = logging.Formatter('[%(asctime)s UTC] [%(levelname)s] [%(name)s] %(message)s', datefmt='%Y-%m-%d %H:%M:%S')
        formatter.converter = time.gmtime
        handler.setFormatter(formatter)
        log.addHandler(handler)
    except OSError:
        print('Browser adapter file logging unavailable; diagnostics remain on stderr', file=sys.stderr)
        log.addHandler(logging.StreamHandler(sys.stderr))
    return log


def read_frames(events, permit):
    try:
        while True:
            # No pipe read overlaps SDK calls: Windows DLL initialization may
            # inspect standard handles and wait for an outstanding read.
            permit.wait()
            permit.clear()
            line = sys.stdin.buffer.readline(MAX_FRAME + 1)
            if not line:
                events.put(None)
                return
            if len(line) > MAX_FRAME or not line.endswith(b'\n'):
                raise ValueError('Invalid frame size')
            value = json.loads(line.decode('utf-8', errors='strict'))
            if not isinstance(value, dict):
                raise ValueError('Frame must be an object')
            events.put(value)
    except Exception:
        events.put({'invalid': True})


def send(request_id, result=None, error=None):
    value = {'protocol': PROTOCOL, 'id': request_id, 'ok': error is None}
    value['error' if error is not None else 'result'] = error if error is not None else result
    data = json.dumps(value, ensure_ascii=True, allow_nan=False).encode('utf-8') + b'\n'
    if len(data) > MAX_FRAME:
        raise ValueError('Response exceeds frame limit')
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()


def available():
    found = {}
    for backend, package in PACKAGES.items():
        installed = importlib.util.find_spec(package) is not None
        try:
            version = importlib.metadata.version(package) if installed else None
        except importlib.metadata.PackageNotFoundError:
            version = None
        found[backend] = {'installed': installed, 'version': version}
    return found


def open_context(backend, options):
    # Only SDK calls live here. Options, profile path, identity scripts and
    # backend choice are supplied by the native owner after its claim exists.
    if backend in ('playwright','patchright'):
        if backend == 'playwright':
            from playwright.sync_api import sync_playwright
        else:
            from patchright.sync_api import sync_playwright
        owner = sync_playwright().start()
        try:
            return owner, owner.chromium.launch_persistent_context(**options), None
        except BaseException:
            owner.stop()
            raise
    if backend == 'camoufox':
        log = logging.getLogger('numera.browser-adapter')
        log.info('Camoufox SDK import starting')
        from camoufox.sync_api import Camoufox
        log.info('Camoufox SDK import complete')
        manager = Camoufox(persistent_context=True, **options)
        log.info('Camoufox persistent context entering')
        context = manager.__enter__()
        log.info('Camoufox persistent context entered')
        return None, context, manager
    import cloakbrowser
    return None, cloakbrowser.launch_persistent_context(**options), None


def main():
    log = logger()
    events = queue.Queue(maxsize=8)
    permit = threading.Event()
    reader = threading.Thread(target=read_frames, args=(events, permit), daemon=True, name='adapter-input')
    reader.start()
    permit.set()
    owner = context = manager = None
    initialized = False
    closed = threading.Event()
    input_deadline = time.monotonic() + 120
    exit_code = 0
    closing_request = None
    log.info('Starting protocol=%s runtime=%s pid=%s', PROTOCOL, sys.version.split()[0], os.getpid())
    try:
        while True:
            if context is not None:
                try:
                    request = events.get_nowait()
                except queue.Empty:
                    if closed.is_set():
                        break
                    # Pump the sync SDK event loop; stdin reader never invokes SDK.
                    context.pages[0].wait_for_timeout(25) if context.pages else context.new_page().wait_for_timeout(25)
                    continue
            else:
                request = events.get(timeout=max(0.001, input_deadline - time.monotonic()))
            if request is None:
                break
            request_id = request.get('id')
            if request.get('protocol') != PROTOCOL or type(request_id) is not int or request_id < 1:
                raise ValueError('Invalid protocol envelope')
            operation = request.get('operation')
            try:
                if operation == 'hello' and not initialized:
                    send(request_id, {'backends': available(), 'runtime': sys.version.split()[0]})
                    initialized = True
                    permit.set()
                elif operation == 'open' and initialized and context is None:
                    backend = request.get('backend')
                    options = request.get('options')
                    scripts = request.get('init_scripts', [])
                    if backend not in PACKAGES or not isinstance(options, dict) or not isinstance(scripts, list) or not all(isinstance(script, str) for script in scripts):
                        raise ValueError('Invalid open request')
                    if not isinstance(options.get('user_data_dir'), str) or not options['user_data_dir']:
                        raise ValueError('Persistent profile directory required')
                    owner, context, manager = open_context(backend, options)
                    log.info('Context scripts applying count=%s', len(scripts))
                    context.on('close', lambda *_: closed.set())
                    for script in scripts:
                        context.add_init_script(script=script)
                    page = context.pages[0] if context.pages else context.new_page()
                    send(request_id, {'opened': True, 'backend': backend})
                    log.info('Persistent context opened backend=%s', backend)
                    permit.set()
                elif operation == 'navigate' and initialized and context is not None:
                    url = request.get('url')
                    if not isinstance(url, str) or not url:
                        raise ValueError('Navigation URL required')
                    page = context.pages[0] if context.pages else context.new_page()
                    page.goto(url, wait_until='domcontentloaded', timeout=30000)
                    send(request_id, {'navigated': True})
                    permit.set()
                elif operation == 'inspect' and initialized and context is not None:
                    page = context.pages[0] if context.pages else context.new_page()
                    send(request_id, page.evaluate("() => ({userAgent:navigator.userAgent, platform:navigator.platform, locale:navigator.language, languages:Array.from(navigator.languages), cores:navigator.hardwareConcurrency, width:innerWidth, height:innerHeight})"))
                    permit.set()
                elif operation == 'close' and initialized:
                    closing_request = request_id
                    break
                else:
                    raise ValueError('Unsupported operation or state')
            except Exception as error:
                # SDK errors can embed proxy credentials/URLs. Type is safe;
                # never send or log raw exception messages or request payloads.
                log.error('Operation failed operation=%s type=%s', operation if operation in ('hello','open','close','inspect','navigate') else 'invalid', type(error).__name__)
                send(request_id, error={'kind':'sdk','type':type(error).__name__})
                exit_code = 1
                break
    except Exception as error:
        log.error('Protocol failed type=%s', type(error).__name__)
        exit_code = 1
    finally:
        for action in (lambda: manager.__exit__(None,None,None) if manager else context.close() if context else None,
                       lambda: owner.stop() if owner else None):
            try:
                action()
            except Exception as error:
                log.error('SDK cleanup failed type=%s', type(error).__name__)
                exit_code = 1
        if closing_request is not None:
            try:
                send(closing_request, {'closing': True} if exit_code == 0 else None,
                     error={'kind':'cleanup'} if exit_code else None)
            except (OSError, ValueError):
                exit_code = 1
        log.info('Finished exit=%s', exit_code)
        for handler in list(log.handlers):
            log.removeHandler(handler)
            handler.close()
    return exit_code

if __name__ == '__main__':
    raise SystemExit(main())
