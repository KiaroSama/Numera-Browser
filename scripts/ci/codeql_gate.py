"""Require the independent CodeQL verdict for the exact analyzed source revision."""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'migration'))
from run_logging import run_log


def verify(pages, sha):
    if not re.fullmatch('[0-9a-f]{40}', sha):
        raise ValueError('CodeQL source SHA is invalid')
    if not isinstance(pages, list) or not pages:
        raise ValueError('CodeQL check response is missing')
    selected = []
    for page in pages:
        if not isinstance(page, dict) or not isinstance(page.get('check_runs'), list):
            raise ValueError('CodeQL check response is malformed')
        selected.extend(check for check in page['check_runs']
                        if check.get('name') == 'CodeQL'
                        and check.get('app', {}).get('slug') == 'github-advanced-security')
    if not selected:
        raise ValueError('Independent CodeQL verdict is unavailable; signing is blocked')
    latest = max(selected, key=lambda check: check.get('id', 0))
    if (latest.get('head_sha') != sha or latest.get('status') != 'completed'
            or latest.get('conclusion') != 'success'):
        raise ValueError('Independent CodeQL verdict is not successful for this exact source')
    return latest['id']


def main():
    repository, sha = os.environ['GITHUB_REPOSITORY'], os.environ['GITHUB_SHA']
    if repository != 'KiaroSama/Numera-Browser' or not re.fullmatch('[0-9a-f]{40}', sha):
        raise ValueError('CodeQL gate must use the owning repository and exact source')
    result = subprocess.run(['gh', 'api', '--paginate', '--slurp',
        f'repos/{repository}/commits/{sha}/check-runs?per_page=100&filter=latest'],
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        timeout=30, check=False)
    if result.returncode or len(result.stdout) > 4 * 1024 * 1024:
        raise ValueError('Exact-source CodeQL verdict could not be retrieved')
    check = verify(json.loads(result.stdout.decode('utf-8', errors='strict')), sha)
    print(f'Independent CodeQL verdict verified check={check} source_sha={sha}', flush=True)


if __name__ == '__main__':
    with run_log('codeql-verdict'):
        main()
