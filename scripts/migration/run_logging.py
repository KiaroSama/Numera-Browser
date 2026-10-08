"""Run-owned, sanitized UTF-8 diagnostics for local migration tools."""
from __future__ import annotations

import logging
import sys
import time
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera.logs import RedactingFilter, UTCFormatter  # noqa: E402


@contextmanager
def run_log(name: str, directory: Path | None = None):
    directory = directory or ROOT / 'scripts' / 'logs'
    logger = logging.getLogger(f'numera.migration-{name}')
    logger.setLevel(logging.INFO)
    old_propagate = logger.propagate
    logger.propagate = False
    handlers = [logging.StreamHandler(sys.stderr)]
    started = time.monotonic()
    try:
        try:
            directory.mkdir(parents=True, exist_ok=True)
            stamp = time.strftime('%Y-%m-%d_%H-%M-%S', time.gmtime())
            for suffix in range(1000):
                tail = f'.{suffix}' if suffix else ''
                try:
                    handlers.append(logging.FileHandler(directory / f'{name}_{stamp}_UTC{tail}.log', mode='x', encoding='utf-8'))
                    break
                except FileExistsError:
                    continue
            else:
                raise OSError('Run log namespace exhausted')
        except OSError as error:
            print(f'Migration log unavailable ({type(error).__name__}); console only', file=sys.stderr)
        for handler in handlers:
            handler.setFormatter(UTCFormatter('[%(asctime)s] [%(levelname)s] [%(name)s] %(message)s'))
            handler.addFilter(RedactingFilter())
            logger.addHandler(handler)
        logger.info('Starting runtime=%s', sys.version.split()[0])
        try:
            yield logger
        except Exception as error:
            logger.error('Failed type=%s exit=1 duration=%.3fs', type(error).__name__, time.monotonic() - started)
            raise
        else:
            logger.info('Finished exit=0 duration=%.3fs', time.monotonic() - started)
    finally:
        for handler in handlers:
            logger.removeHandler(handler)
            handler.close()
        logger.propagate = old_propagate
