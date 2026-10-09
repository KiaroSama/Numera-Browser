"""Execute native-supplied CDP policy; no application identity decisions."""
from __future__ import annotations
import logging
import time


def apply(context, policy):
    if not isinstance(policy, dict):
        raise ValueError('CDP policy must be an object')
    commands = policy.get('commands')
    prefixes = policy.get('strip_headers_prefixes')
    if not isinstance(commands, list) or not 1 <= len(commands) <= 8:
        raise ValueError('CDP commands missing or excessive')
    if not isinstance(prefixes, list) or not all(isinstance(prefix, str) and prefix for prefix in prefixes):
        raise ValueError('CDP header prefixes invalid')
    for command in commands:
        if not isinstance(command, dict) or command.get('method') not in ('Emulation.setUserAgentOverride','Fetch.enable') or not isinstance(command.get('payload'), dict):
            raise ValueError('CDP command invalid')
    retained = []
    configured = {}
    log = logging.getLogger('numera.browser-adapter')

    def on_page(page):
        configured[page] = 'pending'
        cdp = context.new_cdp_session(page)
        if prefixes:
            def paused(event):
                request_id = event.get('requestId')
                if not request_id:
                    return
                try:
                    headers = event.get('request', {}).get('headers') or {}
                    cdp.send('Fetch.continueRequest', {
                        'requestId':request_id,
                        'headers':[{'name':key,'value':value} for key,value in headers.items()
                                   if not any(key.lower().startswith(prefix) for prefix in prefixes)],
                    })
                except Exception as error:
                    log.warning('CDP request continuation failed type=%s', type(error).__name__)
                    try:
                        cdp.send('Fetch.continueRequest', {'requestId':request_id})
                    except Exception as fallback:
                        log.warning('CDP fallback continuation failed type=%s', type(fallback).__name__)
            cdp.on('Fetch.requestPaused', paused)
            retained.append(cdp)
        for command in commands:
            cdp.send(command['method'], command['payload'])
        configured[page] = 'ready'

    def later(page):
        try:
            on_page(page)
        except Exception as error:
            configured[page] = 'failed'
            log.warning('CDP new page configuration failed type=%s', type(error).__name__)

    def ready(page):
        deadline = time.monotonic() + 5
        while configured.get(page) != 'ready':
            if configured.get(page) == 'failed':
                raise RuntimeError('New page CDP configuration failed')
            if time.monotonic() >= deadline:
                raise TimeoutError('New page CDP configuration did not complete')
            # A sync page event callback can yield during CDP.send; pump only
            # this SDK thread until its finite configuration state completes.
            page.wait_for_timeout(10)

    context.on('page', later)
    for page in context.pages:
        on_page(page)
    return retained, ready
