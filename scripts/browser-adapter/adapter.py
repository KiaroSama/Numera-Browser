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
OUTPUT = None
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
    OUTPUT.write(data)
    OUTPUT.flush()


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
        if 'exclude_addons' in options:
            from camoufox.addons import DefaultAddons
            options = dict(options)
            options['exclude_addons'] = [DefaultAddons[name] for name in options['exclude_addons']]
        geoip = os.environ.get('NUMERA_ADAPTER_GEOIP_DIR')
        if geoip:
            from camoufox import geolocation
            geolocation.GEOIP_DIR = Path(geoip)
            geolocation.MMDB_DIR = Path(geoip) / 'mmdb'
            geolocation.GEOIP_CONFIG = Path(geoip) / 'config.yml'
        log.info('Camoufox SDK import complete')
        manager = Camoufox(**options)
        log.info('Camoufox persistent context entering')
        context = manager.__enter__()
        log.info('Camoufox persistent context entered')
        return None, context, manager
    import cloakbrowser
    return None, cloakbrowser.launch_persistent_context(**options), None


def main():
    global OUTPUT
    # SDKs can print downloads/configuration, including from native libraries.
    # Only protocol frames may reach the native owner's response pipe.
    descriptor = os.dup(sys.stdout.fileno())
    try:
        os.set_inheritable(descriptor, False)
        OUTPUT = os.fdopen(descriptor, 'wb')
    except BaseException:
        os.close(descriptor)
        raise
    with open(os.devnull, 'wb') as sink:
        os.dup2(sink.fileno(), sys.stdout.fileno(), inheritable=False)
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
    cdp_sessions = []
    cdp_ready = None
    log.info('Starting protocol=%s runtime=%s pid=%s', PROTOCOL, sys.version.split()[0], os.getpid())
    try:
        while True:
            if context is not None:
                if closed.is_set():
                    break
                try:
                    request = events.get_nowait()
                except queue.Empty:
                    if closed.is_set():
                        break
                    # Pump SDK events while retaining responsive native close input.
                    try:
                        context.pages[0].wait_for_timeout(25) if context.pages else context.new_page().wait_for_timeout(25)
                    except Exception:
                        if not closed.is_set():
                            raise
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
                    if request.get('cdp_policy') is not None:
                        if backend not in ('playwright','patchright'):
                            raise ValueError('Selected SDK does not accept CDP policy')
                        from hints import apply
                        sessions, cdp_ready = apply(context, request['cdp_policy'])
                        cdp_sessions.extend(sessions)
                    page = context.pages[0] if context.pages else context.new_page()
                    if os.environ.get('NUMERA_ADAPTER_FIRST_PAGE') == '1':
                        page.on('close', lambda *_: closed.set())
                    if cdp_ready:
                        cdp_ready(page)
                    send(request_id, {'opened': True, 'backend': backend})
                    log.info('Persistent context opened backend=%s', backend)
                    permit.set()
                elif operation == 'navigate' and initialized and context is not None:
                    url = request.get('url')
                    if not isinstance(url, str) or not url:
                        raise ValueError('Navigation URL required')
                    new_page = request.get('new_page', False)
                    if type(new_page) is not bool:
                        raise ValueError('New page option must be a boolean')
                    tolerate_failure = request.get('tolerate_failure', False)
                    if type(tolerate_failure) is not bool:
                        raise ValueError('Navigation failure option must be a boolean')
                    timeout_ms = request.get('timeout_ms', 30000)
                    if type(timeout_ms) is not int or not 1 <= timeout_ms <= 45000:
                        raise ValueError('Navigation deadline is invalid')
                    navigated = True
                    try:
                        if new_page:
                            page = context.new_page()
                            if cdp_ready:
                                cdp_ready(page)
                        page.goto(url, wait_until='domcontentloaded', timeout=timeout_ms)
                    except Exception:
                        if not tolerate_failure:
                            raise
                        navigated = False
                    send(request_id, {'navigated': navigated})
                    permit.set()
                elif operation == 'mouse' and initialized and context is not None:
                    import math
                    action, x, y = request.get('action'), request.get('x'), request.get('y')
                    if (action not in ('wheel', 'move') or type(x) not in (int, float)
                            or type(y) not in (int, float) or not math.isfinite(x)
                            or not math.isfinite(y)):
                        raise ValueError('Invalid mouse action')
                    steps = request.get('steps', 1)
                    if type(steps) is not int or not 1 <= steps <= 1000:
                        raise ValueError('Invalid mouse steps')
                    performed = True
                    try:
                        if action == 'wheel':
                            page.mouse.wheel(x, y)
                        else:
                            page.mouse.move(x, y, steps=steps)
                    except Exception as error:
                        log.warning('Mouse failed type=%s', type(error).__name__)
                        performed = False
                    send(request_id, {'performed': performed})
                    permit.set()
                elif operation == 'route' and initialized and context is not None:
                    pattern, body = request.get('pattern'), request.get('body')
                    content_type, status = request.get('content_type'), request.get('status')
                    if (not isinstance(pattern, str) or not pattern or not isinstance(body, str)
                            or not isinstance(content_type, str) or not content_type
                            or type(status) is not int or not 100 <= status <= 599):
                        raise ValueError('Invalid route response')
                    page.route(pattern, lambda route, body=body, content_type=content_type, status=status:
                               route.fulfill(status=status, content_type=content_type, body=body))
                    send(request_id, {'routed': True})
                    permit.set()
                elif operation == 'evaluate' and initialized and context is not None:
                    expression = request.get('expression')
                    tolerate_failure = request.get('tolerate_failure', False)
                    if not isinstance(expression, str) or not expression or type(tolerate_failure) is not bool:
                        raise ValueError('Invalid evaluation request')
                    evaluated = True
                    try:
                        value = page.evaluate(expression, request.get('argument'))
                    except Exception as error:
                        if not tolerate_failure:
                            raise
                        log.warning('Evaluation failed type=%s', type(error).__name__)
                        evaluated, value = False, None
                    send(request_id, {'evaluated': evaluated, 'value': value})
                    permit.set()
                elif operation == 'inspect' and initialized and context is not None:
                    observation = """async () => ({
                        userAgent:navigator.userAgent, platform:navigator.platform,
                        locale:navigator.language, languages:Array.from(navigator.languages),
                        cores:navigator.hardwareConcurrency, width:innerWidth, height:innerHeight,
                        userAgentData:navigator.userAgentData
                            ? await navigator.userAgentData.getHighEntropyValues([
                                'platformVersion','architecture','bitness','model',
                                'uaFullVersion','fullVersionList','wow64']) : null
                    })"""
                    view = page.evaluate(observation, isolated_context=False) if backend == 'patchright' else page.evaluate(observation)
                    send(request_id, view)
                    permit.set()
                elif operation == 'cookies-read' and initialized and context is not None:
                    send(request_id, {'cookies': context.cookies()})
                    permit.set()
                elif operation == 'cookies-clear' and initialized and context is not None:
                    context.clear_cookies()
                    send(request_id, {'cleared': True})
                    permit.set()
                elif operation == 'cookies-add' and initialized and context is not None:
                    cookies = request.get('cookies')
                    if not isinstance(cookies, list) or not all(isinstance(cookie, dict) for cookie in cookies):
                        raise ValueError('Cookie rows must be objects')
                    context.add_cookies(cookies)
                    send(request_id, {'added': True})
                    permit.set()
                elif operation == 'close' and initialized:
                    closing_request = request_id
                    break
                else:
                    raise ValueError('Unsupported operation or state')
            except Exception as error:
                # SDK errors can embed proxy credentials/URLs. Type is safe;
                # never send or log raw exception messages or request payloads.
                log.error('Operation failed operation=%s type=%s', operation if operation in ('hello','open','close','inspect','navigate','mouse','route','evaluate','cookies-read','cookies-clear','cookies-add') else 'invalid', type(error).__name__)
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
        OUTPUT.close()
    return exit_code

if __name__ == '__main__':
    raise SystemExit(main())
