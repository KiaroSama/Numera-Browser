"""Retained extension ID recognition and source diagnostic vocabulary."""
from numera import webstore


def cases() -> list[dict]:
    samples = ['', '  ', 'a' * 32, 'p' * 32, 'a' * 31, 'a' * 33,
               'q' * 32, 'A' * 32, 'a' * 20 + 'z' * 12, 'prefix_' + 'a' * 32,
               'https://chromewebstore.google.com/detail/name/' + 'a' * 32,
               'https://chrome.google.com/webstore/detail/name/' + 'p' * 31,
               'https://example.org/' + 'a' * 32 + '?x=1', 'https://example.org/no-id',
               'https://chromewebstore.google.com/detail/name/' + 'a' * 31 + '/?x=' + 'p' * 32,
               'α' + 'a' * 32, ' ' + 'a' * 32 + '\n', 'socks5://example.org',
               'CHROMEWEBSTORE.GOOGLE.COM/example', 'https://evil.example/chromewebstore.google.com']
    found = []
    for text in samples:
        try:
            result,error = webstore.extension_id(text),None
        except webstore.WebStoreError as exc:
            result,error = None,str(exc)
        found.append({'input':text,'result':result,'error':error,'is_store_url':webstore.is_store_url(text)})
    return found
