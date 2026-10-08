"""Original first-run copy/merge decisions with synthetic stores and state files."""
from __future__ import annotations

import tempfile
from pathlib import Path
from unittest.mock import patch
from numera import firstrun


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    for overlap, marker, source_present in [(False, False, True), (True, False, True), (False, True, True), (False, False, False)]:
        with tempfile.TemporaryDirectory(prefix='first-run-oracle-', dir=parent) as directory:
            root = Path(directory)
            source, destination = root / 'source', root / 'destination'
            destination.mkdir()
            source_files = {'profiles/a.json': 'source profile α', 'profiles/b.json': 'source profile b',
                            'profiles/.locks/source.lock': 'not imported', 'user-data/a/state.txt': 'source session',
                            'tokens.json': 'source fixture token table', 'dashboard-config.json': 'source folder metadata'} if source_present else {}
            initial = {'profiles/a.json': 'destination wins', 'tokens.json': 'destination fixture token table'} if overlap else {}
            if marker: initial['.imported'] = 'existing marker\n'
            for base, files in [(source, source_files), (destination, initial)]:
                for name, text in files.items():
                    path = base / name; path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(text, encoding='utf-8')
            with patch('numera.firstrun.legacy_dirs', return_value=[source]):
                result = firstrun.run_if_needed(destination)
                again = firstrun.run_if_needed(destination)
            summary = {key: value for key, value in result.items() if key not in ('source', 'destination')} if result else None
            written = {path.relative_to(destination).as_posix(): path.read_text(encoding='utf-8')
                       for path in sorted(destination.rglob('*')) if path.is_file() and '.locks' not in path.parts and path.name != '.imported'}
            found.append({'source': source_files, 'initial': initial, 'result': summary, 'again': again,
                          'written': written, 'marker_present': (destination / '.imported').exists(),
                          'source_unchanged': all((source / name).read_text(encoding='utf-8') == text for name, text in source_files.items())})
    return found
