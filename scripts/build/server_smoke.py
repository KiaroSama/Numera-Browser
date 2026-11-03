"""Verify full wheel notices and exercise the installed server without source imports."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile

from distribution import NOTICES, ROOT
from smoke import Smoke
from run_logging import run_log


def verify_notices(directory: Path) -> Path:
    wheels = list(directory.glob('*py3-none-any.whl'))
    sdists = list(directory.glob('*.tar.gz'))
    if len(wheels) != 1 or len(sdists) != 1:
        raise ValueError('Server build must produce one platform-independent wheel and one sdist')
    with zipfile.ZipFile(wheels[0]) as wheel:
        names = wheel.namelist()
        license_names = [name for name in names if name.endswith('.dist-info/licenses/LICENSE')]
        if len(license_names) != 1 or wheel.read(license_names[0]) != (ROOT / 'LICENSE').read_bytes():
            raise ValueError('Server wheel does not contain the complete project LICENSE')
        for source, destination in NOTICES.items():
            if wheel.read('numera/legal/' + destination) != (ROOT / source).read_bytes():
                raise ValueError('Server wheel changed or omitted a required notice')
        if 'numera/web/index.html' not in names:
            raise ValueError('Server wheel omitted its built dashboard')
    with tarfile.open(sdists[0], 'r:gz') as sdist:
        licenses = [member for member in sdist.getmembers() if member.name.endswith('/LICENSE')
                    and member.name.count('/') == 1]
        if len(licenses) != 1 or sdist.extractfile(licenses[0]).read() != (ROOT / 'LICENSE').read_bytes():
            raise ValueError('Server source distribution omitted its complete LICENSE')
    return wheels[0]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    scratch = ROOT / '.ci-work/server-package-smoke'
    scratch.mkdir(parents=True, exist_ok=True)
    workspace = Path(tempfile.mkdtemp(prefix='wheel-', dir=scratch))
    with run_log('server-package-smoke') as logger:
        wheel = verify_notices(args.directory.resolve())
        venv = workspace / 'venv'
        subprocess.run([sys.executable, '-m', 'venv', str(venv)], check=True,
                       stdin=subprocess.DEVNULL, timeout=60)
        python = venv / ('Scripts/python.exe' if os.name == 'nt' else 'bin/python')
        subprocess.run([str(python), '-m', 'pip', 'install', '--quiet',
                        '-c', str(ROOT / 'scripts/build/requirements.txt'), str(wheel) + '[api,secure]'],
                       check=True, stdin=subprocess.DEVNULL, timeout=120)
        cli = venv / ('Scripts/numera.exe' if os.name == 'nt' else 'bin/numera')
        smoke = Smoke(cli, workspace, 180, logger)
        try:
            smoke.verify(False)
        finally:
            smoke.close()
        shutil.rmtree(workspace)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
