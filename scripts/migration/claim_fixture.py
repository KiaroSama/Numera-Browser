"""Read a native claim through the retained source session API, fixture roots only."""
from __future__ import annotations

import argparse
import json
import platform
import sys
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera import sessions  # noqa: E402
from run_logging import run_log  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('host')
    args = parser.parse_args()
    if not args.root.is_absolute():
        raise ValueError('Claim fixture root must be absolute')
    with run_log('claim-fixture') as logger:
        # Host equality is isolated, but the actual claim parsing and native
        # job/group observation remain the unmodified production source.
        with patch.object(platform, 'node', return_value=args.host):
            verdict = sessions.state(sessions.read(args.root, 'fixture-profile'))
        logger.info('Source claim verdict=%s', verdict)
        print(json.dumps({'state': verdict}), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
