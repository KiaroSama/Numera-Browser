"""HTTP process startup, secure binding and shell-owned shutdown."""
from __future__ import annotations

import os
from typing import Optional

# An empty host binds every interface and is deliberately not a loopback name.
LOOPBACK = {'127.0.0.1', 'localhost', '::1', '0:0:0:0:0:0:0:1'}


def _refuse_insecure_bind(host: str, root) -> Optional[str]:
    from .stores import TokenStore

    if str(host).strip().lower() in LOOPBACK:
        return None
    if (os.environ.get('NUMERA_API_TOKEN') or '').strip():
        return None
    try:
        if TokenStore(root).list():
            return None
    except Exception:  # An unreadable credential store never authorizes a bind.
        pass
    if (os.environ.get('NUMERA_ALLOW_INSECURE_BIND') or '').strip():
        return None
    return (
        f'Refusing to serve on {host} without authentication.\n'
        '  This API can create profiles, launch browsers and export cookies, and\n'
        '  on a non-loopback address anything that can reach the port can use it.\n'
        '  Fix it with either:\n'
        '    numera token create "<what it is for>"   (recommended)\n'
        '    set NUMERA_API_TOKEN=<secret>\n'
        '  Then set NUMERA_ALLOWED_ORIGINS if a browser page will call it.\n'
        '  To bind anyway - only when something else in front already\n'
        '  authenticates - set NUMERA_ALLOW_INSECURE_BIND=1.'
    )


def serve(host: str = '127.0.0.1', port: int = 8787, data_dir: Optional[str] = None,
          verbose: bool = False, password: Optional[str] = None):
    from . import _require_fastapi, create_app, dashboard_dir
    _require_fastapi()
    import uvicorn
    from .. import firstrun, logs
    from ..paths import resolve_data_dir, watch_parent

    root = resolve_data_dir(data_dir)
    host = (host or '').strip() or '127.0.0.1'
    refusal = _refuse_insecure_bind(host, root)
    if refusal:
        raise SystemExit(refusal)
    from ..update_startup import startup
    with startup(root, first_import=True) as recovery:
        log_path = logs.setup(root, verbose=verbose)
        log = logs.get('serve')
        # A recovery engine only serves ready/private resume until the worker's
        # identity-bound commit. It must never import user data in that interval.
        imported = None if recovery else firstrun.run_if_needed(root)
        if imported:
            log.info('first run: imported from %s - copied %s, skipped %s, %d profile(s)',
                     imported['source'], imported['copied'] or 'nothing',
                     imported['skipped'] or 'nothing', imported['profiles'])
            for problem in imported['failed']:
                log.warning('first-run import problem: %s', problem)
            if imported['copied']:
                print(f"  Imported {imported['profiles']} profile(s) from {imported['source']}")
        app = create_app(str(root), password=password)

    # macOS has no Linux PDEATHSIG/Windows Job guarantee. The watcher must run
    # app cleanup before os._exit, which never executes the lifespan handlers.
    def shell_is_gone():
        try:
            app.state.run_shutdown()
        finally:
            os._exit(0)

    watch_parent(on_gone=shell_is_gone)
    where = 'the dashboard and API' if dashboard_dir() else 'the API'
    print(f'Numera Browser: {where} on http://{host}:{port}  (data: {root})')
    if log_path:
        print(f'  log: {log_path}')
    log.info('listening on %s:%s', host, port)
    uvicorn.run(app, host=host, port=port, log_level='debug' if verbose else 'info')
