"""Historical failure proof rejects unrelated failures and restores fixed source."""
import ast
import hashlib
import importlib.util
import json
import logging
from pathlib import Path
from types import SimpleNamespace
import sys

import pytest

ROOT = Path(__file__).resolve().parents[2]


@pytest.mark.parametrize('damage', [None, 'missing-relay', 'missing-receipt', 'unmapped-target'])
def test_oracle_verification_requires_complete_native_target_registration(monkeypatch, damage):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    inventory = json.loads((ROOT / 'docs/migration-coverage.json').read_text(encoding='utf-8'))
    reached = []

    class OracleReached(Exception):
        pass

    def document(seam):
        reached.append(seam)
        raise OracleReached

    from unittest.mock import patch
    with patch.dict(sys.modules, {
        'inventory': SimpleNamespace(discover=lambda: inventory),
        'oracle': SimpleNamespace(document=document),
    }):
        spec = importlib.util.spec_from_file_location(
            'registered_oracles_fixture', ROOT / 'scripts/migration/verify_oracles.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    directory = ROOT / 'engine-rust/tests'
    targets = list(directory.glob('*.rs'))
    assert directory / 'relay_request_parity.rs' in targets
    if damage == 'missing-relay':
        targets.remove(directory / 'relay_request_parity.rs')
    elif damage == 'missing-receipt':
        targets.remove(directory / 'ownership_receipt_parity.rs')
    elif damage == 'unmapped-target':
        targets.append(directory / 'unmapped_fixture.rs')
    glob = Path.glob
    monkeypatch.setattr(Path, 'glob', lambda path, pattern: iter(targets)
                        if path == directory and pattern == '*.rs' else glob(path, pattern))
    with pytest.raises(ValueError if damage else OracleReached,
                       match='integration suite discovery' if damage else None):
        module.verify()
    assert reached == ([] if damage else ['models'])


def load():
    spec = importlib.util.spec_from_file_location(
        'native_regressions_fixture', ROOT / 'scripts/ci/native_regressions.py')
    module = importlib.util.module_from_spec(spec)
    helper_spec = importlib.util.spec_from_file_location(
        'native_settings_history', ROOT / 'scripts/ci/native_settings_history.py')
    helper = importlib.util.module_from_spec(helper_spec)
    helper_spec.loader.exec_module(helper)
    from unittest.mock import patch

    cases_spec = importlib.util.spec_from_file_location(
        'native_regression_cases', ROOT / 'scripts/ci/native_regression_cases.py')
    cases = importlib.util.module_from_spec(cases_spec)
    cases_spec.loader.exec_module(cases)
    current_spec = importlib.util.spec_from_file_location(
        'native_current_history', ROOT / 'scripts/ci/native_current_history.py')
    current = importlib.util.module_from_spec(current_spec)
    current_spec.loader.exec_module(current)
    with patch.dict(sys.modules, {'native_settings_history': helper,
                                 'native_current_history': current,
                                 'native_regression_cases': cases}):
        spec.loader.exec_module(module)
    return module


def test_option_reader_companions_preserve_selected_defects_and_prior_snapshots():
    module = load()
    current = {(case['target'], case['test'], case['commit']): case for case in module._STORAGE_REGRESSIONS}
    original_tree = ast.parse(
        (ROOT / 'scripts/ci/native_regression_cases.py').read_text(encoding='utf-8'))
    first_registry = next(node.value for node in original_tree.body
                          if isinstance(node, ast.Assign)
                          and any(isinstance(target, ast.Name)
                                  and target.id == 'REGRESSIONS' for target in node.targets))
    original = ast.literal_eval(first_registry)
    assert len(module._STORAGE_REGRESSIONS) == len(original) + 1
    grouped = 0
    for before in original:
        after = current[(before['target'], before['test'], before['commit'])]
        assert {key: after[key] for key in before if key != 'companions'} == {
            key: value for key, value in before.items() if key != 'companions'}
        for name, source in before.get('companions', {}).items():
            assert after['companions'][name] == source
        touched = set(before['files']) | set(before.get('companions', {}))
        if touched & module._STORAGE_READERS.keys():
            grouped += 1
            assert set(module._STORAGE_READERS) <= set(after['files']) | set(after['companions'])
            assert not set(after['files']) & set(after['companions'])
        else:
            assert after == before
    assert grouped == 9
    added = module._STORAGE_REGRESSIONS[-1]
    assert added['test'] == 'settings_errors::storage_settings_errors_match_original_lazy_reads_and_preserve_bytes'
    assert added['files'] == module._STORAGE_READERS
    assert added['commit'] == module._STORAGE_READERS_COMMIT
    assert added['symptom'] == 'Storage settings admission differs'


def test_settings_signature_companions_preserve_all_existing_proofs():
    module = load()
    history = module.native_settings_history
    previous = module._STORAGE_REGRESSIONS
    assert len(module._SETTINGS_REGRESSIONS) == len(previous) + 2
    for before, after in zip(previous, module._SETTINGS_REGRESSIONS):
        assert {key: after[key] for key in before if key != 'companions'} == {
            key: value for key, value in before.items() if key != 'companions'}
        for name, source in before.get('companions', {}).items():
            assert after['companions'][name] == source
        touched = before['files'].keys() | before.get('companions', {}).keys()
        if touched & (history.SOURCES.keys() | module._STORAGE_READERS.keys()):
            assert history.SOURCES.keys() <= after['files'].keys() | after['companions'].keys()
            assert not after['files'].keys() & after['companions'].keys()
            for name, expected in history.SOURCES.items():
                if name not in touched:
                    assert after['companions'][name] == {'commit': history.COMMIT, 'sha256': expected}
        else:
            assert after == before
    added = module._SETTINGS_REGRESSIONS[-2]
    assert added['target'] == 'cli_settings_parity'
    assert added['test'] == 'strict_settings::settings_commands_preserve_original_decoding_refusals_and_early_errors'
    assert added['symptom'] == 'Strict settings CLI admission differs'
    assert added['commit'] == history.COMMIT
    assert added['files'] == history.SOURCES
    extension = module._SETTINGS_REGRESSIONS[-1]
    assert extension == {
        'target': 'config_parity',
        'test': 'extension_admission::extension_setter_preserves_original_stored_set_admission_and_cache',
        'symptom': 'Stored extension set admission differs',
        'commit': 'b8e589f28e57e5734be2bacbd000bb11cc454ba2',
        'files': {'engine-rust/src/storage/config.rs':
                  '2c037f3fcb63a1d210c84437a72285a371a9c1fe89d85d510fe3c649142c406f'},
    }
    assert not extension['files'].keys() & extension.get('companions', {}).keys()


def test_extension_history_follows_settings_coherence_without_contaminating_other_proofs():
    module = load()
    expected = {'commit': 'edca9d09805ccdc2d29735c8977a4354c71223b7',
                'sha256': 'd6624367937929bd0e719243925d12a94787382525c01c0717e58e916dea2ac2'}
    route = 'engine-rust/src/http/extension_routes.rs'
    storage = 'engine-rust/src/storage/extensions.rs'
    selected = {
        'extension_path_whitespace_matches_original_validation':
            'e9ddc94cd7db3cb867f9c118a94a858592786ad02eea92a517b9dbadc3b78f6f',
        'malformed_extension_text_matches_original_refusal_without_changing_bytes':
            '46636571378a748deee180593862b2005ce6a6f7d7439f35a33ae0e97768ef69',
        'extension_icon_integer_ranking_matches_original_selection':
            '640b01c025fe2df3f58c927f05a66042dbf4eb8af5e492cc0ffd616b6afce570',
    }
    corrected = []
    for before, after in zip(module._SETTINGS_REGRESSIONS, module._EXTENSION_REGRESSIONS):
        if before['test'] not in selected:
            assert after == before
            continue
        corrected.append(before['test'])
        assert before['files'] == {storage: selected[before['test']]}
        assert not before.get('companions')
        assert {key: after[key] for key in before} == before
        assert after['companions'] == {route: expected}
    assert set(corrected) == selected.keys() and len(corrected) == 3
    assert module.REGRESSIONS == module._EXTENSION_REGRESSIONS + module.CURRENT_CALLER_REGRESSIONS


def test_schedule_history_preserves_text_failure_without_later_api_callers():
    module = load()
    cases = [case for case in module.REGRESSIONS if case['target'] == 'schedule_store_parity']
    assert len(cases) == 1
    case = cases[0]
    assert case['test'] == 'schedule_text_admission_matches_original_without_replacing_bytes'
    assert case['symptom'] == 'Schedule text admission differs'
    assert case['commit'] == 'b9d7399732c140d6141f37633486fffd90a2e30a'
    assert case['files'] == {
        'engine-rust/src/storage/schedules.rs':
            '749856bedd3df0be79cbd3853014fc75d24bb8b9320dcc9a09c32747ccf47527',
    }
    assert case['companions'] == {
        'engine-rust/tests/schedule_store_parity.rs': {
            'commit': '93db8f8b93d8b8e961bc548a3174da560c16c0b3',
            'sha256': '0419aa623eb9ec63e9d442af7e683756e7d4e04436c56ef0537691c6386504a4',
        },
        'engine-rust/src/cli/schedule_commands.rs': {
            'commit': '93db8f8b93d8b8e961bc548a3174da560c16c0b3',
            'sha256': '8ecf3e0a949f9eedef112c9c49b7661468d167072dc7230fdeb6ff9ae7f72b52',
        },
    }
    assert not case['files'].keys() & case['companions'].keys()


def test_pool_history_keeps_a_compatible_importer_without_replacing_the_selected_defect():
    module = load()
    cases = [case for case in module.REGRESSIONS if case['target'] == 'pool_parse_parity']
    assert len(cases) == 1
    case = cases[0]
    assert case['test'] == 'pooled_proxy_text_boundaries_match_original_full_results'
    assert case['symptom'] == 'Pooled proxy text boundaries differ'
    assert case['commit'] == 'ef6ebe26ba183fbbd9892d077d70fdcd8f7f7c64'
    assert case['files']['engine-rust/src/pool.rs'] == '5381266449dafc5eca9009fe1d900c3aed760da00a3855b8d122dc58772bb5ab'
    assert case['files']['engine-rust/src/pool_normalize.rs'] == 'f97331c9da3246f9ad993df1cacfe80f9d59d5494fd6db4c9c8cd26b05cd17e7'
    assert case['companions']['engine-rust/src/importers.rs'] == {
        'commit': '4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1',
        'sha256': 'e04030f2743e3f77625e0428c28c5727aabf39091e90aa01351775d7a0d16572',
    }
    assert case['companions']['engine-rust/src/browser/options.rs'] == {
        'commit': '461eebcb6b580d8b399dde00e189b510c650f6f4',
        'sha256': '48f97356e1d6232fa5a84ab619d8bb7f7d9fc25f6b7f7623447339f5de488263',
    }
    assert case['companions']['engine-rust/src/network/proxy_check.rs'] == {
        'commit': '4e570f130ba807448453c12777457bcf55911a67',
        'sha256': '39d4c4f728c91ef9ad342ba4663ae73c3499651b985f07d558faf34ba04424d5',
    }
    assert case['companions']['engine-rust/src/mcp.rs'] == {
        'commit': '8f6ad94e95693d2c3ff18d2ca39ec8effa635cd6',
        'sha256': '802ba1d69ce81d705cb501970df1230b9fd2b733409381b0b7632afaacdebca6',
    }
    assert case['companions']['engine-rust/src/mcp_command.rs'] == {
        'commit': 'db29928b16d08810ce88243f0dd1a6e233e2dbf6',
        'sha256': '212a821995a0582c0f07913ec1ac9c83172b0fcb1024ccfc1eb3fe9eb95e7dd1',
    }
    assert case['companions']['engine-rust/src/browser/network_grades.rs'] == {
        'commit': 'f409196e68c86a2bb3e3a2b136217e1d2f276c62',
        'sha256': '5583d5df14afb382ec177587e2ac54b9553beb4c3b49e6d6b89654adcb52fe71',
    }
    assert case['companions']['engine-rust/src/cli/output.rs'] == {
        'commit': '5501361f815916c1a2bd47e7e342de5117b4e4ff',
        'sha256': 'aab0acee9ecf439ddf252a983878e908b362eebeb54901e6e892d205b2b55ac6',
    }
    assert case['companions']['engine-rust/src/native_entry.rs'] == {
        'commit': '5501361f815916c1a2bd47e7e342de5117b4e4ff',
        'sha256': '563c0a9ec513d92db641075382f97b52fa0c014ec15cfb3a3da602982290a98d',
    }
    assert not case['files'].keys() & case['companions'].keys()


def test_scheduler_row_history_requires_the_original_isolation_failure():
    module = load()
    cases = [case for case in module.REGRESSIONS
             if case['test'] == 'scheduler_rows::skipped_malformed_schedule_ids_do_not_abort_later_occurrences']
    assert len(cases) == 1
    assert cases[0] == {
        'target': 'launch_http_parity',
        'test': 'scheduler_rows::skipped_malformed_schedule_ids_do_not_abort_later_occurrences',
        'symptom': 'Scheduler row isolation differs',
        'commit': '7d8108a31551330c34a7d183bb04335422b0bcb0',
        'files': {'engine-rust/src/http/scheduler.rs':
                  '3e0e0e7fc208acc286c5006d15d172658df9e08078192a19c7c42e62c39d4a0c'},
    }


def test_ascii_refusal_history_requires_the_shared_representation_failure():
    module = load()
    cases = [case for case in module.REGRESSIONS
             if case['test'] == 'ascii_refusal_representation_matches_original_without_exposing_secret_values']
    assert cases == [{
        'target': 'validation_parity',
        'test': 'ascii_refusal_representation_matches_original_without_exposing_secret_values',
        'symptom': 'ASCII refusal representation differs',
        'commit': '65f5dfd15f2a4b6cc7fd375e858a76b153c83862',
        'files': {'engine-rust/src/validation.rs':
                  'd505c76357437baee6a8ef76d7626f099adbf59b8f2829cd8b4dfc73c1e8d4e6'},
    }]


def test_legacy_read_history_requires_the_existing_http_assertion():
    module = load()
    cases = [case for case in module.REGRESSIONS
             if case['test'] == 'read_view::legacy_read_views_match_original_core_http_and_name_admission']
    assert cases == [{
        'target': 'profile_read_parity',
        'test': 'read_view::legacy_read_views_match_original_core_http_and_name_admission',
        'symptom': 'Legacy HTTP read view differs',
        'commit': '65f5dfd15f2a4b6cc7fd375e858a76b153c83862',
        'files': {'engine-rust/src/http/profile_routes.rs':
                  '9c3fcf9caf0bc6fa97ba48b62341b527fb1bc6f017e3859720a257b8d88bd202'},
    }]


def test_current_caller_history_is_appended_after_compatibility_overlays():
    module = load()
    expected = ({
        'target': 'repo_batch_parity',
        'test': 'complete_batch_refusals_match_original_index_order_and_preserved_bytes',
        'symptom': 'Complete indexed batch refusal differs',
        'commit': 'd0d81cc6c173c302520f107061f53b57ff0ce885',
        'files': {'engine-rust/src/storage/repo_write.rs':
                  'ce4d904503fd52f828896f9abd840e76aee1e0131f292064fd99a875a864c53f'},
    }, {
        'target': 'profile_restore_parity',
        'test': 'restore_copy_candidates_match_original_existing_name_ownership',
        'symptom': 'Restore candidate ownership differs',
        'commit': '5f356644fa7a132862b002bebf8222931bccc806',
        'files': {'engine-rust/src/storage/restore.rs':
                  '85987e107f9aa70c6a3f17fea325eaac4d67309d9a2fe446b288a6ca6be1aa1a'},
    }, {
        'target': 'relay_request_parity',
        'test': 'socks_origin_requests_preserve_original_target_bytes',
        'symptom': 'SOCKS origin request bytes differ',
        'commit': '60ffc489916e7a2560229fcc5a7be5b813196e72',
        'files': {'engine-rust/src/network/relay_http.rs':
                  '830ac7683ae157810cb5f2589f0e253cc6c09661b3204f6785b9417f2ca738dd'},
    }, {
        'target': 'native_entry',
        'test': 'browser_dispatch_uses_actual_handshake_without_redundant_metadata',
        'symptom': 'Browser dispatch repeated SDK metadata preflight',
        'commit': '2c6d23eed78cdf1f3ddf2c41c6e103d987cfe527',
        'files': {'engine-rust/src/native_entry.rs':
                  'abf6a5e9b6690231795845589d28c2ddb7faded39d65fa635afd3919547837e1'},
    }, {
        'target': 'relay_request_parity',
        'test': 'handshake::socks_handshake_failures_preserve_original_diagnostics_and_wire',
        'symptom': 'SOCKS handshake diagnostic differs',
        'commit': 'a45e4ad46b804b19423dde4967017b0087a15598',
        'files': {'engine-rust/src/network/relay_protocol.rs':
                  'dfc78f467205732f15b0b200aa074729706766002fb3923588856bca4784c489'},
    })
    assert module.CURRENT_CALLER_REGRESSIONS[0] == {
        'target': 'mcp_parity',
        'test': 'stdio_text_boundaries_match_original_responses',
        'symptom': 'MCP stdio text response differs',
        'commit': '8f6ad94e95693d2c3ff18d2ca39ec8effa635cd6',
        'files': {'engine-rust/src/mcp.rs':
                  '802ba1d69ce81d705cb501970df1230b9fd2b733409381b0b7632afaacdebca6'},
    }
    assert module.CURRENT_CALLER_REGRESSIONS[1] == {
        'target': 'lib',
        'test': 'network::tests::proxy_echo_boundaries_match_original_through_transport',
        'symptom': 'Proxy echo response differs',
        'commit': '4e570f130ba807448453c12777457bcf55911a67',
        'files': {'engine-rust/src/network/proxy_check.rs':
                  '39d4c4f728c91ef9ad342ba4663ae73c3499651b985f07d558faf34ba04424d5'},
    }
    assert module.CURRENT_CALLER_REGRESSIONS[2] == {
        'target': 'browser_options_parity',
        'test': 'dns_options_match_original_whitespace_and_refusals',
        'symptom': 'DNS launch options differ',
        'commit': '461eebcb6b580d8b399dde00e189b510c650f6f4',
        'files': {'engine-rust/src/browser/options.rs':
                  '48f97356e1d6232fa5a84ab619d8bb7f7d9fc25f6b7f7623447339f5de488263'},
    }
    assert module.CURRENT_CALLER_REGRESSIONS[3] == {
        'target': 'bulk_parity',
        'test': 'selection::selection_matches_original_complete_profile_membership',
        'symptom': 'Bulk profile selection differs',
        'commit': 'd7227856f1fc7bec01b66d3737d3cbb729115623',
        'files': {'engine-rust/src/bulk.rs':
                  'e425d6fd6a60615996504fb5c58758a895bc574126609650f9fafeee62938d09'},
    }
    importer = module.CURRENT_CALLER_REGRESSIONS[4:6]
    assert [(case['target'], case['test'], case['symptom']) for case in importer] == [
        ('profile_http_import_parity', 'tags::importer_tags_match_original_whitespace_and_group_display', 'Imported tag display differs'),
        ('profile_http_import_parity', 'tags::importer_refusals_retain_all_original_ordered_problems', 'Complete importer refusal differs'),
    ]
    assert all(case['commit'] == '965ba02e5eceacb7c0be4e2d9a4e873b52279610' and case['files'] == {
        'engine-rust/src/importers.rs': 'e4526b6a17983cc5a15fe89cd1d3de28dc60b0fcccb57a078a8a5195e3355751',
    } for case in importer)
    jobs = module.CURRENT_CALLER_REGRESSIONS[6]
    assert jobs == {
        'target': 'job_parity',
        'test': 'text::job_text_admission_matches_original_without_replacing_identity_or_outcome',
        'symptom': 'Job text admission differs',
        'commit': '51a627bd81ad3b7d6b130e85fa45c7354e5d3637',
        'files': {'engine-rust/src/jobs.rs':
                  '643bcc66a0d2750dd530abdd91e9a59c24d97c9e3afb39897f07160b18b3977a'},
    }
    claim = module.CURRENT_CALLER_REGRESSIONS[7]
    assert claim == {
        'target': 'claim_parity',
        'test': 'diagnostics::claim_diagnostics_match_original_truth_and_display_without_changing_records',
        'symptom': 'Claim diagnostic display differs',
        'commit': 'd25e088d8adb4c70e2dd401ffbe100331cfb35f4',
        'files': {'engine-rust/src/ownership/claims.rs':
                  '6af8a9ffd4a29f9ce29da25da9ecf8e4b4fc2031b9228424be8c578650eccbb8'},
    }
    added = module.CURRENT_CALLER_REGRESSIONS[8:10]
    assert [(case['target'], case['test'], case['symptom']) for case in added] == [
        ('profile_http_import_parity', 'lexical::importer_lexical_profiles_match_original_without_changing_inputs', 'Imported lexical dashboard differs'),
        ('profile_http_import_parity', 'lexical::native_envelope_versions_match_original_exact_numeric_guidance', 'Native export version guidance differs'),
    ]
    assert all(case['commit'] == '4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1' for case in added)
    assert added[0]['files'] == {'engine-rust/src/importers.rs': 'e04030f2743e3f77625e0428c28c5727aabf39091e90aa01351775d7a0d16572'}
    assert added[1]['files'] == {'engine-rust/src/import_format.rs': 'fb386692f47d6e7d0281b302112001f6af120418d0bbf5d765c231243de51233'}
    assert module.CURRENT_CALLER_REGRESSIONS[10] == {
        'target': 'mcp_parity',
        'test': 'captured_command_errors_match_original_stream_selection',
        'symptom': 'MCP captured result differs',
        'commit': 'db29928b16d08810ce88243f0dd1a6e233e2dbf6',
        'files': {'engine-rust/src/mcp_command.rs':
                  '212a821995a0582c0f07913ec1ac9c83172b0fcb1024ccfc1eb3fe9eb95e7dd1'},
    }
    assert module.CURRENT_CALLER_REGRESSIONS[11] == {
        'target': 'browser_ip_parity',
        'test': 'timezone_grades_match_original_proxy_and_trust_boundaries',
        'symptom': 'Timezone proxy grade differs',
        'commit': 'f409196e68c86a2bb3e3a2b136217e1d2f276c62',
        'files': {'engine-rust/src/browser/network_grades.rs':
                  '5583d5df14afb382ec177587e2ac54b9553beb4c3b49e6d6b89654adcb52fe71'},
    }
    assert module.CURRENT_CALLER_REGRESSIONS[12] == {
        'target': 'cli_cookie_parity',
        'test': 'alignment_versions::alignment_versions_match_original_decimal_captures',
        'symptom': 'Alignment version fingerprint differs',
        'commit': '8e94027cf9671b343528fb29a79375187f42b7ba',
        'files': {'engine-rust/src/browser/alignment.rs':
                  '327731362383aea951c7ed3fc75c2fb1d02017c7f8ed2b1ac25e5315dfaddbaf'},
    }
    assert module.CURRENT_CALLER_REGRESSIONS[13] == {
        'target': 'cli_registry_parity',
        'test': 'command_registry_matches_original_schemas_arguments_and_parse_views',
        'symptom': 'CLI confirmation decision differs',
        'commit': '5501361f815916c1a2bd47e7e342de5117b4e4ff',
        'files': {'engine-rust/src/cli/output.rs':
                  'aab0acee9ecf439ddf252a983878e908b362eebeb54901e6e892d205b2b55ac6'},
    }
    assert module.CURRENT_CALLER_REGRESSIONS[14:19] == expected
    assert module.CURRENT_CALLER_REGRESSIONS[19:] == ({
        'target': 'extension_routes_parity',
        'test': 'icons::extension_icon_http_matches_original_known_folder_and_file_contracts',
        'symptom': 'Extension icon HTTP status differs',
        'commit': 'edca9d09805ccdc2d29735c8977a4354c71223b7',
        'files': {'engine-rust/src/http/extension_routes.rs':
                  'd6624367937929bd0e719243925d12a94787382525c01c0717e58e916dea2ac2'},
    }, {
        'target': 'extension_routes_parity',
        'test': 'icons::extension_icon_http_matches_original_known_folder_and_file_contracts',
        'symptom': 'Extension icon HTTP status differs',
        'commit': '8354eeb286d93f9418ee372ed057c129521404ad',
        'platform': 'win32',
        'files': {'engine-rust/src/storage/locks.rs':
                  '8cc9b17a4c77c5db9b178d8ea2b16318132f5d11439359c3e47abca269d6a373'},
    }, {
        'target': 'profile_read_parity',
        'test': 'divergence::divergent_profile_http_matches_original_refusal_kind_without_mutation',
        'symptom': 'Divergent HTTP response kind differs',
        'commit': 'edca9d09805ccdc2d29735c8977a4354c71223b7',
        'files': {'engine-rust/src/http/profile_errors.rs':
                  '11c6e0df12446f6f5b5669b32c00ac6b832a574c18b89c2f3efa24b2509740a9'},
    }, {
        'target': 'ownership_receipt_parity',
        'test': 'receipt_tokens_match_original_without_resolving_invalid_transfers',
        'symptom': 'Receipt completeness differs',
        'commit': 'b1ad79f18f75d9a328b731bb092130cc3b61541a',
        'files': {'engine-rust/src/ownership/unit_report.rs':
                  '0db789c9ab6f184f811bfadd0f1d3cea7ab0b0bd058663eecbffd816a7bbadcb'},
    }, {
        'target': 'store_open_parity',
        'test': 'denied_store_directories_match_original_actionable_errors_and_preserve_state',
        'symptom': 'Store directory permission diagnostic differs',
        'commit': 'a640a8317180d23e7064e074550bb1a03c432dca',
        'files': {'engine-rust/src/storage/repo.rs':
                  '3c7f327011f3cba1b54b0934bdcc70c1ca5f1731f5ae5113e9cb645900af256d'},
    }, {
        'target': 'repo_lookup_parity',
        'test': 'read_warnings::read_repair_warnings_match_original_without_changing_read_results',
        'symptom': 'Read repair warning differs',
        'commit': '017f0dcac603bdbbcf56ea80b1156fca23579f9a',
        'files': {'engine-rust/src/storage/repo_lookup.rs':
                  '8cb44510b66c7c21074cfb168aea8ea3f22d8aa6cc255ea8be70e7a44cde5e32'},
    }, {
        'target': 'lib',
        'test': 'ownership::tree::tests::malformed_group_observations_match_original_without_releasing_claims',
        'symptom': 'Process group uncertainty differs',
        'commit': 'c73bc1abab22788936227e5e2c800dac43acd69e',
        'platform': 'linux',
        'files': {'engine-rust/src/ownership/tree.rs':
                  '2688b722ab3411a1a5528335a69b029a97e840cb79f7cb065d55735cefe6078b'},
    }, {
        'target': 'native_entry',
        'test': 'entry_logs_follow_original_data_root_and_preserve_explicit_override',
        'symptom': 'Entry log destination differs',
        'commit': '5bd0aca5731e5bd1b5f3a53e2e45c8d58823c756',
        'files': {
            'engine-rust/src/native_entry.rs':
                '2c785a4d4171742e69e9b970960622d55cbfeb0a9eefa65f3f0b5efafa5ec3eb',
            'engine-rust/src/native_detach.rs':
                '194e25cf92609c836d5a3ae1e53d2e6942145842449c50175557470d24e09350',
        },
    }, {
        'target': 'lib',
        'test': 'storage::repo_delete::tests::failed_delete_rollback_preserves_causes_and_actual_recovery_paths',
        'symptom': 'Delete rollback recovery paths differ',
        'commit': 'c5b466add5c22ca23b53ce2c7ff4ac55093c3ba0',
        'files': {'engine-rust/src/storage/repo_delete.rs':
                  '9be0a936e39e8ee56c3ac183af13d760fa067c7ae02c6412b28e2e18a199d7dd'},
    }, {
        'target': 'logging_parity',
        'test': 'server_scheduler_errors_reach_the_runtime_file_without_starting_an_sdk',
        'symptom': 'Scheduler error runtime log missing',
        'commit': '09da16a74b2d12d86dfe291148cc431ba39b0e35',
        'files': {'engine-rust/src/http/server.rs':
                  '7a3f0426584cde037a661abe397344a4d859eb743bef4629b3dc96a20eaf8894'},
    })
    assert module.REGRESSIONS == module._EXTENSION_REGRESSIONS + module.CURRENT_CALLER_REGRESSIONS
    assert not any(case.get('companions') for case in module.CURRENT_CALLER_REGRESSIONS)


@pytest.mark.parametrize('damage', [None, 'exit', 'empty', 'name', 'symptom', 'summary'])
def test_failure_proof_requires_the_exact_production_assertion(damage):
    module = load()
    case = module.REGRESSIONS[0]
    evidence = module.FailureEvidence(case)
    messages = {
        'empty': 'running 1 test',
        'name': f"test {case['test']} ... FAILED",
        'symptom': f"assertion `left == right` failed: {case['symptom']}",
        'summary': 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                   '1 filtered out; finished in 0.01s',
    }
    try:
        for key, text in messages.items():
            if key != damage:
                evidence.emit(logging.makeLogRecord({'msg': text}))
        if damage:
            with pytest.raises(ValueError, match='required production assertion'):
                evidence.verify(0 if damage == 'exit' else 101)
        else:
            evidence.verify(101)
    finally:
        evidence.close()


@pytest.mark.parametrize('damage', [None, 'local', 'repository', 'sha', 'head', 'dirty'])
def test_historical_execution_requires_the_exact_clean_hosted_source(monkeypatch, damage):
    import subprocess

    module = load()
    sha = 'a' * 40
    monkeypatch.setenv('GITHUB_ACTIONS', 'false' if damage == 'local' else 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'other/repository' if damage == 'repository'
                       else 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('GITHUB_SHA', 'invalid' if damage == 'sha' else sha)
    calls = []

    def git(root, *arguments):
        calls.append(arguments)
        if arguments == ('rev-parse', 'HEAD'):
            return (('b' * 40 if damage == 'head' else sha) + '\n').encode('ascii')
        assert arguments[:3] == ('diff', '--exit-code', 'HEAD')
        if damage == 'dirty':
            raise subprocess.CalledProcessError(1, ['git', 'diff'])
        return b''

    monkeypatch.setattr(module, 'git', git)
    if damage:
        with pytest.raises((ValueError, subprocess.CalledProcessError)):
            module.require_hosted_source(ROOT)
    else:
        assert module.require_hosted_source(ROOT) == sha
        assert len(calls) == 2


@pytest.mark.parametrize('platform', ['win32', 'linux'])
@pytest.mark.parametrize('damage', [None, 'warm', 'launch', 'assertion', 'integrity', 'companion'])
@pytest.mark.parametrize('incremental', [None, '0', '1'])
@pytest.mark.parametrize('target', ['model_parity', 'lib'])
def test_historical_runner_restores_source_and_environment(
        tmp_path, monkeypatch, damage, platform, incremental, target):
    module = load()
    source = tmp_path / 'engine-rust/src/models.rs'
    source.parent.mkdir(parents=True)
    current, historical = b'fixed production bytes\n', b'historical production bytes\n'
    source.write_bytes(current)
    companion = tmp_path / 'engine-rust/tests/config_parity.rs'
    companion.parent.mkdir(parents=True)
    fixed_test, old_test = b'fixed test bytes\n', b'original assertion bytes\n'
    companion.write_bytes(fixed_test)
    case = dict(module.REGRESSIONS[0], target=target, files={
        'engine-rust/src/models.rs': hashlib.sha256(historical).hexdigest()},
        companions={'engine-rust/tests/config_parity.rs': {
            'commit': 'companion-commit', 'sha256': hashlib.sha256(old_test).hexdigest()}})
    excluded = dict(case, platform='linux' if platform == 'win32' else 'win32',
                    commit='excluded-source-must-not-be-read')
    selected = dict(case, platform=platform)
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform=platform))
    monkeypatch.setattr(module, 'REGRESSIONS',
                        (excluded, selected, case) if damage is None else (excluded, selected))
    sha = 'a' * 40
    monkeypatch.setenv('GITHUB_ACTIONS', 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('GITHUB_SHA', sha)
    monkeypatch.setenv('NUMERA_ORACLE_SOURCE_ROOT', 'prior-original-root')
    monkeypatch.delenv('PYTHONDONTWRITEBYTECODE', raising=False)
    if incremental is None:
        monkeypatch.delenv('CARGO_INCREMENTAL', raising=False)
    else:
        monkeypatch.setenv('CARGO_INCREMENTAL', incremental)
    calls, warmups, blob_reads = [], [], []

    def git(root, *arguments):
        if arguments == ('rev-parse', 'HEAD'):
            return sha.encode('ascii')
        if arguments[0] == 'diff':
            return b''
        blob_reads.append(arguments)
        if arguments == ('show', 'companion-commit:engine-rust/tests/config_parity.rs'):
            return old_test + (b'changed' if damage == 'companion' else b'')
        assert arguments == ('show', f"{case['commit']}:engine-rust/src/models.rs")
        return historical + (b'changed' if damage == 'integrity' else b'')

    def run(command, wall, idle, logger):
        import os

        calls.append(command)
        assert source.read_bytes() == historical
        assert companion.read_bytes() == old_test
        assert (wall, idle) == (180, 90)
        assert command[-4:] == [case['test'], '--exact', '--test-threads=1', '--color=never']
        assert '--locked' in command and '--offline' in command
        assert command[command.index('--config') + 1] == 'profile.dev.package.numera-core.opt-level=0'
        override = (['--config', 'profile.dev.package.numera-core.codegen-units=16']
                    if platform == 'win32' else [])
        assert command.count('--config') == (2 if override else 1)
        selection = ['--lib'] if target == 'lib' else ['--test', target]
        assert command[command.index('--config') + 2:command.index('--')] == ['--timings', *override, *selection]
        assert os.environ['NUMERA_ORACLE_SOURCE_ROOT'] == str(tmp_path / 'original')
        assert os.environ['PYTHONDONTWRITEBYTECODE'] == '1'
        assert os.environ['CARGO_INCREMENTAL'] == '1'
        if damage == 'launch':
            raise TimeoutError('fixture runner timeout')
        logger.info('running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', 'unrelated failure' if damage == 'assertion' else case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 101

    def warmup(command, **options):
        import os

        assert os.environ['CARGO_INCREMENTAL'] == '1'
        assert source.read_bytes() == historical
        assert companion.read_bytes() == old_test
        override = (['--config', 'profile.dev.package.numera-core.codegen-units=16']
                    if platform == 'win32' else [])
        assert command == ['cargo', 'test', '--manifest-path', str(tmp_path / 'engine-rust/Cargo.toml'),
                           '--locked', '--offline', '--config',
                           'profile.dev.package.numera-core.opt-level=0', '--timings', *override,
                           *(['--lib'] if target == 'lib' else ['--test', target]), '--no-run']
        assert options == {'stdin': module.subprocess.DEVNULL, 'check': True}
        warmups.append(command)
        if damage == 'warm':
            raise module.subprocess.CalledProcessError(1, command)
        return SimpleNamespace(returncode=0)

    monkeypatch.setattr(module, 'git', git)
    monkeypatch.setattr(module.subprocess, 'run', warmup)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    monkeypatch.setitem(sys.modules, 'baseline_source',
                        SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    logger = logging.Logger('native-regression-fixture', level=logging.INFO)
    if damage:
        with pytest.raises((ValueError, TimeoutError, module.subprocess.CalledProcessError)):
            module.prove(tmp_path, tmp_path / 'original', logger)
    else:
        module.prove(tmp_path, tmp_path / 'original', logger)
    import os

    assert source.read_bytes() == current
    assert companion.read_bytes() == fixed_test
    assert os.environ['NUMERA_ORACLE_SOURCE_ROOT'] == 'prior-original-root'
    assert 'PYTHONDONTWRITEBYTECODE' not in os.environ
    assert os.environ.get('CARGO_INCREMENTAL') == incremental
    assert logger.handlers == []
    assert len(calls) == (2 if damage is None else 0 if damage in ('integrity', 'companion', 'warm') else 1)
    assert len(warmups) == (0 if damage in ('integrity', 'companion') else 1)
    assert len(blob_reads) == (1 if damage == 'integrity' else 2)
