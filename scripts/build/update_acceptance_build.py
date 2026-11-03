"""Build two isolated fixture generations; never sign or publish them as owner builds."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from run_logging import run_log
from identity import prepare, verify
from signing import native_sign
from distribution import digest
from update_acceptance import identity, prepare as fixture_recipe

SIGNING_INPUTS = ('NUMERA_RELEASE_SIGNING_KEY', 'NUMERA_CODE_SIGNING_PFX', 'NUMERA_CODE_SIGNING_PASSWORD')


def command(arguments, env, log, *, seconds, cwd=ROOT):
    from numera import proctree
    log.info('Fixture build tool=%s', Path(arguments[0]).name)
    process, tree = proctree.spawn([str(value) for value in arguments], env=env, cwd=cwd,
                                   stdin=subprocess.DEVNULL)
    log.info('Fixture build owner pid=%s', process.pid)
    try:
        code = process.wait(timeout=seconds)
        if code:
            raise RuntimeError(f'Fixture build failed tool={Path(arguments[0]).name} exit={code}')
    finally:
        proctree.end_session(process, tree, grace=1, kill_grace=3)


def freeze(identity_directory, output, scratch, env, log):
    arguments = [sys.executable, '-m', 'PyInstaller', '--noconfirm', '--clean', '--onedir',
                 '--console', '--name', 'numera-engine', '--paths', identity_directory,
                 '--distpath', output, '--workpath', scratch / 'work', '--specpath', scratch,
                 '--add-data', str(ROOT / 'dashboard/dist') + os.pathsep + 'numera/web']
    if os.name == 'nt':
        arguments += ['--version-file', identity_directory / 'windows-version.txt',
                      '--icon', ROOT / 'desktop/src-tauri/icons/icon.ico']
    for package in ('uvicorn', 'playwright', 'patchright', 'camoufox', 'cloakbrowser', 'browserforge',
                    'apify_fingerprint_datapoints', 'language_tags', 'ua_parser', 'screeninfo', 'maxminddb'):
        arguments += ['--collect-all', package]
    arguments += ['--collect-submodules', 'numera']
    for package in ('camoufox', 'cloakbrowser'):
        arguments += ['--recursive-copy-metadata', package]
    for package in ('patchright', 'playwright'):
        arguments += ['--copy-metadata', package]
    arguments += [ROOT / 'scripts/engine_entry.py']
    command(arguments, env, log, seconds=540)


def browser_fixture(source: Path, target: Path):
    """Keep real compatibility metadata and one real resource, not duplicate browsers."""
    source = source.resolve(strict=True)
    target.mkdir()
    shutil.copy2(source / 'manifest.json', target / 'manifest.json')
    selected = None
    for path in source.rglob('*'):
        if (path.is_symlink() or not path.is_file() or path.name == 'manifest.json'
                or path.suffix.lower() not in {'.json', '.txt', '.pak', '.dat'}
                or not 0 < path.stat().st_size <= 8 * 1024 * 1024):
            continue
        if not path.resolve(strict=True).is_relative_to(source):
            raise ValueError('Fixture browser resource is outside the bundle')
        selected = path
        break
    if selected is None:
        raise ValueError('No bounded actual browser resource is available for updater acceptance')
    relative = selected.relative_to(source)
    destination = target / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(selected, destination)
    checksum = digest(selected)
    if digest(destination) != checksum:
        raise ValueError('Fixture browser resource copy differs')
    (target.parent / 'ACCEPTANCE-RESOURCES.json').write_text(json.dumps({
        'acceptance': True, 'coverage': 'metadata-and-one-real-resource-only',
        'path': relative.as_posix(), 'sha256': checksum, 'size': selected.stat().st_size,
        'full_browser_binary_acceptance': False}) + '\n', encoding='utf-8')


def build(stage: Path, directory: Path, platform: str, log):
    if any(os.environ.get(name) for name in SIGNING_INPUTS):
        raise ValueError('Updater acceptance must never inherit owner signing inputs')
    verify(stage)
    directory.mkdir(parents=True, exist_ok=False)
    recipe = fixture_recipe(directory / 'recipe')
    env = os.environ.copy()
    env.update(NUMERA_ACCEPTANCE_KEY=recipe['public_key'], NUMERA_SIGNING_SCOPE='validation-only')
    executable = 'numera.exe' if os.name == 'nt' else 'numera'
    target = ROOT / 'desktop/src-tauri/target'
    if os.environ.get('CARGO_TARGET_DIR'):
        raise ValueError('Fixture build requires the documented owned Cargo target')
    for name in ('old', 'new'):
        generation = recipe[name + '_generation']
        env['NUMERA_ACCEPTANCE_GENERATION'] = str(generation)
        identity_directory = directory / (name + '-identity')
        prepare(identity_directory, acceptance_generation=generation)
        # Build-script env tracking recompiles the actual binary for each identity;
        # common dependencies are reused, not rebuilt into another target directory.
        if name == 'old':
            command(['cargo', 'clippy', '--locked', '--all-targets', '--features', 'update-acceptance',
                     '--', '-D', 'warnings'], env, log, seconds=900, cwd=ROOT / 'desktop/src-tauri')
        command(['cargo', 'build', '--release', '--locked', '--features', 'update-acceptance'],
                env, log, seconds=900, cwd=ROOT / 'desktop/src-tauri')
        product = directory / name / 'Numera Browser'
        product.mkdir(parents=True)
        shutil.copy2(target / 'release' / executable, product / executable)
        scratch = directory / (name + '-freeze')
        scratch.mkdir()
        freeze(identity_directory, scratch / 'dist', scratch, env, log)
        shutil.move(str(scratch / 'dist/numera-engine'), product / 'engine')
        browser_fixture(stage / 'browsers', product / 'browsers')
        generation_bytes = sum(path.stat().st_size for path in product.rglob('*') if path.is_file())
        free = shutil.disk_usage(directory).free
        log.info('Fixture disk budget generation_bytes=%s free_bytes=%s', generation_bytes, free)
        if free < generation_bytes * 3 + 512 * 1024 * 1024:
            raise ValueError('Hosted disk cannot retain the actual updater fixture and rollback safely')
        native_sign(product, platform, ROOT / 'docs/code-signing-cert.pem', fixture=True)
        record = identity(product, env)
        if record['generation'] != generation:
            raise ValueError('Fixture generation was not embedded in both real executables')
        log.info('Actual fixture generation built name=%s generation=%s', name, generation)
        # Only this invocation's disposable compiler/freezer scratch is removed.
        shutil.rmtree(scratch)
    # The production stage was copied before feature compilation and is not
    # replaced with target/release output; verify its exact original identity again.
    verify(stage)
    (directory / 'ready.json').write_text(json.dumps({'acceptance': True, 'platform': platform,
        'old_generation': recipe['old_generation'], 'new_generation': recipe['new_generation']}) + '\n', encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stage', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--platform', choices=('windows-x64', 'linux-x64', 'macos-arm64'), required=True)
    args = parser.parse_args()
    with run_log('update-acceptance-build') as logger:
        build(args.stage.resolve(strict=True), args.directory.resolve(), args.platform, logger)
