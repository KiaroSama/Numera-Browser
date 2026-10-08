"""Discover source contracts without importing or executing application modules."""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from run_logging import run_log

ROOT = Path(__file__).resolve().parents[2]
BASELINE = '13b6d9c6973281fe123353eaec05b29d65781a2d'


def tracked_paths(prefix: str) -> list[Path]:
    result = subprocess.run(
        ['git', '-C', str(ROOT), 'ls-tree', '-rz', '--name-only', BASELINE, '--', prefix],
        check=True, capture_output=True, timeout=10,
    )
    return sorted(ROOT / name.decode('utf-8') for name in result.stdout.split(b'\0') if name)


def describe(path: Path) -> dict:
    raw = path.read_bytes()
    tree = ast.parse(raw.decode('utf-8'), filename=str(path))
    symbols, tests, routes = [], [], []

    def walk(nodes, parent=''):
        for node in nodes:
            if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
                name = f'{parent}.{node.name}' if parent else node.name
                symbols.append(name)
                if node.name.startswith('test_'):
                    tests.append(name)
                for decorator in getattr(node, 'decorator_list', []):
                    if (isinstance(decorator, ast.Call)
                            and isinstance(decorator.func, ast.Attribute)
                            and decorator.func.attr in {'get', 'post', 'put', 'patch', 'delete', 'options', 'head'}
                            and decorator.args and isinstance(decorator.args[0], ast.Constant)
                            and isinstance(decorator.args[0].value, str)):
                        routes.append({'method': decorator.func.attr.upper(), 'path': decorator.args[0].value, 'handler': name})
                walk(node.body, name)
            elif isinstance(node, (ast.If, ast.With, ast.AsyncWith, ast.Try)):
                walk(node.body, parent)
                walk(getattr(node, 'orelse', []), parent)
                walk(getattr(node, 'finalbody', []), parent)
                for handler in getattr(node, 'handlers', []):
                    walk(handler.body, parent)
    walk(tree.body)
    return {'path': path.relative_to(ROOT).as_posix(),
            'sha256': hashlib.sha256(raw).hexdigest(), 'symbols': symbols,
            'tests': tests, 'routes': routes}


def discover() -> dict:
    files = [describe(p) for prefix in ('engine/numera', 'engine/tests')
             for p in tracked_paths(prefix) if p.suffix == '.py']
    return {'schema': 1, 'source_commit': BASELINE, 'files': files,
            'capabilities': [
                {'id': f'{row["path"]}::{name}', 'target': None, 'test': None, 'status': 'pending'}
                for row in files for name in row['symbols']
                if not row['path'].startswith('engine/tests/') and name.rsplit('.', 1)[-1] != '__init__'
            ]}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', type=Path, help='Check committed source inventory for drift')
    parser.add_argument('--output', type=Path, help='Create a source inventory without overwriting one')
    args = parser.parse_args(argv)
    inventory = discover()
    if args.check:
        expected = json.loads(args.check.read_text(encoding='utf-8'))
        if inventory['files'] != expected['files'] or inventory['source_commit'] != expected['source_commit']:
            print('Migration source inventory drifted; reconcile contracts before release', file=sys.stderr)
            return 1
    elif args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        # One record per file/capability, rather than a huge expanded source dump.
        with args.output.open('x', encoding='utf-8', newline='\n') as stream:
            stream.write(json.dumps(inventory, ensure_ascii=True, separators=(',', ':')) + '\n')
        print(f'Source inventory: {len(inventory["files"])} files, {len(inventory["capabilities"])} pending symbols')
    else:
        print(json.dumps(inventory, ensure_ascii=True, separators=(',', ':')))
    return 0


if __name__ == '__main__':
    with run_log('inventory') as log:
        code = main()
        log.info('Inventory verdict exit=%s', code)
    raise SystemExit(code)
