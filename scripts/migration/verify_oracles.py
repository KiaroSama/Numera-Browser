"""Regenerate recorded source expectations and fail on behavioral fixture drift."""
from __future__ import annotations

import json
from pathlib import Path
from inventory import discover
from oracle import document
from run_logging import run_log

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'engine-rust' / 'tests' / 'fixtures'


def verify() -> None:
    inventory = json.loads((ROOT / 'docs' / 'migration-coverage.json').read_text(encoding='utf-8'))
    current = discover()
    # The original application and test inventory are frozen provenance. Current
    # tests are discovered independently by the hosted suite coverage guard.
    source_files = lambda rows: [row for row in rows if row['path'].startswith('engine/numera/')]
    if source_files(inventory['files']) != source_files(current['files']) or inventory['source_commit'] != current['source_commit']:
        raise ValueError('Source application contract inventory drift')
    discovered_tests = {path.stem for path in (ROOT / 'engine-rust' / 'tests').glob('*.rs')}
    expected_tests = {'model_parity','generation_parity','identity_parity','validation_parity',
                      'vault_parity','conversion_parity','journal_parity','storage_parity','auth_policy_parity','lock_interop_parity','path_parity','config_parity','secret_policy_parity','repo_parity','completion_parity','repo_write_parity','vault_files_parity','recovery_parity','ownership_identity_parity','ownership_tree_parity','ownership_receipt_parity','claim_parity','vault_enable_parity','vault_rotate_parity','rotation_recovery_parity','folder_parity','repo_batch_parity','repo_lookup_parity','repo_delete_parity','data_root_parity','store_open_parity','first_run_parity','pool_parse_parity','pool_normalize_parity','pool_store_parity','pool_vault_parity','profile_export_parity','profile_import_parity','profile_backup_parity','profile_restore_parity','extension_resources_parity','extension_ids_parity','extension_packages_parity','extension_install_parity','token_store_parity','token_create_parity','request_auth_parity','dashboard_store_parity','dashboard_vault_parity','dashboard_batch_parity','dashboard_meta_parity','dashboard_revision_parity','schedule_store_parity','schedule_write_parity','bind_policy_parity','auth_middleware_parity','cors_parity','diagnostic_routes_parity','token_routes_parity','folder_routes_parity','schedule_routes_parity','application_routes_parity','config_routes_parity','extension_routes_parity','proxy_routes_parity','fingerprint_routes_parity','profile_read_parity','profile_application_parity','http_fields_parity','profile_write_parity','profile_overview_parity','profile_batch_parity','profile_http_import_parity','ready_parity','profile_http_delete_parity','init_script_parity','browser_options_parity','browser_admission_parity','browser_resources_parity','browser_ip_parity','browser_hints_parity','browser_startup_parity'}
    # These real-file/OS regressions are native integration targets as well,
    # even though their names do not end in _parity. Cargo runs every target.
    expected_tests |= {'first_run_failure', 'live_migration_owner', 'repo_batch_rollback',
                       'pool_add', 'dashboard_write_rollback', 'dashboard_error_kinds', 'test_temp_cleanup', 'session_ready_lifecycle', 'session_registry_lifecycle', 'profile_delete_owned_session', 'browser_adapter_protocol', 'browser_adapter_live', 'browser_launch_admission', 'browser_cookies_parity', 'browser_extensions_parity', 'session_stop_http', 'cookie_export_parity', 'cookie_http_parity', 'finite_browser_lifecycle', 'cli_registry_parity', 'cli_profile_parity', 'cli_settings_parity', 'cli_cookie_parity', 'mcp_parity', 'job_parity', 'bulk_parity', 'cli_pool_parity', 'cli_import_parity', 'cli_launch_parity', 'launch_http_parity', 'command_capture_lifecycle', 'proxy_assignment_parity', 'native_entry', 'native_environment', 'native_input', 'integer_seed_parity', 'relay_request_parity'}
    if discovered_tests != expected_tests:
        raise ValueError('Rust integration suite discovery changed; map its source oracle and CI tier')
    seams = {'models':'models.json','generation':'generation.json','identity':'identity.json',
             'validation':'validation.json','vault':'vault.json','conversion':'conversion.json',
             'journal':'journal.json','paths':'paths.json','config':'config.json','secrets':'secrets.json','repo':'repo.json','completion':'completion.json','repo-write':'repo-write.json','migration':'migration.json','recovery':'recovery.json','claims':'claims.json','vault-enable':'vault-enable.json','vault-rotate':'vault-rotate.json','rotation-recovery':'rotation-recovery.json','folders':'folders.json','repo-batch':'repo-batch.json','repo-lookup':'repo-lookup.json','repo-delete':'repo-delete.json','data-root':'data-root.json','first-run':'first-run.json','pool-parse':'pool-parse.json','pool-normalize':'pool-normalize.json','pool-store':'pool-store.json','pool-vault':'pool-vault.json','profile-export':'profile-export.json','profile-import':'profile-import.json','profile-backup':'profile-backup.json','profile-restore':'profile-restore.json','extension-resources':'extension-resources.json','extension-ids':'extension-ids.json','extension-packages':'extension-packages.json','extension-install':'extension-install.json','token-store':'token-store.json','token-create':'token-create.json','request-auth':'request-auth.json','dashboard-store':'dashboard-store.json','dashboard-vault':'dashboard-vault.json','dashboard-batch':'dashboard-batch.json','dashboard-meta':'dashboard-meta.json','dashboard-revision':'dashboard-revision.json','schedule-store':'schedule-store.json','schedule-write':'schedule-write.json','bind-policy':'bind-policy.json','cors':'cors.json','diagnostic-routes':'diagnostic-routes.json','token-routes':'token-routes.json','folder-routes':'folder-routes.json','schedule-routes':'schedule-routes.json','application-routes':'application-routes.json','config-routes':'config-routes.json','extension-routes':'extension-routes.json','proxy-routes':'proxy-routes.json','fingerprint-routes':'fingerprint-routes.json','profile-read':'profile-read.json','http-fields':'http-fields.json','profile-write':'profile-write.json','profile-overview':'profile-overview.json','profile-batch':'profile-batch.json','profile-http-import':'profile-http-import.json','ready':'ready.json','profile-http-delete':'profile-http-delete.json','init-script':'init-script.json','browser-options':'browser-options.json','browser-admission':'browser-admission.json','browser-resources':'browser-resources.json','browser-ip':'browser-ip.json','browser-hints':'browser-hints.json','browser-startup':'browser-startup.json'}
    for seam, name in seams.items():
        expected = json.loads((FIXTURES / name).read_text(encoding='utf-8'))
        actual = document(seam)
        actual['source_runtime'] = expected['source_runtime']
        if not expected['cases'] or actual != expected:
            raise ValueError(f'Source oracle drift: {seam}')
    schema = json.loads((ROOT / 'engine-rust/src/http/field_schema.json').read_text(encoding='utf-8'))
    original = document('http-fields')
    original.pop('cases')
    original['source_runtime'] = schema['source_runtime']
    if schema != original:
        raise ValueError('HTTP field schema drift')
    for seam,name in [('devices','device_catalog.json'),('field-policy','field_policy.json'),('pool-digits','pool_digit_ranges.json')]:
        expected = json.loads((ROOT / 'engine-rust' / 'src' / name).read_text(encoding='utf-8'))
        # Catalog constants contain Python tuples; fixtures are their JSON
        # representation. Compare that wire representation, not tuple vs list.
        actual = json.loads(json.dumps(document(seam), ensure_ascii=True))
        actual['source_runtime'] = expected['source_runtime']
        if actual != expected:
            raise ValueError(f'Source catalog drift: {seam}')


if __name__ == '__main__':
    with run_log('verify-oracles') as logger:
        verify()
        logger.info('All implemented source oracle seams match; full migration is not implied')
