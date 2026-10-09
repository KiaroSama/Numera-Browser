"""Original CDP payload and page lifecycle policy without browser/network access."""
from numera import drivers
from numera.fingerprint import generate


class Session:
    def __init__(self, calls):
        self.calls = calls

    def send(self, method, payload=None):
        self.calls.append({'method':method,'payload':payload or {}})

    def on(self, event, callback):
        pass


class Context:
    def __init__(self):
        self.pages = [object()]
        self.calls = []
        self.handler = None

    def new_cdp_session(self, page):
        return Session(self.calls)

    def on(self, event, callback):
        if event != 'page':
            raise ValueError('Unexpected original lifecycle subscription')
        self.handler = callback


def cases():
    result = []
    for os in ('windows','macos','linux','android','ios'):
        for seed in (1,42):
            fp = generate(seed=seed,os=os,locale='en-US')
            context = Context()
            drivers._apply_client_hints(context,fp)
            initial = list(context.calls)
            if context.handler is None:
                raise ValueError('Original new-page policy absent')
            context.handler(object())
            result.append({'name':f'{os}-{seed}','input':fp.to_dict(),
                           'initial':initial,'later':context.calls[len(initial):]})
    return result
