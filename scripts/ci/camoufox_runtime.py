"""Verify the pinned Camoufox runtime on its owning Ubuntu hosted runner."""
from __future__ import annotations

from importlib.metadata import version
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log  # noqa: E402

# Playwright 1.62 Ubuntu24.04 Firefox libraries, then Camoufox 0.5.7 CI extras.
PACKAGES = (
    'libasound2t64', 'libatk1.0-0t64', 'libavcodec60', 'libcairo-gobject2',
    'libcairo2', 'libdbus-1-3', 'libfontconfig1', 'libfreetype6',
    'libgdk-pixbuf-2.0-0', 'libglib2.0-0t64', 'libgtk-3-0t64',
    'libpango-1.0-0', 'libpangocairo-1.0-0', 'libx11-6', 'libx11-xcb1',
    'libxcb-shm0', 'libxcb1', 'libxcomposite1', 'libxcursor1', 'libxdamage1',
    'libxext6', 'libxfixes3', 'libxi6', 'libxrandr2', 'libxrender1',
    'xvfb', 'libdbus-glib-1-2', 'libxtst6', 'libpci3', 'zstd',
)


def require_host() -> None:
    expected = {
        'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'KiaroSama/Numera-Browser',
        'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64', 'NUMERA_TEST_ENGINE': 'camoufox',
    }
    if any(os.environ.get(key) != value for key, value in expected.items()):
        raise ValueError('Camoufox runtime installation requires its owning Linux hosted job')
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise ValueError('Camoufox runtime packages require Linux x86_64')
    release = platform.freedesktop_os_release()
    if release.get('ID') != 'ubuntu' or release.get('VERSION_ID') != '24.04':
        raise ValueError('Camoufox runtime packages require Ubuntu 24.04')
    if version('camoufox') != '0.5.7' or version('playwright') != '1.62.0':
        raise ValueError('Camoufox runtime package contract differs from the pinned SDKs')


def missing_packages() -> list[str]:
    result = subprocess.run(
        ['dpkg-query', '-W', '-f=${Package}\t${Architecture}\t${db:Status-Status}\t${db:Status-Eflag}\n',
         '--', *PACKAGES], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, encoding='utf-8', errors='strict', timeout=10,
        env={**os.environ, 'LC_ALL': 'C.UTF-8', 'LANGUAGE': 'C'},
    )
    if result.returncode not in (0, 1):
        raise RuntimeError('Camoufox runtime package database query failed')
    if len(result.stdout.encode('utf-8')) > 32768:
        raise ValueError('Camoufox runtime package observation exceeds its bound')
    found = {}
    for line in result.stdout.splitlines():
        fields = line.split('\t')
        if len(fields) != 4:
            raise ValueError('Camoufox runtime package observation is malformed')
        name, architecture, state, error = fields
        if (name not in PACKAGES or name in found
                or architecture not in ('amd64', 'all') or error != 'ok'
                or state not in ('installed', 'not-installed', 'config-files')):
            raise ValueError('Camoufox runtime package state is ambiguous or unhealthy')
        found[name] = state
    if ((result.returncode == 0 and len(found) != len(PACKAGES))
            or (result.returncode == 1 and len(found) == len(PACKAGES))):
        raise ValueError('Camoufox runtime package query result is inconsistent')
    return [name for name in PACKAGES if found.get(name) != 'installed']


def prepare(logger) -> None:
    require_host()
    missing = missing_packages()
    if missing:
        logger.info('Runtime packages missing count=%s packages=%s', len(missing), ','.join(missing))
        env = {**os.environ, 'DEBIAN_FRONTEND': 'noninteractive', 'LC_ALL': 'C.UTF-8', 'LANGUAGE': 'C'}
        prefix = ['sudo', '-n', 'env', 'DEBIAN_FRONTEND=noninteractive',
                  'LC_ALL=C.UTF-8', 'LANGUAGE=C', 'apt-get']
        for command in (
            [*prefix, 'update'],
            [*prefix, 'install', '-y', '--no-install-recommends',
             '--no-remove', '--no-upgrade', *missing],
        ):
            # The existing outer owner retains the shared 300 wall / 120 idle bound.
            result = subprocess.run(command, stdin=subprocess.DEVNULL, env=env, timeout=270)
            if result.returncode:
                raise RuntimeError('Camoufox runtime package installation failed')
        if missing_packages():
            raise RuntimeError('Camoufox runtime packages remain missing after installation')
    logger.info('Runtime packages verified count=%s', len(PACKAGES))


def main() -> int:
    try:
        with run_log('camoufox-runtime') as logger:
            prepare(logger)
    except Exception as error:
        print(f'Camoufox runtime prerequisites failed ({type(error).__name__})', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
