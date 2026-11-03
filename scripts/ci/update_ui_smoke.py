"""Exercise the built updater UI headlessly without downloading or installing an update."""
from __future__ import annotations

import argparse
import json
import mimetypes
from pathlib import Path
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log

ORIGIN = 'http://127.0.0.1:18877'
BRIDGE = 'http://127.0.0.1:18878'
TOKEN = 'fixture-capability-not-a-production-secret-' + 'a' * 32


def exercise(dashboard: Path, output: Path, log) -> None:
    from playwright.sync_api import expect, sync_playwright

    if not (dashboard / 'index.html').is_file():
        raise ValueError('The actual built dashboard is required')
    output.mkdir(parents=True, exist_ok=True)
    status = {
        'state': 'available', 'message': 'A verified update is available.',
        'current': {'version': '0.36.0', 'sourceSha': '1' * 40, 'generation': 10,
                    'platform': 'linux-x64', 'portable': True},
        'offer': {'id': 'fixture-offer-11', 'version': '0.36.1', 'sourceSha': '2' * 40,
                  'generation': 11, 'downloadBytes': 1048576, 'expandedBytes': 4194304,
                  'components': {'sdk': {'playwright': '1.62.0'}, 'browsers': {}},
                  'restartRequired': True},
        'progress': {'receivedBytes': 0, 'totalBytes': 1048576}, 'canCancel': True,
    }
    operations = []
    errors = []
    api = {'/api/health': {'ok': True}, '/api/profiles': [], '/api/engines': {'playwright': True},
           '/api/config': {'root': '/fixture/data', 'version': '0.36.0', 'folders': [],
                           'default_engine': 'playwright'}, '/api/tokens': {'tokens': [], 'envToken': False}}

    def route(request_route):
        request = request_route.request
        url = urlsplit(request.url)
        origin = f'{url.scheme}://{url.netloc}'
        if origin == BRIDGE:
            cors = {'Access-Control-Allow-Origin': ORIGIN,
                    'Access-Control-Allow-Headers': 'authorization, content-type',
                    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS'}
            if request.method == 'OPTIONS':
                request_route.fulfill(status=204, headers=cors)
                return
            if request.header_value('authorization') != 'Bearer ' + TOKEN:
                raise AssertionError('Updater capability was missing')
            if url.path != '/status':
                operations.append((url.path, request.post_data_json))
            if url.path == '/download':
                assert request.post_data_json == {'offerId': 'fixture-offer-11', 'approve': True}
                status['state'] = 'ready'
                status['message'] = 'The verified update is ready.'
            elif url.path == '/install':
                assert request.post_data_json == {'offerId': 'fixture-offer-11', 'approve': True}
                status['state'] = 'deferred'
                status['message'] = 'Close active sessions before installing.'
            elif url.path == '/cancel':
                status['state'] = 'cancelled'
                status['message'] = 'Update cancelled. The installed generation is unchanged.'
                status['offer'] = None
            request_route.fulfill(json=status, headers=cors)
        elif origin == ORIGIN and url.path.startswith('/api/'):
            request_route.fulfill(json=api.get(url.path, {}))
        elif origin == ORIGIN:
            relative = unquote(url.path).lstrip('/') or 'index.html'
            path = (dashboard / relative).resolve()
            if not path.is_relative_to(dashboard) or not path.is_file():
                request_route.fulfill(status=404, body='Not found')
                return
            content_type = mimetypes.guess_type(path.name)[0] or 'application/octet-stream'
            if path.suffix == '.js':
                content_type = 'application/javascript'
            request_route.fulfill(body=path.read_bytes(), content_type=content_type)
        else:
            # No public services are needed by this isolated fixture.
            request_route.fulfill(body='', content_type='text/css')

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True, timeout=30000)
        try:
            context = browser.new_context(viewport={'width': 1280, 'height': 900})
            context.route('**/*', route)
            context.add_init_script('window.__NUMERA_UPDATES__ = ' + json.dumps({'endpoint': BRIDGE, 'token': TOKEN}))
            page = context.new_page()
            page.set_default_timeout(10000)
            page.set_default_navigation_timeout(15000)
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.goto(ORIGIN, wait_until='domcontentloaded')
            expect(page.get_by_role('button', name='Review update', exact=True)).to_be_visible()
            assert operations == [], 'Displaying an offer authorized a mutation'
            page.get_by_role('button', name='Review update', exact=True).click()
            panel = page.get_by_role('region', name='Software updates', exact=True)
            expect(panel).to_be_visible()
            page.screenshot(path=str(output / 'update-offer-dark.png'), full_page=True)
            panel.get_by_role('button', name='Download update', exact=True).click()
            dialog = page.get_by_role('dialog', name='Confirm update', exact=True)
            expect(dialog).to_be_visible()
            expect(dialog.get_by_role('button', name='Later', exact=True)).to_be_focused()
            page.keyboard.press('Shift+Tab')
            expect(dialog.get_by_role('button', name='Approve download', exact=True)).to_be_focused()
            page.keyboard.press('Escape')
            expect(dialog).not_to_be_visible()
            assert operations == [], 'Declining an offer contacted download/install'
            panel.get_by_role('button', name='Download update', exact=True).click()
            dialog.get_by_role('button', name='Approve download', exact=True).click()
            expect(panel.get_by_role('button', name='Install and restart', exact=True)).to_be_visible()
            assert [row[0] for row in operations] == ['/download']
            panel.get_by_role('button', name='Install and restart', exact=True).click()
            dialog.get_by_role('button', name='Approve restart', exact=True).click()
            expect(panel.get_by_text('Close active sessions before installing.', exact=True)).to_be_visible()
            assert [row[0] for row in operations] == ['/download', '/install']
            page.set_viewport_size({'width': 940, 'height': 760})
            page.get_by_role('button', name='Light', exact=True).click()
            panel.scroll_into_view_if_needed()
            assert panel.evaluate('(node) => node.scrollWidth <= node.clientWidth'), 'Update panel overflowed'
            page.screenshot(path=str(output / 'update-deferred-light.png'), full_page=True)
            panel.get_by_role('button', name='Cancel update', exact=True).click()
            expect(panel.get_by_text('Update cancelled. The installed generation is unchanged.', exact=True)).to_be_visible()
            assert [row[0] for row in operations] == ['/download', '/install', '/cancel']
            assert not errors, 'The rendered dashboard raised a JavaScript exception'
            context.close()
        finally:
            browser.close()
    (output / 'result.json').write_text(json.dumps({'rendered': True, 'mutations': [row[0] for row in operations],
        'viewports': [1280, 940], 'themes': ['dark', 'light'], 'page_errors': errors}, indent=2), encoding='utf-8')
    log.info('Consent, exact offer, keyboard focus, deferral, cancellation and two themes verified')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dashboard', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    with run_log('update-ui-smoke', ROOT / 'scripts/logs') as log:
        exercise(args.dashboard.resolve(), args.output.resolve(), log)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
