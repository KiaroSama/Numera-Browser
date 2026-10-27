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
)
