"""Discover maintained CI suites and reject missing, skipped prerequisite evidence."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts' / 'migration'))
from run_logging import run_log

LIVE_TARGET = 'browser_adapter_live'
SEAMS = {'identity-script': 'init_script_parity',
         'browser-options': 'browser_options_parity',
         'browser-admission': 'browser_admission_parity',
         'browser-resources': 'browser_resources_parity',
         'browser-ip': 'browser_ip_parity', 'browser-hints': 'browser_hints_parity',
         'browser-cookies': 'browser_cookies_parity',
         'browser-extensions': 'browser_extensions_parity', 'cookie-export': 'cookie_export_parity'}
PLATFORM_REASONS = {
    'windows': ('POSIX process groups', 'POSIX process-group observation',
                'a Windows session cannot outlive its job', 'a Windows session always has a job object',
                'a Windows session is in a job object however it was started',
                'Windows adopts a job object and never touches signals',
                'liveness is only answerable on POSIX', 'the orphan case only exists on POSIX',
                'Windows owns the engine through a job object'),
    'posix': ('kernel32 exists only on Windows', 'job objects are Windows-only',
              'only Windows can fail to create the tree primitive', 'only Windows assigns a job object',
              'exclusive file locking is a Windows behaviour'),
    'not-linux': ('requires Linux procfs',),
}
# These two source modules are wholly POSIX-specific; mixed-platform modules
# still require at least one actual pass on every platform where they are run.
WHOLE_MODULE_OS_LIMITS = {
    'win32': {'test_posix_observation_errors': 'POSIX process-group observation',
              'test_shell_death_cleanup': 'Windows owns the engine through a job object'},
}
CAPABILITY_REASONS = {
    'test_webrtc_live': {'no STUN reachability here - nothing to suppress'},
    'test_dead_controls_live': {'no OfflineAudioContext in this browser build',
                               'host reports none of the probe fonts installed'},
    'test_parity_live': {'this build exposes no navigator.userAgentData',
                         'navigator.deviceMemory unavailable in this build',
                         'no WebGL context in this browser build',
                         'navigator.mediaDevices unavailable in this build'},
    'test_relay': {'no non-loopback address here to aim at'},
}


def discover(root: Path = ROOT) -> dict[str, list[Path]]:
    python = sorted((root / 'engine/tests').rglob('test_*.py'))
    if any(path.parent != root / 'engine/tests' for path in python if path.name.endswith('_live.py')):
        raise ValueError('Nested browser suites require explicit hosted tier selection')
    nested_native = list((root / 'engine-rust/tests').glob('*/main.rs'))
    if nested_native:
        raise ValueError('Nested Cargo test entry points require explicit hosted tier mapping')
    native = sorted((root / 'engine-rust/tests').glob('*.rs'))
    dashboard = sorted(path for path in (root / 'dashboard/test').rglob('*')
                       if re.search(r'\.test\.[jt]sx?$', path.name))
    if len({path.stem for path in python}) != len(python):
        raise ValueError('Duplicate Python suite names require qualified report mapping')
    if not any(path.name.endswith('_live.py') for path in python):
        raise ValueError('Browser suite mapping is empty')
    result = {'python': [path for path in python if not path.name.endswith('_live.py')],
              'browser': [path for path in python if path.name.endswith('_live.py')],
              'native': [path for path in native if path.stem != LIVE_TARGET],
              'native-sdk': [path for path in native if path.stem == LIVE_TARGET],
              'dashboard': dashboard,
              'shell': [root / 'desktop/src-tauri/src/main.rs']}
    if any(not paths or any(not path.is_file() for path in paths) for paths in result.values()):
        raise ValueError('A maintained verification tier is absent or empty')
    return result


def python_sources(shard: int = 0, shards: int = 1, root: Path = ROOT) -> list[Path]:
    if not 1 <= shards <= 4 or not 0 <= shard < shards:
        raise ValueError('Invalid source verification partition')
    selected = discover(root)['python'][shard::shards]
    if not selected:
        raise ValueError('Source verification partition is empty')
    return selected


BINARY_SUFFIXES = {'.png', '.ico', '.jpg', '.jpeg', '.gif', '.webp', '.woff', '.woff2',
                   '.ttf', '.otf', '.zip', '.gz', '.zst', '.whl', '.exe', '.dll'}


def check_utf8(paths: list[Path]) -> int:
    checked = 0
    for path in paths:
        if path.suffix.lower() in BINARY_SUFFIXES:
            continue
        try:
            path.read_bytes().decode('utf-8', errors='strict')
        except UnicodeDecodeError as error:
            raise ValueError(f'Tracked text is not UTF-8: {path.name}') from error
        checked += 1
    return checked


def tracked_paths(root: Path = ROOT) -> list[Path]:
    output = subprocess.run(['git', '-C', str(root), 'ls-files', '-z'],
                            capture_output=True, check=True, timeout=30).stdout
    paths = [root / name for name in output.decode('utf-8', errors='strict').split('\0') if name]
    if not paths:
        raise ValueError('Tracked verification scope is empty')
    return paths


def stage_tracked(destination: Path, root: Path = ROOT) -> int:
    import shutil

    if destination.exists():
        raise ValueError('Tracked secret scan requires a new empty destination')
    destination.mkdir(parents=True)
    paths = tracked_paths(root)
    for source in paths:
        relative = source.relative_to(root)
        if source.is_symlink() or not source.is_file():
            raise ValueError('Tracked secret scope contains a link or non-file')
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    return len(paths)


SCANNER_CONFIG_SHA256 = 'e163e53b9e7e8a8511e77271e2b323ed057759542a6d988258afe3a1fa329caf'


def prepare_secret_config(base: Path, destination: Path, root: Path = ROOT) -> None:
    import hashlib

    data = base.read_bytes()
    if hashlib.sha256(data).hexdigest() != SCANNER_CONFIG_SHA256:
        raise ValueError('Official scanner configuration checksum mismatch')
    text = data.decode('utf-8', errors='strict')
    # Remove whole path-dependent rule exceptions: clearing the path of an
    # AND exception would broaden its remaining regex into a global waiver.
    # Singular `path` remains a detection selector, not an exception.
    blocks = re.split(r'(?m)(?=^\[\[)', text)
    text = ''.join(block for block in blocks if not (
        block.startswith('[[rules.allowlists]]') and re.search(r'(?m)^\s*paths\s*=', block)))
    text, count = re.subn(r'(?ms)^([ \t]*)paths = \[[ \t]*\r?\n.*?^[ \t]*\][ \t]*(?=\r?$)',
                          r'\1paths = []', text)
    if count != 1:
        raise ValueError('Official scanner exclusion layout changed; inspect before scanning')
    base.write_text(text, encoding='utf-8')
    template = (root / 'scripts/ci/gitleaks.toml').read_text(encoding='utf-8')
    if template.count('useDefault = true') != 1:
        raise ValueError('Exact reviewed scanner configuration changed')
    config = template.replace('useDefault = true', 'path = ' + json.dumps(str(base.resolve())))
    if destination.exists():
        raise ValueError('Scanner configuration destination must be new')
    destination.write_text(config, encoding='utf-8')


def third_party_requirements(frozen: str) -> list[str]:
    from packaging.requirements import Requirement
    from packaging.utils import canonicalize_name

    selected = []
    local_count = 0
    for line in frozen.splitlines():
        if not line.strip():
            continue
        requirement = Requirement(line)
        if canonicalize_name(requirement.name) == 'numera-engine':
            local_count += 1
            continue
        pins = list(requirement.specifier)
        if requirement.url or requirement.marker or requirement.extras or len(pins) != 1 or pins[0].operator != '==':
            raise ValueError('Resolved third-party audit input is not exactly pinned')
        selected.append(line)
    if local_count != 1 or not selected:
        raise ValueError('Resolved audit scope must exclude exactly one local Numera distribution')
    return selected


def audit_requirements(destination: Path) -> int:
    frozen = subprocess.run([sys.executable, '-m', 'pip', 'freeze', '--all'], check=True,
                            capture_output=True, encoding='utf-8', errors='strict', timeout=30).stdout
    requirements = third_party_requirements(frozen)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text('\n'.join(requirements) + '\n', encoding='utf-8')
    return len(requirements)


def check_dashboard(path: Path, root: Path = ROOT) -> dict[str, int]:
    report = json.loads(path.read_text(encoding='utf-8'))
    expected = {source.resolve() for source in discover(root)['dashboard']}
    observed = {Path(suite['name']).resolve() for suite in report.get('testResults', [])}
    if observed != expected:
        raise ValueError('Dashboard report does not cover every maintained test file')
    if not report.get('success') or report.get('numPassedTests', 0) <= 0:
        raise ValueError('Dashboard report has no successful test evidence')
    if any(report.get(key, 0) for key in ('numFailedTests', 'numPendingTests', 'numTodoTests')):
        raise ValueError('Dashboard tests failed, skipped or were left unimplemented')
    for suite in report['testResults']:
        assertions = suite.get('assertionResults', [])
        if suite.get('status') != 'passed' or not assertions or any(row['status'] != 'passed' for row in assertions):
            raise ValueError('Dashboard suite is incomplete or skipped')
    return {'tests': report['numPassedTests'], 'suites': len(observed)}


def check_native_ignored(paths: list[Path]) -> None:
    for path in paths:
        if re.search(r'#\s*\[\s*ignore(?:\s|\]|=)', path.read_text(encoding='utf-8')):
            raise ValueError(f'Native source has unmapped ignored tests: {path.name}')


def check_source_module_action(action: dict) -> None:
    steps = action.get('runs', {}).get('steps', [])
    settled = "${{ steps.module_8.outputs.settled == 'true' && steps.checkpoint_8.outcome == 'success' }}"
    if (action.get('runs', {}).get('using') != 'composite' or len(steps) != 18
            or action.get('outputs', {}).get('settled', {}).get('value') != settled):
        raise ValueError('Invalid source module checkpoint action topology')
    for index in range(9):
        run, upload = steps[index * 2:index * 2 + 2]
        condition = '${{ !cancelled() }}' if index == 0 else (
            "${{ (success() || failure()) && !cancelled() && steps.module_%s.outputs.settled == 'true' "
            "&& steps.checkpoint_%s.outcome == 'success' }}" % (index - 1, index - 1))
        upload_condition = ("${{ (success() || failure()) && !cancelled() "
                            "&& steps.module_%s.outputs.settled == 'true' }}" % index)
        command = run.get('run', '')
        fragments = ['run_owned.py --wall 600 --idle 120',
                     f'--shard 1 --shards 4 --block 2 --blocks 4 --module-index {index}',
                     'if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }',
                     f'--settled-index {index}',
                     '$state = Get-Content -LiteralPath build/source-windows-1/checkpoint.json -Raw -Encoding utf8 | ConvertFrom-Json',
                     f'$testExit = $state.slot_results[{index}]\n', 'if ($testExit -notin @(0, 1))',
                     "'settled=true'", 'exit [int]$testExit']
        positions = [command.find(fragment) for fragment in fragments]
        if (run.get('id') != f'module_{index}' or run.get('shell') != 'pwsh'
                or run.get('if') != condition or upload.get('if') != upload_condition
                or any(position < 0 for position in positions) or positions != sorted(positions)
                or command.count('if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }') != 2
                or 'run_owned.py --wall 30 --idle 15 -- python scripts/ci/python_source.py --settled-index' not in command
                or run.get('continue-on-error') or upload.get('continue-on-error')
                or upload.get('id') != f'checkpoint_{index}'
                or upload.get('uses') != 'actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9'
                or upload.get('with') != {'name': f'source-windows-checkpoint-2-module-{index}',
                                         'path': 'build/source-windows-1/',
                                         'if-no-files-found': 'error', 'retention-days': 7}):
            raise ValueError('Unsafe or incomplete source module checkpoint step')


def check_native_acceptance_order(steps: list[dict]) -> None:
    uses = './.github/actions/native-entry-performance'
    performance = [index for index, step in enumerate(steps) if step.get('uses') == uses]
    commands = ('native_http_acceptance.py', 'native_parent_acceptance.py',
                'native_console_acceptance.py', 'native_tty_acceptance.py',
                'native_job_acceptance.py', 'native_signal_acceptance.py', 'verify_default.py')
    if len(performance) != 1:
        raise ValueError('Native performance acceptance must occur exactly once')
    for command in commands:
        matches = [index for index, step in enumerate(steps) if command in step.get('run', '')]
        if len(matches) != 1 or matches[0] >= performance[0]:
            raise ValueError('Independent native functional acceptance must precede performance')


def check_desktop_contracts(workflows: dict) -> None:
    jobs = workflows['ci']['jobs']
    caller = {'needs': 'coverage', 'uses': './.github/workflows/desktop-contracts.yml'}
    if jobs.get('desktop-contracts') != caller:
        raise ValueError('Desktop contract verification must precede signing without security dependencies or secrets')
    child = workflows['desktop-contracts']
    if (child.get('permissions') != {'contents': 'read'} or 'secrets.' in json.dumps(child)
            or any(child.get('env', {}).get(key) != value for key, value in {
                'PYTHONUTF8': '1', 'PYTHONIOENCODING': 'utf-8',
                'HOOKMAKER_MAX_TEST_WORKERS': '2', 'CARGO_BUILD_JOBS': '2'}.items())):
        raise ValueError('Desktop contract verification must retain private inputs and bounded workers')
    contract = child['jobs'].get('contracts', {})
    if (set(child['jobs']) != {'contracts'} or contract.get('runs-on') != '${{ matrix.os }}'
            or contract.get('if') or contract.get('timeout-minutes') != 45
            or contract.get('strategy') != {'fail-fast': False, 'max-parallel': 2,
                'matrix': {'os': ['ubuntu-latest', 'windows-latest', 'macos-latest']}}):
        raise ValueError('Desktop contract verification requires all three bounded hosted platforms')
    commands = (
        'run_owned.py --wall 60 --idle 30 -- cargo fmt --all --check',
        'run_owned.py --wall 900 --idle 300 -- cargo clippy --locked --all-targets --no-default-features -- -D warnings',
        'run_owned.py --wall 900 --idle 300 -- cargo test --locked --no-default-features --no-run',
        'run_owned.py --wall 600 --idle 120 -- cargo test --locked --no-default-features -- --test-threads=2',
    )
    steps = contract.get('steps', [])
    for command in commands:
        matches = [step for step in steps if step.get('run') == 'python ../../scripts/migration/' + command]
        if (len(matches) != 1 or matches[0].get('working-directory') != 'desktop/src-tauri'
                or matches[0].get('if') or matches[0].get('continue-on-error')):
            raise ValueError('Desktop contract commands must remain required with unchanged bounds')
    distribution = jobs['distribution']
    if ('desktop-contracts' not in distribution.get('needs', [])
            or "needs.desktop-contracts.result == 'success'" not in distribution.get('if', '')
            or 'desktop-contracts' not in jobs['required'].get('needs', [])):
        raise ValueError('Desktop contract verification must gate distribution and the aggregate')
    for job in workflows['build-desktop']['jobs'].values():
        if any('cargo test' in step.get('run', '') or 'cargo clippy' in step.get('run', '')
               or 'cargo fmt' in step.get('run', '') for step in job.get('steps', [])):
            raise ValueError('Desktop contract commands must not repeat in distribution')
    if jobs.get('docker') != {'needs': 'coverage', 'uses': './.github/workflows/container.yml'}:
        raise ValueError('Container verification must remain required through the same-commit workflow')
    container = workflows['container']
    if (container.get('env') != workflows['ci']['env']
            or container.get('permissions') != {'contents': 'read'}
            or set(container.get('jobs', {})) != {'docker'}):
        raise ValueError('Container verification lost its environment or permission boundary')
    docker = container['jobs']['docker']
    if (docker.get('if') or docker.get('runs-on') != 'ubuntu-latest'
            or docker.get('timeout-minutes') != 30
            or any(fragment not in json.dumps(docker) for fragment in (
                '--wall 1200 --idle 150 -- docker build', '/api/health', '/api/profiles',
                'smoke_container_browser.py', '--network none', 'docker rm -f'))):
        raise ValueError('Container verification lost a bounded functional or cleanup stage')


def check_workflows(root: Path = ROOT) -> dict[str, int]:
    import yaml

    class UniqueLoader(yaml.SafeLoader):
        pass

    def unique_mapping(loader, node, deep=False):
        result = {}
        for key_node, value_node in node.value:
            key = loader.construct_object(key_node, deep=deep)
            if key in result:
                raise ValueError(f'Duplicate workflow key: {key}')
            result[key] = loader.construct_object(value_node, deep=deep)
        return result

    UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping)
    suites = discover(root)
    workflows = {}
    for path in (root / '.github/workflows').glob('*.yml'):
        workflows[path.stem] = yaml.load(path.read_text(encoding='utf-8'), Loader=UniqueLoader)
    for workflow in workflows.values():
        events = workflow.get('on', workflow.get(True, {}))
        events = {events} if isinstance(events, str) else set(events)
        if not events or events - {'workflow_dispatch', 'workflow_call'}:
            raise ValueError('CI workflows must start only from an explicit manual root')
    jobs = workflows['ci']['jobs']
    expected = {'coverage', 'secrets', 'engine', 'engine-windows', 'engine-macos', 'engine-live',
                'browser', 'rust-core', 'docker', 'dashboard', 'codeql', 'dependencies', 'desktop-contracts', 'distribution'}
    if set(jobs) != expected | {'required', 'promote', 'publish'} or set(jobs['required']['needs']) != expected:
        raise ValueError('Required verification does not gate every maintained CI tier')
    if jobs['promote'].get('needs') != 'required' or set(jobs['publish'].get('needs', [])) != {'required', 'promote'}:
        raise ValueError('Promotion and publication must follow the verification aggregate')
    if jobs['required'].get('if') != 'always()':
        raise ValueError('Required verification must reject failed and skipped dependencies')
    checks = {
        'engine': ('--ignore-glob=tests/test_*_live.py', '--junitxml='),
        'engine-windows': ('python_source.py --shard ${{ matrix.shard }} --shards 4',
                           'report --tier python', '--shard ${{ matrix.shard }} --shards 4'),
        'engine-macos': ('--ignore-glob=tests/test_*_live.py', '--junitxml='),
        'browser': ('tests/test_*_live.py', '--deselect=', 'xvfb-run', 'update_ui_smoke.py', 'rendered-update-ui'),
        'rust-core': ('suites.py native --phase history', 'suites.py native --phase build', 'suites.py native --phase test',
                      'verify_oracles.py', 'verify_default.py', './.github/actions/native-entry-performance'),
        'engine-live': ('test_session_tree_live.py', './.github/actions/native-candidate-acceptance',
                        'report --engine ${{ matrix.engine }}'),
        'secrets': ('--redact=100', '--ignore-gitleaks-allow', '--timeout 240',
                    'stage-tracked', '--log-opts=--all', 'sha256sum --check --strict'),
    }
    for job, fragments in checks.items():
        text = json.dumps(jobs[job])
        if any(fragment not in text for fragment in fragments):
            raise ValueError(f'Missing suite command in CI tier: {job}')
    check_native_acceptance_order(jobs['rust-core']['steps'])
    for workflow_name in ('ci', 'dashboard', 'build-desktop', 'desktop-contracts', 'container',
                          'codeql', 'dependency-review', 'migration-profile'):
        for name, job in workflows[workflow_name]['jobs'].items():
            if job.get('continue-on-error') or 'self-hosted' in str(job.get('runs-on', '')):
                raise ValueError(f'Unbounded or softened hosted tier: {workflow_name}/{name}')
            if 'runs-on' in job and not isinstance(job.get('timeout-minutes'), int):
                raise ValueError(f'Missing job deadline: {workflow_name}/{name}')
            if any(step.get('continue-on-error') for step in job.get('steps', [])):
                raise ValueError(f'Softened required step: {workflow_name}/{name}')
    verdict = workflows['codeql']['jobs'].get('verdict', {})
    if (verdict.get('needs') != 'analyze' or 'codeql_gate.py' not in json.dumps(verdict)
            or 'wait-for-processing' not in json.dumps(workflows['codeql']['jobs']['analyze'])):
        raise ValueError('Signing must require an independent exact-source CodeQL verdict')
    if jobs['distribution'].get('uses') != './.github/workflows/build-desktop.yml':
        raise ValueError('Distribution verification is not reused by the full CI gate')
    check_desktop_contracts(workflows)
    for job in ('desktop', 'desktop-windows'):
        text = json.dumps(workflows['build-desktop']['jobs'][job])
        if 'update_acceptance_build.py' not in text or 'update_acceptance.py run' not in text:
            raise ValueError('Three-platform updater activation is not required by distribution verification')
    if 'macos-latest' not in json.dumps(workflows['build-desktop']['jobs']['desktop']):
        raise ValueError('Requested macOS distribution is not mapped')
    dashboard_text = json.dumps(workflows['dashboard']['jobs'])
    if 'npm test' not in dashboard_text or 'dashboard-dist' not in dashboard_text:
        raise ValueError('Dashboard suite and exact-run build artifact are not mapped')
    windows_steps = jobs['engine-windows']['steps']
    for block in range(4):
        run = next((step for step in windows_steps if step.get('name') == f'Partition 1 block {block}'), {})
        upload = next((step for step in windows_steps if step.get('name') == f'Preserve partition 1 checkpoint {block}'), {})
        correct_run = (run.get('uses') == './.github/actions/source-module-checkpoints'
                       and run.get('id') == 'source_modules' if block == 2
                       else f'--shard 1 --shards 4 --block {block} --blocks 4' in run.get('run', ''))
        condition = 'matrix.shard == 1 && !cancelled()'
        settled = " && steps.source_modules.outputs.settled == 'true'"
        if (not correct_run or run.get('timeout-minutes') != 12
                or run.get('if') != condition + (settled if block == 3 else '')
                or upload.get('if') != condition + (settled if block == 2 else '')
                or upload.get('with', {}).get('name') != f'source-windows-checkpoint-{block}'):
            raise ValueError('Missing complete bounded Windows checkpoint diagnostics')
    action = root / '.github/actions/source-module-checkpoints/action.yml'
    check_source_module_action(yaml.load(action.read_text(encoding='utf-8'), Loader=UniqueLoader))
    live_steps = jobs['engine-live']['steps']
    checkouts = [index for index, step in enumerate(live_steps)
                 if step.get('uses', '').startswith('actions/checkout@')]
    acceptance = [index for index, step in enumerate(live_steps)
                  if step.get('uses') == './.github/actions/native-candidate-acceptance']
    if (len(checkouts) != 1 or len(acceptance) != 1 or checkouts[0] >= acceptance[0]
            or live_steps[checkouts[0]].get('if')
            or live_steps[checkouts[0]].get('with', {}).get('fetch-depth') != 0):
        raise ValueError('Browser acceptance requires original history before comparison')
    candidate_action = yaml.load((root / '.github/actions/native-candidate-acceptance/action.yml')
                                 .read_text(encoding='utf-8'), Loader=UniqueLoader)
    candidate_steps = candidate_action.get('runs', {}).get('steps', [])
    if (candidate_action.get('runs', {}).get('using') != 'composite' or len(candidate_steps) != 7
            or any(step.get('continue-on-error') for step in candidate_steps)
            or any(step.get('if') for step in candidate_steps[:6])
            or candidate_steps[6].get('if') != 'always()'):
        raise ValueError('Native application acceptance must remain required and complete')
    for step, fragments in zip(candidate_steps, (
            ('--test browser_adapter_live', '--ignored', '--test-threads=1'),
            ('dashboard-dist', 'build/native-candidate/dashboard'),
            ('ci-source.json', 'GITHUB_SHA'),
            ('candidate_stage.py --release --report', '--wall 960 --idle 330'),
            ('candidate_acceptance.py --report', '--wall 600 --idle 150 --report-units'),
            ('benchmark_browser.py --report', '--wall 600 --idle 150 --report-units',
             '--engine', '--destination build/native-candidate/browser-performance.json'),
            ('browser-performance-${{ runner.os }}-${{ inputs.engine }}', 'browser-performance.json'))):
        if any(fragment not in json.dumps(step) for fragment in fragments):
            raise ValueError('Missing required native application acceptance stage')
    if candidate_steps[6].get('with', {}).get('if-no-files-found') != 'error':
        raise ValueError('Missing browser measurements must fail artifact acceptance')
    performance = yaml.load((root / '.github/actions/native-entry-performance/action.yml')
                            .read_text(encoding='utf-8'), Loader=UniqueLoader)
    steps = performance.get('runs', {}).get('steps', [])
    if (performance.get('runs', {}).get('using') != 'composite' or len(steps) != 4
            or any(step.get('continue-on-error') or step.get('if') for step in steps[:3])
            or steps[3].get('if') != 'always()'):
        raise ValueError('Native replay and performance checks must remain required')
    for step, fragments in zip(steps, (
            ('native_entry_acceptance.py', '--wall 180 --idle 90 --report-units'),
            ('candidate_stage.py --release', '--wall 960 --idle 330'),
            ('benchmark.py --report', '--wall 180 --idle 90 --report-units'),
            ('measurements.json', 'native-performance-${{ runner.os }}'))):
        if any(fragment not in json.dumps(step) for fragment in fragments):
            raise ValueError('Missing native replay, optimized measurement or evidence stage')
    windows_strategy = jobs['engine-windows']['strategy']
    if (windows_strategy['matrix'] != {'shard': [0, 1, 2, 3]}
            or windows_strategy['max-parallel'] != 2 or windows_strategy['fail-fast'] is not False):
        raise ValueError('Windows source partitions must remain complete and independently diagnostic')
    if jobs['engine-live']['strategy']['matrix']['os'] != ['ubuntu-latest', 'windows-latest']:
        raise ValueError('SDK coverage must retain both supported native ownership platforms')
    if set(jobs['engine-live']['strategy']['matrix']['engine']) != {'playwright', 'patchright', 'camoufox', 'cloak'}:
        raise ValueError('A supported SDK is absent from the required live matrix')
    check_native_ignored(suites['native'] + sorted((root / 'engine-rust/src').rglob('*.rs')))
    manifest = (root / 'engine-rust/Cargo.toml').read_text(encoding='utf-8')
    if re.search(r'autotests\s*=\s*false|\[\[test\]\]', manifest):
        raise ValueError('Native target discovery requires explicit mapping after Cargo configuration changes')
    return {tier: len(paths) for tier, paths in suites.items()}


def native_command(phase: str, root: Path = ROOT) -> list[str]:
    command = ['cargo', 'test', '--manifest-path', str(root / 'engine-rust/Cargo.toml'),
               '--locked', '--lib']
    for path in discover(root)['native']:
        command.extend(('--test', path.stem))
    command.extend(('--no-fail-fast', '--message-format=json,json-render-diagnostics'))
    if phase == 'build':
        command.append('--no-run')
    else:
        command.extend(('--', '--test-threads=2'))
    return command


def run_native(command: list[str], environment: dict[str, str]) -> int:
    # The outer run_owned owner retains the existing wall/idle/tree limits.
    with subprocess.Popen(command, env=environment, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, text=True, encoding='utf-8', errors='strict') as child:
        finished = False
        for line in child.stdout:
            if finished or not line.lstrip().startswith('{'):
                print(line, end='', flush=True)
                continue
            # Build-script records can contain environment values. Never forward raw JSON.
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if not isinstance(event, dict):
                continue
            if event.get('reason') == 'compiler-artifact':
                target = event.get('target')
                name = target.get('name') if isinstance(target, dict) else None
                fresh = event.get('fresh')
                if isinstance(name, str) and re.fullmatch(r'[A-Za-z0-9_-]{1,128}', name) and type(fresh) is bool:
                    print(f'Cargo artifact target={name} fresh={str(fresh).lower()}', flush=True)
            elif event.get('reason') == 'build-finished' and type(event.get('success')) is bool:
                print(f"Cargo build finished success={str(event['success']).lower()}", flush=True)
                finished = True
        return child.wait()


def seam_command(phase: str, scope: str, root: Path = ROOT) -> list[str]:
    target = SEAMS[scope]
    if not (root / 'engine-rust/tests' / f'{target}.rs').is_file():
        raise ValueError('Requested native seam is absent')
    command = ['cargo', 'test', '--manifest-path', str(root / 'engine-rust/Cargo.toml'),
               '--locked', '--test', target]
    if phase == 'build':
        command.append('--no-run')
    else:
        command.extend(('--', '--test-threads=1', '--nocapture'))
    return command


def check_report(path: Path, tier: str, root: Path = ROOT, *,
                 shard: int = 0, shards: int = 1) -> dict[str, int]:
    sources = python_sources(shard, shards, root) if tier == 'python' else discover(root)[tier]
    expected = {source.stem for source in sources}
    reports = sorted(path.glob('*.xml')) if path.is_dir() else [path]
    if path.is_dir() and {item.stem for item in reports} != expected:
        raise ValueError('Source report inventory differs from its verification partition')
    cases = [case for report in reports for case in ET.parse(report).iter('testcase')]
    if not cases:
        raise ValueError('Verification report is empty')
    observed = set()
    passed_by_module = dict.fromkeys(expected, 0)
    module_skip_reasons = {module: [] for module in expected}
    passed = skipped = 0
    for case in cases:
        module = next((part for part in case.get('classname', '').split('.')
                       if part in expected), '')
        if module not in expected:
            # Module-level collection skips use the file name as their testcase name.
            module = Path(case.get('name', '')).name.split('.py', 1)[0]
        if module not in expected:
            raise ValueError(f'Unexpected verification report module: {module}')
        observed.add(module)
        if case.find('failure') is not None or case.find('error') is not None:
            raise ValueError(f'Failed verification case: {module}/{case.get("name")}')
        skip = case.find('skipped')
        if skip is None:
            passed += 1
            passed_by_module[module] += 1
            continue
        if skip.get('type') == 'pytest.xfail':
            raise ValueError(f'Expected failure is not passing coverage: {module}/{case.get("name")}')
        skipped += 1
        reason = skip.get('message', '') + ' ' + (skip.text or '')
        module_skip_reasons[module].append(reason)
        platform_reasons = PLATFORM_REASONS['windows' if sys.platform == 'win32' else 'posix']
        if sys.platform != 'linux':
            platform_reasons += PLATFORM_REASONS['not-linux']
        allowed = any(value in reason for value in platform_reasons)
        allowed |= any(value in reason for value in CAPABILITY_REASONS.get(module, set()))
        if not allowed:
            raise ValueError(f'Unexpected skip: {module}/{case.get("name")}: {reason[:180]}')
        print(f'Explicit platform/capability limit: {module}/{case.get("name")}: {reason[:180]}', flush=True)
    if observed != expected:
        raise ValueError('Maintained suite missing from report: ' + ', '.join(sorted(expected - observed)))
    whole_os_limits = WHOLE_MODULE_OS_LIMITS.get(sys.platform, {})
    for module, count in passed_by_module.items():
        if count:
            continue
        required_reason = whole_os_limits.get(module)
        reasons = module_skip_reasons[module]
        if not required_reason or not reasons or not all(required_reason in reason for reason in reasons):
            raise ValueError(f'Maintained suite has no passing cases: {module}')
    if passed == 0:
        raise ValueError('No maintained verification case passed')
    return {'passed': passed, 'explicit_limits': skipped, 'suites': len(observed)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('coverage')
    staged = sub.add_parser('stage-tracked')
    staged.add_argument('--destination', type=Path, required=True)
    scanner = sub.add_parser('secret-config')
    scanner.add_argument('--base', type=Path, required=True)
    scanner.add_argument('--destination', type=Path, required=True)
    requirements = sub.add_parser('audit-requirements')
    requirements.add_argument('--destination', type=Path, required=True)
    native = sub.add_parser('native')
    native.add_argument('--phase', choices=('history', 'build', 'test'), required=True)
    seam = sub.add_parser('seam')
    seam.add_argument('--phase', choices=('build', 'test'), required=True)
    seam.add_argument('--scope', choices=tuple(SEAMS), required=True)
    report = sub.add_parser('report')
    report.add_argument('--tier', choices=('python', 'browser', 'dashboard'), required=True)
    report.add_argument('--report', type=Path, required=True)
    report.add_argument('--shard', type=int, default=0)
    report.add_argument('--shards', type=int, default=1)
    args = parser.parse_args()
    with run_log('hosted-suite-coverage') as log:
        if args.command == 'coverage':
            counts = check_workflows()
            checked = check_utf8(tracked_paths())
            log.info('Maintained tiers mapped counts=%s utf8_text_files=%s', json.dumps(counts, sort_keys=True), checked)
        elif args.command == 'audit-requirements':
            count = audit_requirements(args.destination)
            log.info('All installed third-party audit inputs pinned distributions=%s; only local Numera excluded', count)
        elif args.command == 'secret-config':
            prepare_secret_config(args.base, args.destination)
            log.info('Checksum-verified scanner rules retained; inherited path omissions removed')
        elif args.command == 'stage-tracked':
            count = stage_tracked(args.destination)
            log.info('Exact tracked secret scope staged files=%s', count)
        elif args.command == 'report':
            result = (check_dashboard(args.report) if args.tier == 'dashboard'
                      else check_report(args.report, args.tier, shard=args.shard, shards=args.shards))
            log.info('Actual report verified tier=%s result=%s', args.tier, json.dumps(result))
        else:
            command = (seam_command(args.phase, args.scope) if args.command == 'seam'
                       else native_command(args.phase))
            log.info('Native phase=%s tier=%s workers=2', args.phase, args.command)
            if args.command == 'native' and args.phase == 'history':
                from baseline_source import reference
                from native_regressions import prove
                subprocess.run(['cargo', 'fetch', '--manifest-path', str(ROOT / 'engine-rust/Cargo.toml'),
                                '--locked'], stdin=subprocess.DEVNULL, check=True)
                with reference() as original:
                    prove(ROOT, original, log)
                return 0
            if args.phase == 'test':
                import os
                from baseline_source import ENVIRONMENT, reference
                with reference() as original:
                    environment = {**os.environ, ENVIRONMENT: str(original), 'PYTHONDONTWRITEBYTECODE': '1'}
                    if args.command == 'native':
                        return run_native(command, environment)
                    return subprocess.run(command, env=environment, stdin=subprocess.DEVNULL, check=False).returncode
            if args.command == 'native':
                import os
                return run_native(command, dict(os.environ))
            return subprocess.run(command, stdin=subprocess.DEVNULL, check=False).returncode
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, ET.ParseError) as error:
        print(f'Hosted suite coverage failed: {error}', file=sys.stderr)
        raise SystemExit(1)
