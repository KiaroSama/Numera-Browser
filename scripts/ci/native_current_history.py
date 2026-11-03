"""Pinned current-signature defects, applied after historical compatibility overlays."""

CURRENT_CALLER_REGRESSIONS = (
    {
        'target': 'mcp_parity',
        'test': 'stdio_text_boundaries_match_original_responses',
        'symptom': 'MCP stdio text response differs',
        'commit': '8f6ad94e95693d2c3ff18d2ca39ec8effa635cd6',
        'files': {
            'engine-rust/src/mcp.rs':
                '802ba1d69ce81d705cb501970df1230b9fd2b733409381b0b7632afaacdebca6',
        },
    },
    {
        'target': 'lib',
        'test': 'network::tests::proxy_echo_boundaries_match_original_through_transport',
        'symptom': 'Proxy echo response differs',
        'commit': '4e570f130ba807448453c12777457bcf55911a67',
        'files': {
            'engine-rust/src/network/proxy_check.rs':
                '39d4c4f728c91ef9ad342ba4663ae73c3499651b985f07d558faf34ba04424d5',
        },
    },
    {
        'target': 'browser_options_parity',
        'test': 'dns_options_match_original_whitespace_and_refusals',
        'symptom': 'DNS launch options differ',
        'commit': '461eebcb6b580d8b399dde00e189b510c650f6f4',
        'files': {
            'engine-rust/src/browser/options.rs':
                '48f97356e1d6232fa5a84ab619d8bb7f7d9fc25f6b7f7623447339f5de488263',
        },
    },
    {
        'target': 'bulk_parity',
        'test': 'selection::selection_matches_original_complete_profile_membership',
        'symptom': 'Bulk profile selection differs',
        'commit': 'd7227856f1fc7bec01b66d3737d3cbb729115623',
        'files': {
            'engine-rust/src/bulk.rs':
                'e425d6fd6a60615996504fb5c58758a895bc574126609650f9fafeee62938d09',
        },
    },
    {
        'target': 'profile_http_import_parity',
        'test': 'tags::importer_tags_match_original_whitespace_and_group_display',
        'symptom': 'Imported tag display differs',
        'commit': '965ba02e5eceacb7c0be4e2d9a4e873b52279610',
        'files': {
            'engine-rust/src/importers.rs':
                'e4526b6a17983cc5a15fe89cd1d3de28dc60b0fcccb57a078a8a5195e3355751',
        },
    },
    {
        'target': 'profile_http_import_parity',
        'test': 'tags::importer_refusals_retain_all_original_ordered_problems',
        'symptom': 'Complete importer refusal differs',
        'commit': '965ba02e5eceacb7c0be4e2d9a4e873b52279610',
        'files': {
            'engine-rust/src/importers.rs':
                'e4526b6a17983cc5a15fe89cd1d3de28dc60b0fcccb57a078a8a5195e3355751',
        },
    },
    {
        'target': 'job_parity',
        'test': 'text::job_text_admission_matches_original_without_replacing_identity_or_outcome',
        'symptom': 'Job text admission differs',
        'commit': '51a627bd81ad3b7d6b130e85fa45c7354e5d3637',
        'files': {
            'engine-rust/src/jobs.rs':
                '643bcc66a0d2750dd530abdd91e9a59c24d97c9e3afb39897f07160b18b3977a',
        },
    },
    {
        'target': 'claim_parity',
        'test': 'diagnostics::claim_diagnostics_match_original_truth_and_display_without_changing_records',
        'symptom': 'Claim diagnostic display differs',
        'commit': 'd25e088d8adb4c70e2dd401ffbe100331cfb35f4',
        'files': {
            'engine-rust/src/ownership/claims.rs':
                '6af8a9ffd4a29f9ce29da25da9ecf8e4b4fc2031b9228424be8c578650eccbb8',
        },
    },
    {
        'target': 'profile_http_import_parity',
        'test': 'lexical::importer_lexical_profiles_match_original_without_changing_inputs',
        'symptom': 'Imported lexical dashboard differs',
        'commit': '4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1',
        'files': {
            'engine-rust/src/importers.rs':
                'e04030f2743e3f77625e0428c28c5727aabf39091e90aa01351775d7a0d16572',
        },
    },
    {
        'target': 'profile_http_import_parity',
        'test': 'lexical::native_envelope_versions_match_original_exact_numeric_guidance',
        'symptom': 'Native export version guidance differs',
        'commit': '4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1',
        'files': {
            'engine-rust/src/import_format.rs':
                'fb386692f47d6e7d0281b302112001f6af120418d0bbf5d765c231243de51233',
        },
    },
    {
        'target': 'mcp_parity',
        'test': 'captured_command_errors_match_original_stream_selection',
        'symptom': 'MCP captured result differs',
        'commit': 'db29928b16d08810ce88243f0dd1a6e233e2dbf6',
        'files': {
            'engine-rust/src/mcp_command.rs':
                '212a821995a0582c0f07913ec1ac9c83172b0fcb1024ccfc1eb3fe9eb95e7dd1',
        },
    },
    {
        'target': 'browser_ip_parity',
        'test': 'timezone_grades_match_original_proxy_and_trust_boundaries',
        'symptom': 'Timezone proxy grade differs',
        'commit': 'f409196e68c86a2bb3e3a2b136217e1d2f276c62',
        'files': {
            'engine-rust/src/browser/network_grades.rs':
                '5583d5df14afb382ec177587e2ac54b9553beb4c3b49e6d6b89654adcb52fe71',
        },
    },
    {
        'target': 'cli_cookie_parity',
        'test': 'alignment_versions::alignment_versions_match_original_decimal_captures',
        'symptom': 'Alignment version fingerprint differs',
        'commit': '8e94027cf9671b343528fb29a79375187f42b7ba',
        'files': {
            'engine-rust/src/browser/alignment.rs':
                '327731362383aea951c7ed3fc75c2fb1d02017c7f8ed2b1ac25e5315dfaddbaf',
        },
    },
    {
        'target': 'cli_registry_parity',
        'test': 'command_registry_matches_original_schemas_arguments_and_parse_views',
        'symptom': 'CLI confirmation decision differs',
        'commit': '5501361f815916c1a2bd47e7e342de5117b4e4ff',
        'files': {
            'engine-rust/src/cli/output.rs':
                'aab0acee9ecf439ddf252a983878e908b362eebeb54901e6e892d205b2b55ac6',
        },
    },
    {
        'target': 'repo_batch_parity',
        'test': 'complete_batch_refusals_match_original_index_order_and_preserved_bytes',
        'symptom': 'Complete indexed batch refusal differs',
        'commit': 'd0d81cc6c173c302520f107061f53b57ff0ce885',
        'files': {
            'engine-rust/src/storage/repo_write.rs':
                'ce4d904503fd52f828896f9abd840e76aee1e0131f292064fd99a875a864c53f',
        },
    },
    {
        'target': 'profile_restore_parity',
        'test': 'restore_copy_candidates_match_original_existing_name_ownership',
        'symptom': 'Restore candidate ownership differs',
        'commit': '5f356644fa7a132862b002bebf8222931bccc806',
        'files': {
            'engine-rust/src/storage/restore.rs':
                '85987e107f9aa70c6a3f17fea325eaac4d67309d9a2fe446b288a6ca6be1aa1a',
        },
    },
    {
        'target': 'relay_request_parity',
        'test': 'socks_origin_requests_preserve_original_target_bytes',
        'symptom': 'SOCKS origin request bytes differ',
        'commit': '60ffc489916e7a2560229fcc5a7be5b813196e72',
        'files': {
            'engine-rust/src/network/relay_http.rs':
                '830ac7683ae157810cb5f2589f0e253cc6c09661b3204f6785b9417f2ca738dd',
        },
    },
    {
        'target': 'native_entry',
        'test': 'browser_dispatch_uses_actual_handshake_without_redundant_metadata',
        'symptom': 'Browser dispatch repeated SDK metadata preflight',
        'commit': '2c6d23eed78cdf1f3ddf2c41c6e103d987cfe527',
        'files': {
            'engine-rust/src/native_entry.rs':
                'abf6a5e9b6690231795845589d28c2ddb7faded39d65fa635afd3919547837e1',
        },
    },
    {
        'target': 'relay_request_parity',
        'test': 'handshake::socks_handshake_failures_preserve_original_diagnostics_and_wire',
        'symptom': 'SOCKS handshake diagnostic differs',
        'commit': 'a45e4ad46b804b19423dde4967017b0087a15598',
        'files': {
            'engine-rust/src/network/relay_protocol.rs':
                'dfc78f467205732f15b0b200aa074729706766002fb3923588856bca4784c489',
        },
    },
    {
        'target': 'extension_routes_parity',
        'test': 'icons::extension_icon_http_matches_original_known_folder_and_file_contracts',
        'symptom': 'Extension icon HTTP status differs',
        'commit': 'edca9d09805ccdc2d29735c8977a4354c71223b7',
        'files': {
            'engine-rust/src/http/extension_routes.rs':
                'd6624367937929bd0e719243925d12a94787382525c01c0717e58e916dea2ac2',
        },
    },
    {
        'target': 'extension_routes_parity',
        'test': 'icons::extension_icon_http_matches_original_known_folder_and_file_contracts',
        'symptom': 'Extension icon HTTP status differs',
        'commit': '8354eeb286d93f9418ee372ed057c129521404ad',
        'platform': 'win32',
        'files': {
            'engine-rust/src/storage/locks.rs':
                '8cc9b17a4c77c5db9b178d8ea2b16318132f5d11439359c3e47abca269d6a373',
        },
    },
    {
        'target': 'profile_read_parity',
        'test': 'divergence::divergent_profile_http_matches_original_refusal_kind_without_mutation',
        'symptom': 'Divergent HTTP response kind differs',
        'commit': 'edca9d09805ccdc2d29735c8977a4354c71223b7',
        'files': {
            'engine-rust/src/http/profile_errors.rs':
                '11c6e0df12446f6f5b5669b32c00ac6b832a574c18b89c2f3efa24b2509740a9',
        },
    },
    {
        'target': 'ownership_receipt_parity',
        'test': 'receipt_tokens_match_original_without_resolving_invalid_transfers',
        'symptom': 'Receipt completeness differs',
        'commit': 'b1ad79f18f75d9a328b731bb092130cc3b61541a',
        'files': {
            'engine-rust/src/ownership/unit_report.rs':
                '0db789c9ab6f184f811bfadd0f1d3cea7ab0b0bd058663eecbffd816a7bbadcb',
        },
    },
    {
        'target': 'store_open_parity',
        'test': 'denied_store_directories_match_original_actionable_errors_and_preserve_state',
        'symptom': 'Store directory permission diagnostic differs',
        'commit': 'a640a8317180d23e7064e074550bb1a03c432dca',
        'files': {
            'engine-rust/src/storage/repo.rs':
                '3c7f327011f3cba1b54b0934bdcc70c1ca5f1731f5ae5113e9cb645900af256d',
        },
    },
    {
        'target': 'repo_lookup_parity',
        'test': 'read_warnings::read_repair_warnings_match_original_without_changing_read_results',
        'symptom': 'Read repair warning differs',
        'commit': '017f0dcac603bdbbcf56ea80b1156fca23579f9a',
        'files': {
            'engine-rust/src/storage/repo_lookup.rs':
                '8cb44510b66c7c21074cfb168aea8ea3f22d8aa6cc255ea8be70e7a44cde5e32',
        },
    },
    {
        'target': 'lib',
        'test': 'ownership::tree::tests::malformed_group_observations_match_original_without_releasing_claims',
        'symptom': 'Process group uncertainty differs',
        'commit': 'c73bc1abab22788936227e5e2c800dac43acd69e',
        'platform': 'linux',
        'files': {
            'engine-rust/src/ownership/tree.rs':
                '2688b722ab3411a1a5528335a69b029a97e840cb79f7cb065d55735cefe6078b',
        },
    },
    {
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
    },
    {
        'target': 'lib',
        'test': 'storage::repo_delete::tests::failed_delete_rollback_preserves_causes_and_actual_recovery_paths',
        'symptom': 'Delete rollback recovery paths differ',
        'commit': 'c5b466add5c22ca23b53ce2c7ff4ac55093c3ba0',
        'files': {
            'engine-rust/src/storage/repo_delete.rs':
                '9be0a936e39e8ee56c3ac183af13d760fa067c7ae02c6412b28e2e18a199d7dd',
        },
    },
    {
        'target': 'logging_parity',
        'test': 'server_scheduler_errors_reach_the_runtime_file_without_starting_an_sdk',
        'symptom': 'Scheduler error runtime log missing',
        'commit': '09da16a74b2d12d86dfe291148cc431ba39b0e35',
        'files': {
            'engine-rust/src/http/server.rs':
                '7a3f0426584cde037a661abe397344a4d859eb743bef4629b3dc96a20eaf8894',
            'engine-rust/src/native_entry.rs':
                '9e4f147c6ec4ef2674af3e1af809b988e0dbab3ce6aa81f04367f5ef52b9a665',
        },
    },
    {
        'target': 'logging_parity',
        'test': 'serve_verbose_matches_original_debug_admission_through_cli_dispatch',
        'symptom': 'Serve verbose debug admission differs',
        'commit': '9541cdec57186d11b8adfea77f52f27a29018848',
        'files': {'engine-rust/src/native_entry.rs':
                  '9e4f147c6ec4ef2674af3e1af809b988e0dbab3ce6aa81f04367f5ef52b9a665'},
    },
    {
        'target': 'first_run_parity',
        'test': 'legacy_discovery_preserves_original_order_and_never_marks_denied_sources_complete',
        'symptom': 'Legacy discovery refusal differs',
        'commit': 'de48cf502e6d89046b0895e84216a5be2e95507c',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '4459858581e6820f7a0ca4b419cee1b0a546c303f2b7934b8f3e3e33d809dd7a'},
    },
    {
        'target': 'first_run_parity',
        'test': 'imported_new_directories_preserve_original_metadata_without_replacing_existing_modes',
        'symptom': 'First-run directory metadata differs',
        'commit': '1a46f71178be5c482ecad04f14666eadaa52dc0d',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '6a9afa7356a02b123d13ebf8d5742a67e4d6e2bf8abff0d1cb8f2459a94c5670'},
    },
    {
        'target': 'cli_registry_parity',
        'test': 'integer_options_preserve_original_digit_bound_before_zero_normalization',
        'symptom': 'CLI integer digit admission differs',
        'commit': 'ff56fc25f25a4ad9874e644b85295ccd622fd34f',
        'files': {'engine-rust/src/cli/definitions.rs':
                  '62577821c1120e9518a30277bb165d4e77c103ef5803c06157a2a4bc4922c4a7'},
    },
    {
        'target': 'browser_hints_parity',
        'test': 'hint_response_peer_disconnects_preserve_other_io_failures',
        'symptom': 'Hint response peer disconnect escaped',
        'commit': 'f173f3ae59c91fc9524507f91fe6d936b2c4a0d4',
        'files': {'engine-rust/tests/common/hint_site.rs':
                  '4dcfdaa91e4079186c5b7c9268ad50efa06c7f6f5782170c4af8fa2172a879ae'},
    },
    {
        'target': 'cli_registry_parity',
        'test': 'separate_option_values_match_original_token_classification',
        'symptom': 'CLI option token admission differs',
        'commit': 'c299ef506c593623bfd0646b3d2efe1d83596532',
        'files': {'engine-rust/src/cli/mod.rs':
                  'c534342a6f7db08b99641bd52e02e729d3abb68a1e78257b0c502c077479fcd8'},
    },
    {
        'target': 'first_run_failure',
        'test': 'fifo_import_matches_original_refusal_continuation_and_retry',
        'symptom': 'First-run FIFO refusal differs',
        'commit': 'f13c271b158663ca44091efba7539a11b6542c59',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  'ae6dcc19e630d1b110837c55f4726fe300309470772945a6904a634692f59b43'},
    },
    {
        'target': 'first_run_metadata_parity',
        'test': 'imported_attributes_match_original_without_replacing_destination_metadata',
        'symptom': 'First-run extended attributes differ',
        'commit': 'ce84984c7ea2da7cb78ca8a27917fa051c96ab5d',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  'fe024803ade4378889782ce206386660c92a790bde9d1d24ca0451080cb470d4'},
    },
    {
        'target': 'cli_registry_parity',
        'test': 'positional_values_preserve_original_admission_and_option_boundaries',
        'symptom': 'CLI positional token admission differs',
        'commit': 'f0bc556e932e75c6034c9282d7ba6b8c4b573bdf',
        'files': {'engine-rust/src/cli/mod.rs':
                  '43b939bcaeb5acef515b2b22cef85f85842a1667547c17811fca17102935aba5'},
    },
    {
        'target': 'cli_registry_parity',
        'test': 'short_help_preserves_original_explicit_tail_admission',
        'symptom': 'CLI short-help token admission differs',
        'commit': 'a104df072bf48ca200440d5f40298a9ce9b9f24d',
        'files': {'engine-rust/src/cli/option_tokens.rs':
                  'dd7ca91090c4cdbfb2e2c17f5f9108432595f3bd28e7356708b4284d938634da'},
    },
    {
        'target': 'cli_registry_parity',
        'test': 'variadic::variadic_positionals_preserve_original_option_separated_runs',
        'symptom': 'CLI variadic run admission differs',
        'commit': 'e382b5432e7a2bceab89849b037e2b3727fc76c8',
        'files': {'engine-rust/src/cli/mod.rs':
                  '7aa0a8e0303f0e674891d554bfcf38e394569ac6e1219b2c11ad85785be92a54',
                  'engine-rust/src/cli/positional_tokens.rs':
                  'cfe23726c81a6a2f04c827c6dc68f4da68bfcc00cffce2c84bec95847d63b093'},
    },
    {
        'target': 'logging_parity',
        'test': 'retention::run_retention_matches_original_mtime_order_and_namespace',
        'symptom': 'Runtime log retention differs',
        'commit': '3be53623c6dbd55e22ab23dcbe28dc7c4852053d',
        'files': {'engine-rust/src/runtime_log.rs':
                  '78db62948c0519ffadbcf154574caac10078bee0577c6269a0161e4191743590'},
    },
    {
        'target': 'first_run_parity',
        'test': 'nested_copy::new_subtrees_preserve_original_nested_lock_entries',
        'symptom': 'First-run nested copy differs',
        'commit': 'c44a948d6f194368fc8316efb127b46cfd80d9d7',
        'files': {'engine-rust/src/storage/first_run.rs':
                  'a7e702675adb3f12e7495d625d0cb06aa400c594d4997526f8027b6dbb578f73'},
    },
    {
        'target': 'validation_parity',
        'test': 'unicode_representation_matches_original_public_validation_and_argv',
        'symptom': 'Unicode representation differs',
        'commit': '57f0d46506f11248640c2fa2095a05e087c20c65',
        'files': {'engine-rust/src/validation.rs':
                  '5397a1882c0a5cdf867e4d21a353322da76129db2d982fe6093c4eeb9c7d0591'},
    },
    {
        'target': 'validation_parity',
        'test': 'finite_numbers_match_original_validation_and_mcp_representation',
        'symptom': 'Numeric representation differs',
        'commit': '65e9dbd4b9a1b15ee76a27d7cf3947b18455c21d',
        'files': {'engine-rust/src/validation.rs':
                  '01a2a4718ff2792ffd80ab5f130261007409ed58e3aea1ffaf28d8eae3327158'},
    },
    {
        'target': 'validation_parity',
        'test': 'permitted_fraction_spellings_match_original_numeric_admission',
        'symptom': 'Fraction numeric admission differs',
        'commit': '75acb553e30728dd58472facc5acd09791de38a1',
        'files': {'engine-rust/src/validation.rs':
                  'd67e56a33198c01e40b4e2fe0ddcfbb4429dd83c4de45c4972e65ed21b486066'},
    },
    {
        'target': 'first_run_metadata_parity',
        'test': 'direct_copy_access_times_match_original_without_changing_copytree_snapshots',
        'symptom': 'First-run access-time copy policy differs',
        'commit': 'b6bab47c1f34872e11061586ae5c5291cfe1c056',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '7f07dcc91641071ebb5e5576c3e0bcf3186400fd9e0d199f227231503a2ea002'},
    },
    {
        'target': 'conversion_parity',
        'test': 'container_text_conversion_matches_original_profiles_and_roundtrips',
        'symptom': 'Container text conversion differs',
        'commit': '12a76ec9f205fd6c2e760eaf28bdd64aa0105c8b',
        'files': {'engine-rust/src/conversion.rs':
                  'c52af49700c56939aa482ba1c155aed5b861258e7404cf4c5069df70e65ceef4'},
    },
    {
        'target': 'first_run_failure',
        'test': 'copytree_errors::new_subtree_errors_match_original_continuation_summaries_and_retry',
        'symptom': 'First-run copytree failure aggregation differs',
        'commit': 'da41e44368222813bb929cf579fd915baeeea7b1',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '6e865c0e82abea1c6cdb7ad793d5bc2eab39c05bedc37cfbd311d92854e5da1b'},
    },
    {
        'target': 'first_run_failure',
        'test': 'profile_count::advisory_profile_count_matches_original_when_destination_scan_is_denied',
        'symptom': 'First-run advisory profile count differs',
        'commit': 'c2615a4e5e90afc60c943142818e37906a952a47',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '81ebc1b78e948ce3c23a275db4b69e15c2555c6c7f2938b5d1a80b447f8669f8'},
    },
    {
        'target': 'cli_registry_parity',
        'test': 'version::version_flag_matches_original_root_scope_and_value_admission',
        'symptom': 'CLI version flag admission differs',
        'commit': 'a071ff7f2ceb55cd7ed9f05198692e9ac19bcb17',
        'files': {'engine-rust/src/cli/mod.rs':
                  '6d6cfccdbed4a0ebf60ed438d17f0ab5d5e688813a61162287e360d4feb4150a'},
    },
    {
        'target': 'cli_registry_parity',
        'test': 'version::version_flag_matches_original_root_scope_and_value_admission',
        'symptom': 'CLI version flag admission differs',
        'commit': '77cab4fb18ac3dcace76a34bc54c13d3239a70ed',
        'files': {'engine-rust/src/cli/mod.rs':
                  '1a838cce524b7b46713ea101e3b45160b3f34a979b44ed3e89a2ec7a058000cb'},
    },
    {
        'target': 'vault_parity',
        'test': 'crypto_text_admission_matches_original_and_new_tokens_reopen_there',
        'symptom': 'Crypto text admission differs',
        'commit': 'dccccf2ad40820606b1c74f4de951cb163e0fe6a',
        'files': {'engine-rust/src/vault/mod.rs':
                  '71150ac122b5f5c26199ff9fcac4c042db5fcd403beb89f1a809dd0da925de3b'},
    },
)
