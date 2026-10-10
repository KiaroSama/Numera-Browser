"""Observe retained startup navigation ordering and per-URL refusal handling."""
from numera.launch_options import _open_startup_urls
from numera.models import Profile
from numera.fingerprint import generate


def cases():
    result = []
    for urls, failed in [([], []), (['about:blank'], []),
                         (['invalid-fixture-url', 'about:blank', 'data:text/html,fixture'], [0]),
                         (['about:blank', 'invalid-fixture-url', 'about:blank'], [1])]:
        seen = []
        class Page:
            def __init__(self, tab):
                self.tab = tab
            def goto(self, url, **kwargs):
                index = len(seen)
                seen.append({'tab': self.tab, 'url': url, **kwargs})
                if index in failed:
                    raise ValueError('Controlled navigation failure')
        class Context:
            def __init__(self):
                self.tabs = 0
            def new_page(self):
                self.tabs += 1
                return Page(self.tabs)
        context = Context()
        profile = Profile(id='a', name='fixture', created_at=1000.0, updated_at=1000.0,
                          fingerprint=generate(seed=1, os='windows'), startup_urls=urls)
        _open_startup_urls(context, Page(0), profile)
        result.append({'name': f'startup-{len(result)}', 'urls': urls,
                       'failed': failed, 'expected': seen, 'new_pages': context.tabs})
    return result
