"""Settings read/merge outcomes from the original ConfigStore, isolated files."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera.config_store import ConfigStore


def cases(parent: Path) -> list[dict]:
    inputs = [None, '{broken', '[]', 'null', 'true', '42',
              '{"future":{"kept":[1,"α"]},"headless_launch":false}',
              '{"default_engine":"camoufox","default_extensions":["a"],"future":7}']
    found = []
    parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='config-oracle-', dir=parent) as directory:
        root = Path(directory)
        store = ConfigStore()
        store.root, store.config_path = root, root / 'config.json'
        for raw in inputs:
            for changes in ({}, {'headless_launch': True, 'default_engine': 'patchright'}, {'future': None}):
                store.config_path.unlink(missing_ok=True)
                store._config_cache = None
                if raw is not None:
                    store.config_path.write_text(raw, encoding='utf-8')
                read = store.get_config()
                before = store.config_path.read_bytes() if store.config_path.exists() else None
                try:
                    result = store.set_config_many(changes)
                    error = None
                except (AttributeError, TypeError) as exc:
                    result, error = None, type(exc).__name__
                after = store.config_path.read_bytes() if store.config_path.exists() else None
                try:
                    disk = json.loads(after) if after else None
                except json.JSONDecodeError:
                    disk = None
                found.append({'input': raw, 'changes': changes, 'read': read,
                              'result': result, 'error_type': error,
                              'unchanged': before == after, 'disk': disk})
    return found
