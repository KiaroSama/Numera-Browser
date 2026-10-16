"""Shell-private update quiescence; public API credentials never authorize it."""
from __future__ import annotations

import os
import secrets

from fastapi import Request
from fastapi.responses import JSONResponse

from ..update_admission import Admission, UpdateBusy

PRIVATE_PATHS = frozenset(('/api/updates/prepare', '/api/updates/quiesce', '/api/updates/resume'))


async def verify_named_token(admission, tokens, supplied):
    """Keep the last-used write owned by its actual authentication worker."""
    from starlette.concurrency import run_in_threadpool

    def verify():
        with admission.work('HTTP authentication'):
            return tokens.verify(supplied) is not None

    return await run_in_threadpool(verify)


def configure_admission(app, root):
    """The executing handler owns its reservation, not the HTTP consumer.

    Cancellation can abandon a threadpool await while its synchronous handler
    still mutates data. Keeping the guard inside that worker retains admission
    until the handler itself returns, even after the response task disappears.
    """
    from functools import wraps
    import inspect
    from fastapi.routing import APIRoute
    from fastapi import HTTPException
    admission = Admission(root)

    class AdmittedRoute(APIRoute):
        def get_route_handler(self):
            endpoint = self.dependant.call
            if self.path.startswith('/api/') and self.path not in PRIVATE_PATHS | {'/api/ready'}:
                if inspect.iscoroutinefunction(endpoint):
                    @wraps(endpoint)
                    async def guarded(**values):
                        from starlette.concurrency import run_in_threadpool
                        work = admission.work('HTTP handler')
                        try:
                            await run_in_threadpool(work.__enter__)
                        except UpdateBusy:
                            raise HTTPException(409, 'An update has closed work admission') from None
                        try:
                            return await endpoint(**values)
                        finally:
                            import anyio
                            with anyio.CancelScope(shield=True):
                                await run_in_threadpool(work.__exit__, None, None, None)
                else:
                    @wraps(endpoint)
                    def guarded(**values):
                        try:
                            with admission.work('HTTP handler'):
                                return endpoint(**values)
                        except UpdateBusy:
                            raise HTTPException(409, 'An update has closed work admission') from None
                self.dependant.call = guarded
            return super().get_route_handler()

    app.router.route_class = AdmittedRoute


def register(app, store):
    admission = app.state.update_admission = Admission(store.root)
    token = (os.environ.get('NUMERA_UPDATE_TOKEN') or '').encode('utf-8')

    @app.middleware('http')
    async def admission_and_private_auth(request, call_next):
        path = request.url.path.rstrip('/')
        if path in PRIVATE_PATHS:
            sent = request.headers.get('authorization', '')
            supplied = sent[7:] if sent.lower().startswith('bearer ') else ''
            if not token or not secrets.compare_digest(
                    token, supplied.encode('utf-8', 'surrogateescape')):
                return JSONResponse({'detail': 'invalid or missing private update token'},
                                    status_code=401)
            if request.headers.get('origin'):
                return JSONResponse({'detail': 'private update calls must originate from the shell'},
                                    status_code=403)
            return await call_next(request)
        if not path.startswith('/api/') or path == '/api/ready' or request.method == 'OPTIONS':
            return await call_next(request)
        return await call_next(request)

    def local_busy():
        app.state.reap_children()
        return bool(app.state.running or app.state.helpers.live())

    async def payload(request):
        content = bytearray()
        async for chunk in request.stream():
            content.extend(chunk)
            if len(content) > 4096:
                raise ValueError('private update request exceeds its limit')
        import json
        return json.loads(content.decode('utf-8', errors='strict'))

    @app.post('/api/updates/prepare', include_in_schema=False)
    async def prepare(request: Request):
        try:
            body = await payload(request)
            if not isinstance(body, dict) or body:
                return JSONResponse({'detail': 'prepare requires an empty object'}, status_code=400)
            return {'root': str(admission.root), 'gate': '.updates/admission.json'}
        except (ValueError, TypeError):
            return JSONResponse({'detail': 'invalid update request'}, status_code=400)

    @app.post('/api/updates/quiesce', include_in_schema=False)
    async def quiesce(request: Request):
        try:
            body = await payload(request)
            if not isinstance(body, dict) or set(body) != {'ticket'}:
                return JSONResponse({'detail': 'quiesce requires the precommitted recovery ticket'}, status_code=400)
            # The shell durably owns this ticket/root before the gate can close.
            from starlette.concurrency import run_in_threadpool
            ticket = await run_in_threadpool(admission.quiesce, local_busy, body['ticket'])
            return {'ticket': ticket, 'root': str(admission.root),
                    'gate': '.updates/admission.json'}
        except (ValueError, TypeError):
            return JSONResponse({'detail': 'invalid update request'}, status_code=400)
        except UpdateBusy:
            return JSONResponse({'detail': 'Active or uncertain work defers this update',
                                 'kind': 'UpdateBusy'}, status_code=409)
        except OSError:
            return JSONResponse({'detail': 'Update state could not be accessed safely',
                                 'kind': 'UpdateBusy'}, status_code=409)

    @app.post('/api/updates/resume', include_in_schema=False)
    async def resume(request: Request):
        try:
            body = await payload(request)
            if not isinstance(body, dict) or set(body) != {'ticket'}:
                return JSONResponse({'detail': 'resume requires only the recovery ticket'}, status_code=400)
            from starlette.concurrency import run_in_threadpool
            await run_in_threadpool(admission.resume, body['ticket'])
            return {'ok': True}
        except (ValueError, TypeError):
            return JSONResponse({'detail': 'invalid update request'}, status_code=400)
        except UpdateBusy:
            return JSONResponse({'detail': 'Active or uncertain work defers this update',
                                 'kind': 'UpdateBusy'}, status_code=409)
        except OSError:
            return JSONResponse({'detail': 'Update state could not be accessed safely',
                                 'kind': 'UpdateBusy'}, status_code=409)
