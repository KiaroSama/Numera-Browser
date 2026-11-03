"""Cheap load-bearing controls for exact shared-store replay observations."""
import importlib.util
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[2]


def replay():
    spec = importlib.util.spec_from_file_location(
        'replay_contract_fixture', ROOT / 'scripts/migration/replay_rollback.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_complete_replay_comparison_ignores_only_object_key_order():
    module = replay()
    expected = {'id': 'real-generated-id', 'nested': {'enabled': True, 'count': 1},
                'items': ['first', 'second'], 'secret': 'synthetic'}
    module.require_equal({'secret': 'synthetic', 'items': ['first', 'second'],
                          'nested': {'count': 1, 'enabled': True},
                          'id': 'real-generated-id'}, expected, 'fixture')
    for observed in (
        {**expected, 'id': 'different-id'},
        {**expected, 'nested': {'enabled': 1, 'count': 1}},
        {**expected, 'nested': {'enabled': True, 'count': 1.0}},
        {**expected, 'items': ['second', 'first']},
        {**expected, 'secret': None},
        {key: value for key, value in expected.items() if key != 'nested'},
        {**expected, 'unexpected': True},
    ):
        with pytest.raises(AssertionError, match='Cross-runtime fixture differs'):
            module.require_equal(observed, expected, 'fixture')


def test_replay_comparison_refuses_nonfinite_values_without_printing_payloads():
    module = replay()
    for value in (float('nan'), float('inf'), -float('inf')):
        with pytest.raises(ValueError):
            module.require_equal({'number': value}, {'number': value}, 'fixture')
    with pytest.raises(AssertionError) as failure:
        module.require_equal({'password': 'synthetic-secret'}, {}, 'fixture')
    assert 'synthetic-secret' not in str(failure.value)
