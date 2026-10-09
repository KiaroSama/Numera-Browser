"""Execute native-supplied CDP policy; no application identity decisions."""
from __future__ import annotations
import logging


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
    log = logging.getLogger('numera.browser-adapter')

    def on_page(page):
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

    def later(page):
        try:
            on_page(page)
        except Exception as error:
            log.warning('CDP new page configuration failed type=%s', type(error).__name__)

    context.on('page', later)
    for page in context.pages:
        on_page(page)
    return retained
