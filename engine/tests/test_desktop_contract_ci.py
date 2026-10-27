"""Desktop policy tests must run before credential-bearing distribution jobs."""
from copy import deepcopy
import importlib.util
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('desktop_ci_suites', ROOT / 'scripts/ci/suites.py')
suites = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(suites)


@pytest.mark.parametrize('damage', [None, 'caller', 'gated', 'secret', 'platform', 'workers',
                                    'aggregate', 'distribution', 'condition', 'compile', 'test',
                                    'duplicate', 'environment', 'container'])
def test_desktop_contracts_are_independent_required_and_not_repeated(damage):
    workflows = {name: yaml.safe_load((ROOT / f'.github/workflows/{name}.yml').read_text(encoding='utf-8'))
                 for name in ('ci', 'desktop-contracts', 'build-desktop', 'container')}
    workflows = deepcopy(workflows)
    jobs = workflows['ci']['jobs']
    contract = workflows['desktop-contracts']['jobs']['contracts']
    if damage == 'caller':
        jobs['desktop-contracts']['uses'] = './.github/workflows/build-desktop.yml'
    elif damage == 'gated':
        jobs['desktop-contracts']['needs'] = ['coverage', 'codeql']
    elif damage == 'secret':
        jobs['desktop-contracts']['secrets'] = 'inherit'
    elif damage == 'platform':
        contract['strategy']['matrix']['os'].remove('windows-latest')
    elif damage == 'workers':
        contract['strategy']['max-parallel'] = 3
    elif damage == 'aggregate':
        jobs['required']['needs'].remove('desktop-contracts')
    elif damage == 'distribution':
        jobs['distribution']['needs'].remove('desktop-contracts')
    elif damage == 'condition':
        jobs['distribution']['if'] = jobs['distribution']['if'].replace(
            "needs.desktop-contracts.result == 'success'", 'true')
    elif damage in ('compile', 'test'):
        fragment = '--no-run' if damage == 'compile' else '--test-threads=2'
        step = next(step for step in contract['steps'] if fragment in step.get('run', ''))
        step['if'] = "runner.os == 'Linux'"
    elif damage == 'duplicate':
        workflows['build-desktop']['jobs']['desktop']['steps'].append(
            {'run': 'cargo test --locked --no-default-features -- --test-threads=2'})
    elif damage == 'environment':
        workflows['desktop-contracts']['env']['CARGO_BUILD_JOBS'] = '8'
    elif damage == 'container':
        jobs['docker']['uses'] = './.github/workflows/desktop-contracts.yml'
    if damage:
        with pytest.raises(ValueError, match='Desktop contract|Container verification'):
            suites.check_desktop_contracts(workflows)
    else:
        suites.check_desktop_contracts(workflows)
