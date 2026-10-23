"""Pinned production defects exercised by the hosted native regression runner."""

REGRESSIONS = (
    {
        'target': 'model_parity',
        'test': 'optional_proxy_reads_match_original_without_rewriting_storage',
        'symptom': '"false" model verdict',
        'commit': 'a2942c0e18c2d90b622bf10a7598c417158f39b6',
        'files': {
            'engine-rust/src/models.rs':
                'be512b37753170fc18a479cadd50e308f6088f76a040977a3f932bffd7fda0f4',
        },
    },
    {
        'target': 'cookie_export_parity',
        'test': 'netscape_export_matches_original_bytes_and_refusals',
        'symptom': '"json-padding-001c" cookie format detection',
        'commit': '87bc6022ef3e576ffe2d9433dfb83ea859d11fb2',
        'files': {
            'engine-rust/src/browser/cookie_format.rs':
                '50c6771ac687ab326b6c5f22a5be8e3f49920d1fe09086cfa2ea7ea9201bc936',
            'engine-rust/src/browser/cookie_normalize.rs':
                'a9f11c1651a1fef70b36380f5b89d09a80c3c36a1e07d5a9c194a5a08a9a97ee',
        },
    },
    {
        'target': 'cookie_export_parity',
        'test': 'netscape_export_matches_original_bytes_and_refusals',
        'symptom': '"positive-expires" numeric expiry normalization verdict',
        'commit': '41917dfa020f107ca95f1ee66ffb961786b6be00',
        'files': {
            'engine-rust/src/browser/cookie_expiry.rs':
                '50578cdaa9765e22754c430d910ae302eb1ae1fddc566ccba8fdefe9d23944e4',
        },
    },
    {
        'target': 'native_environment',
        'test': 'metadata_discovery_uses_sdk_only_adapter_and_rejects_malformed_receipts',
        'symptom': 'SDK metadata wrote bytecode into runtime inputs',
        'commit': 'd1c443d4cdef191dcb7c4e82cf6194a2e85b2b33',
        'files': {
            'engine-rust/src/native_environment.rs':
                '0fae0f236ffb9f71423a0d362a570edbe3e26c92d2b77b30377f060fa3ecaec5',
        },
    },
    {
        'target': 'repo_lookup_parity',
        'test': 'ordinary_encrypted_reads_preserve_original_bytes_and_repair_only_real_drift',
        'symptom': 'Ordinary encrypted read rewrote protected bytes',
        'commit': '6fe0b77f09f1e4c02e3bb1a02e8aa4b1c53cea4c',
        'files': {
            # Keep the shared raw-reader signature consistent with every historical caller.
            'engine-rust/src/storage/repo.rs':
                '1169cbe37db745c440570f31ce14cb398448e74751383c09f07e50e50a2d229a',
            'engine-rust/src/storage/repo_lookup.rs':
                'a787c5c83d09becd8f76aadc5892493a3fd071efdd135e15183696bd09ab19af',
            'engine-rust/src/storage/repo_write.rs':
                '93cd77e5c4af0de5e2b8d9440099defae9e17dd93f2deae3ec050af5343d5bbf',
            'engine-rust/src/storage/dashboard.rs':
                '949ec2682b55d039eaa915d8c83298dbaa0adc7f89df4d2d7c1495836c071808',
        },
        'companions': {
            'engine-rust/tests/repo_lookup_parity.rs': {
                'commit': '0cb635a85fc639eec6926304291a3f91a4f6e637',
                'sha256': '3267e368991ce775f580b66f2068211d2835dddcf98eb37c205a2e371386d406',
            },
            'engine-rust/src/storage/archive.rs': {
                'commit': '01be6dfe20c04d4cbabe3b4bdce63c8270c45535',
                'sha256': '6382da3256a98870707a45d25a17a7f2933902ae8171b4044b3a305e2e354ec5',
            },
            'engine-rust/src/storage/pool.rs': {
                'commit': '371fe475ad72fb518e5c6650fc5ebb022d7d1c10',
                'sha256': '2693fea8b2b4b7a6897072ee31a83342792c06853f3a70e0a7d401e77601df79',
            },
        },
    },
    {
        'target': 'integer_seed_parity',
        'test': 'complete_integer_seed_identities_and_commands_match_original',
        'symptom': 'Integer seed command admission differs',
        'commit': '348bac29ca7e522dad0c7dc64ab2e7349152f790',
        'files': {
            'engine-rust/src/cli/bulk_commands.rs':
                '8f104af04b864e076e35a8ad41494ed819e84a44e9cfbf2fcf574dd8da931d5c',
        },
    },
    {
        'target': 'vault_parity',
        'test': 'envelope_classification_matches_original_unicode_and_prefix_contracts',
        'symptom': 'Unicode envelope classification differs',
        'commit': '2c51d1ad66e5da28e8035de77ecb32d3998870ea',
        'files': {
            'engine-rust/src/vault/mod.rs':
                'ecbe0ae8c19f4ca8c15c71fab2dab37ecac024a6a05d784b5041db12ccc12dd8',
        },
    },
    {
        'target': 'validation_parity',
        'test': 'packed_screen_integer_grammar_matches_original',
        'symptom': 'Packed screen integer grammar differs',
        'commit': '22e0e596732041039759ea27d02efc160b8712b6',
        'files': {
            'engine-rust/src/validation.rs':
                '43b7b97c20df8722a8acc5f9108c06441372302439fe1abff624b0e0de69d538',
        },
    },
    {
        'target': 'folder_parity',
        'test': 'folder_name_whitespace_matches_original_operations',
        'symptom': 'Folder name normalization differs',
        'commit': 'bb829418b9bd15bebd86030666a4ac91d45d55af',
        'files': {
            'engine-rust/src/storage/folders.rs':
                '614fd00e19b151103eea2d61d17ef91ee65e15b182430b4723f0be2ce0dc5171',
        },
    },
    {
        'target': 'folder_parity',
        'test': 'malformed_folder_text_refuses_without_replacing_source_bytes',
        'symptom': 'Folder text admission differs',
        'commit': '86c0084eed5a170385a6653024e9e9e5e2c9809a',
        'files': {
            'engine-rust/src/storage/folders.rs':
                'ae1558078e57a802b067aa17fcc65ccd960fe8bf38c8242a3f2335842bb6962a',
        },
    },
    {
        'target': 'config_parity',
        'test': 'malformed_settings_text_refuses_without_replacing_source_bytes',
        'symptom': 'Settings text mutation admission differs',
        'commit': '1c0d77d23ec72aaa67f69d680378ebd8e6fc89d7',
        'files': {
            'engine-rust/src/storage/config.rs':
                'a803a3e5c3ce12cae8b4200e4f54ae5dec1a05e38d3cac5b99e966f360180c01',
        },
        'companions': {
            # Preserve the old assertion without compiling APIs added afterward.
            'engine-rust/tests/config_parity.rs': {
                'commit': '03041cedae816d5d360352ab6186a6b485e4d60f',
                'sha256': '3bdc096885588ad4640650934e1118c5068c112afb045d9815be6586f4432c1a',
            },
        },
    },
    {
        'target': 'extension_resources_parity',
        'test': 'extension_path_whitespace_matches_original_validation',
        'symptom': 'Extension path boundary whitespace differs',
        'commit': '4b84da3a5502ef54123f78e76a70e245593cadee',
        'files': {
            'engine-rust/src/storage/extensions.rs':
                'e9ddc94cd7db3cb867f9c118a94a858592786ad02eea92a517b9dbadc3b78f6f',
        },
    },
    {
        'target': 'extension_resources_parity',
        'test': 'malformed_extension_text_matches_original_refusal_without_changing_bytes',
        'symptom': 'Extension resource text admission differs',
        'commit': '573b743f7c444f2f3cba672ee12ac9d25e3d2aec',
        'files': {
            'engine-rust/src/storage/extensions.rs':
                '46636571378a748deee180593862b2005ce6a6f7d7439f35a33ae0e97768ef69',
        },
    },
    {
        'target': 'extension_resources_parity',
        'test': 'extension_icon_integer_ranking_matches_original_selection',
        'symptom': 'Extension icon integer ranking differs',
        'commit': '195e15e84342749bcbee6570931a8be5aa4662ec',
        'files': {
            'engine-rust/src/storage/extensions.rs':
                '640b01c025fe2df3f58c927f05a66042dbf4eb8af5e492cc0ffd616b6afce570',
        },
    },
    {
        'target': 'first_run_parity',
        'test': 'complete_first_run_summaries_match_original_paths_and_copy_results',
        'symptom': 'Complete first-run summary differs',
        'commit': 'fa2bcf0198323354420f109ad6c947d54fabcb29',
        'files': {
            'engine-rust/src/storage/first_run.rs':
                '43ba84c3feff481e7c9bb3949be68fc56ffdf8427d1f1a31f948ec657f8e822b',
        },
    },
    {
        'target': 'storage_parity',
        'test': 'persisted_json_text_matches_original_escaping_and_platform_newlines',
        'symptom': 'Persisted JSON bytes differ',
        'commit': '323db4f514bc24b03d0a8ef3594bd3924769f6ba',
        'files': {
            'engine-rust/src/storage/atomic.rs':
                'd02177de1a44ef62ebe74a4c2bd6bc17b588b5d2f0342d33a7eca20a4aa8c7dd',
        },
    },
    {
        'target': 'cli_cookie_parity',
        'test': 'cookie_cli_services_preserve_source_refusals_output_and_owned_sdk_policy',
        'symptom': 'Cookie CLI persisted export bytes differ',
        'commit': '06b69660cb5378588167654fd0f7c51e4a64724d',
        'platform': 'win32',
        'files': {
            'engine-rust/src/cli/cookie_commands.rs':
                '0eb9dc93b3279e5bfa52bedecf1a2411286a07caa356ace94859faf4d2cbb521',
        },
    },
    {
        'target': 'repo_lookup_parity',
        'test': 'damaged_profile_text_refuses_before_repair_or_replacement',
        'symptom': 'Profile text admission differs',
        'commit': '5c5d7b8c1af91d72a5ed08fbf5dd7bc3101581c2',
        'files': {
            'engine-rust/src/storage/repo.rs':
                '1169cbe37db745c440570f31ce14cb398448e74751383c09f07e50e50a2d229a',
            'engine-rust/src/storage/repo_lookup.rs':
                'b1fc30eacfc5fce2e8390e08e2bb34d260bd2fe1d72adf7e33d77d0bc316ba16',
            'engine-rust/src/storage/repo_write.rs':
                '93cd77e5c4af0de5e2b8d9440099defae9e17dd93f2deae3ec050af5343d5bbf',
            'engine-rust/src/storage/dashboard.rs':
                '949ec2682b55d039eaa915d8c83298dbaa0adc7f89df4d2d7c1495836c071808',
        },
        'companions': {
            'engine-rust/tests/repo_lookup_parity.rs': {
                'commit': '0cb635a85fc639eec6926304291a3f91a4f6e637',
                'sha256': '3267e368991ce775f580b66f2068211d2835dddcf98eb37c205a2e371386d406',
            },
            'engine-rust/src/storage/archive.rs': {
                'commit': '01be6dfe20c04d4cbabe3b4bdce63c8270c45535',
                'sha256': '6382da3256a98870707a45d25a17a7f2933902ae8171b4044b3a305e2e354ec5',
            },
            'engine-rust/src/storage/pool.rs': {
                'commit': '371fe475ad72fb518e5c6650fc5ebb022d7d1c10',
                'sha256': '2693fea8b2b4b7a6897072ee31a83342792c06853f3a70e0a7d401e77601df79',
            },
        },
    },
    {
        'target': 'dashboard_store_parity',
        'test': 'dashboard_readers_preserve_strict_text_admission_and_original_fallbacks',
        'symptom': 'Dashboard text admission differs',
        'commit': 'e2437340a4ea92a0f84a4bb15a0a31b8f07c9b78',
        'files': {
            'engine-rust/src/storage/dashboard.rs':
                'c38a06af763503c9dbf9b74068a48c82527f82c7b5a0d65551d49ebbb1cd1f0a',
        },
    },
    {
        'target': 'conversion_parity',
        'test': 'conversion_text_boundaries_match_original_profiles_and_roundtrips',
        'symptom': 'Conversion text boundaries differ',
        'commit': '1a2b09f7cd996f9f81a88e99f7f5a68a9297a007',
        'files': {
            'engine-rust/src/conversion.rs':
                '5fd4c1691c48c219514875526ab26bc8d43736f428ce8ee5eea18dadef2c108c',
            'engine-rust/src/http/fingerprint_routes.rs':
                '6d9a0ef84108e4ba75bcc7531adbe95069f1177f227739f2575d14bf7cc1d11e',
        },
    },
    {
        'target': 'conversion_parity',
        'test': 'reverse_proxy_conversion_matches_original_without_canonicalizing_records',
        'symptom': 'Reverse proxy conversion differs',
        'commit': 'e04e4fce1042e0a1862503db89f688efeceee585',
        'files': {
            'engine-rust/src/conversion.rs':
                '075d140ff7ec579549192429042da4438f382563d44c069bb3da013d5e5ecf9f',
            'engine-rust/src/http/fingerprint_routes.rs':
                '6d9a0ef84108e4ba75bcc7531adbe95069f1177f227739f2575d14bf7cc1d11e',
        },
    },
    {
        'target': 'conversion_parity',
        'test': 'integer_conversion_matches_original_grammar_defaults_and_seed_magnitude',
        'symptom': 'Integer conversion differs',
        'commit': '67f6ac34073fc56bc7bffe4d577c22c7ef4049be',
        'files': {
            'engine-rust/src/conversion.rs':
                '7c187a074ae65c892eddd20732685fb1e3e0af08d4e8cb4875c3bfa4b8301033',
            'engine-rust/src/http/fingerprint_routes.rs':
                '6d9a0ef84108e4ba75bcc7531adbe95069f1177f227739f2575d14bf7cc1d11e',
        },
    },
    {
        'target': 'conversion_parity',
        'test': 'fractional_conversion_matches_original_numeric_values_and_defaults',
        'symptom': 'Fractional conversion differs',
        'commit': 'ba29315bc8a3bf5990252e2acb900cc5fa6733df',
        'files': {
            'engine-rust/src/conversion.rs':
                '555663bfd93960c5a46fb92641093c288aa735ac6def3cff2521ddc0e0216c49',
            'engine-rust/src/http/fingerprint_routes.rs':
                '6d9a0ef84108e4ba75bcc7531adbe95069f1177f227739f2575d14bf7cc1d11e',
        },
    },
    {
        'target': 'profile_read_parity',
        'test': 'relative_activity_http_matches_original_without_rewriting_storage',
        'symptom': 'Relative activity HTTP status differs',
        'commit': '199eb08a6a7114395513ad4ad8aba985504c91a0',
        'files': {
            'engine-rust/src/http/profile_routes.rs':
                '2077705d1d09bc686361406d68b65ac99de9a049c6957bbd12974237c25935cd',
        },
    },
    {
        'target': 'pool_parse_parity',
        'test': 'pooled_proxy_text_boundaries_match_original_full_results',
        'symptom': 'Pooled proxy text boundaries differ',
        'commit': 'ef6ebe26ba183fbbd9892d077d70fdcd8f7f7c64',
        'files': {
            'engine-rust/src/pool.rs':
                '5381266449dafc5eca9009fe1d900c3aed760da00a3855b8d122dc58772bb5ab',
            'engine-rust/src/pool_normalize.rs':
                'f97331c9da3246f9ad993df1cacfe80f9d59d5494fd6db4c9c8cd26b05cd17e7',
        },
        'companions': {
            'engine-rust/src/vault/journal.rs': {
                'commit': '0147a3cafbab1c0912484db576bc5b5169b2dc75',
                'sha256': '406542fa8f85431a0ef72a922b1815fbc149bac136f4ff291afaaf76c5a5fade',
            },
            'engine-rust/src/importers.rs': {
                'commit': '4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1',
                'sha256': 'e04030f2743e3f77625e0428c28c5727aabf39091e90aa01351775d7a0d16572',
            },
            'engine-rust/src/browser/options.rs': {
                'commit': '461eebcb6b580d8b399dde00e189b510c650f6f4',
                'sha256': '48f97356e1d6232fa5a84ab619d8bb7f7d9fc25f6b7f7623447339f5de488263',
            },
            'engine-rust/src/network/proxy_check.rs': {
                'commit': '4e570f130ba807448453c12777457bcf55911a67',
                'sha256': '39d4c4f728c91ef9ad342ba4663ae73c3499651b985f07d558faf34ba04424d5',
            },
            'engine-rust/src/mcp.rs': {
                'commit': '8f6ad94e95693d2c3ff18d2ca39ec8effa635cd6',
                'sha256': '802ba1d69ce81d705cb501970df1230b9fd2b733409381b0b7632afaacdebca6',
            },
        },
    },
    {
        'target': 'pool_normalize_parity',
        'test': 'pooled_proxy_integer_and_whitespace_normalization_matches_original',
        'symptom': 'Pooled proxy normalization differs',
        'commit': 'ef6ebe26ba183fbbd9892d077d70fdcd8f7f7c64',
        'files': {
            'engine-rust/src/pool_normalize.rs':
                'f97331c9da3246f9ad993df1cacfe80f9d59d5494fd6db4c9c8cd26b05cd17e7',
        },
    },
    {
        'target': 'pool_store_parity',
        'test': 'malformed_pool_text_refuses_without_replacing_original_bytes',
        'symptom': 'Pool storage text admission differs',
        'commit': 'ef6ebe26ba183fbbd9892d077d70fdcd8f7f7c64',
        'files': {
            'engine-rust/src/storage/pool.rs':
                'e42ab23148d1e1a757e2486e3c7ae164b068d87902ddd260329c66bd8cfbfa3b',
        },
    },
    {
        'target': 'lock_interop_parity',
        'test': 'configured_store_acquisitions_honor_original_lock_owners',
        'symptom': 'Configured data-lock acquisition ignored the original timeout',
        'commit': '0de3fabd302595e48a156022625c31cd2018bdb3',
        'files': {
            'engine-rust/src/storage/pool.rs':
                'bfb4199d7423b264e4f2c61901ec0a2d9431b39fe28aa6de7802c805027967ec',
        },
    },
    {
        'target': 'profile_export_parity',
        'test': 'withheld_text_admission_matches_original_without_replacing_bytes',
        'symptom': 'Withheld text admission differs',
        'commit': '01be6dfe20c04d4cbabe3b4bdce63c8270c45535',
        'files': {
            'engine-rust/src/storage/archive.rs':
                '6382da3256a98870707a45d25a17a7f2933902ae8171b4044b3a305e2e354ec5',
        },
    },
    {
        'target': 'profile_export_parity',
        'test': 'exported_profile_bytes_match_original_text_format_and_preserve_refusals',
        'symptom': 'Profile export bytes differ',
        'commit': '01be6dfe20c04d4cbabe3b4bdce63c8270c45535',
        'files': {
            'engine-rust/src/storage/archive.rs':
                '6382da3256a98870707a45d25a17a7f2933902ae8171b4044b3a305e2e354ec5',
        },
    },
    {
        'target': 'vault_parity',
        'test': 'crypto_text_admission_matches_original_and_new_tokens_reopen_there',
        'symptom': 'Crypto text admission differs',
        'commit': '234504803ef8ac92c9cb076aeec9bc942f4553d3',
        'files': {
            'engine-rust/src/vault/mod.rs':
                '5fbf9a042308ce8405b66c8e2276372272157566cc2e420594e0aa9e1ae677fd',
        },
    },
    {
        'target': 'journal_parity',
        'test': 'journal_descriptor_boundaries_match_original_without_repairing_evidence',
        'symptom': 'Journal descriptor boundary differs',
        'commit': '0147a3cafbab1c0912484db576bc5b5169b2dc75',
        'files': {
            'engine-rust/src/vault/journal.rs':
                '406542fa8f85431a0ef72a922b1815fbc149bac136f4ff291afaaf76c5a5fade',
        },
    },
    {
        'target': 'completion_parity',
        'test': 'completion_diagnostics_match_original_distinct_paths_and_readable_holders',
        'symptom': 'Completion diagnostic differs',
        'commit': '0147a3cafbab1c0912484db576bc5b5169b2dc75',
        'files': {
            'engine-rust/src/vault/completion.rs':
                '17f08839d708f47f1b2d926b6e55f32691706d99d294647ae576d87cbc98f8ce',
        },
    },
    {
        'target': 'recovery_parity',
        'test': 'unreadable_active_config_preserves_original_recovery_causes_and_bytes',
        'symptom': 'Active config recovery causes differ',
        'commit': '0147a3cafbab1c0912484db576bc5b5169b2dc75',
        'files': {
            'engine-rust/src/vault/recovery.rs':
                'a86dd9cb60e15f03b20fd0d56e4e1e1767df08cc40746b949a75f432a6b6512b',
        },
        'companions': {
            'engine-rust/src/vault/rotation_recovery.rs': {
                'commit': '0147a3cafbab1c0912484db576bc5b5169b2dc75',
                'sha256': '113218c52d07a1af318ee0f05d073443334159612f561a335e66523f12738837',
            },
        },
    },
    {
        'target': 'config_parity',
        'test': 'settings_instance_cache_matches_original_file_identity_and_detached_results',
        'symptom': 'Settings cache result differs',
        'commit': '813a4c6c490fbe8438a1604a3c25d7fddf6ba9db',
        'files': {
            'engine-rust/src/storage/config.rs':
                '808535a3970096c53c68151cca8965f5ef261175e3eb54f85daea21ee1f4b002',
        },
    },
    {
        'target': 'repo_parity',
        'test': 'repository_settings_cache_matches_original_across_secret_reads',
        'symptom': 'Repository settings cache differs',
        'commit': '371fe475ad72fb518e5c6650fc5ebb022d7d1c10',
        'files': {
            'engine-rust/src/storage/repo.rs':
                'e14695584f9a43425511c688cab958140ca791e1bc5c571e8a391437606c1532',
        },
        'companions': {
            'engine-rust/src/storage/repo_write.rs': {
                'commit': '371fe475ad72fb518e5c6650fc5ebb022d7d1c10',
                'sha256': '4246e16cd7945415174cafd1f7e2b45f98a40258a5f9d9a7779b95d5be55a9d6',
            },
            'engine-rust/src/storage/dashboard.rs': {
                'commit': '371fe475ad72fb518e5c6650fc5ebb022d7d1c10',
                'sha256': 'dff94396936796a18cbd6e2b4282a2bf6afa658df259422d6d38da733e4b6e67',
            },
            'engine-rust/src/storage/pool.rs': {
                'commit': '371fe475ad72fb518e5c6650fc5ebb022d7d1c10',
                'sha256': '2693fea8b2b4b7a6897072ee31a83342792c06853f3a70e0a7d401e77601df79',
            },
        },
    },
    {
        'target': 'config_parity',
        'test': 'fallible_settings_reads_preserve_original_utf8_refusal_and_cache',
        'symptom': 'Fallible settings read admission differs',
        'commit': '7b49a201b2d9d552b2f2aa228ee331dce1610f44',
        'files': {
            'engine-rust/src/storage/config.rs':
                '45013f4fffc3dfa0d96f0e4c115ad57c5c56105564fc97cd461158c822e20612',
        },
    },
    {
        'target': 'folder_parity',
        'test': 'json_filename_selection_matches_original_storage_consumers',
        'symptom': 'JSON filename selection differs',
        'commit': '7b49a201b2d9d552b2f2aa228ee331dce1610f44',
        'files': {
            'engine-rust/src/storage/folders.rs':
                'c640ac042356cbf9042d13364c63fc203043be2c0ff390044fe0702db1687f33',
            'engine-rust/src/storage/dashboard.rs':
                'fe9c2fe120bdf2b7db8865804d81b8c40308a7c466ab6bb90f3ee07997fc3a3f',
            'engine-rust/src/storage/first_run.rs':
                '0727f60e4856dbf6771d971f80ff1815e49290b0feeb2d412386e32832cff65d',
            'engine-rust/src/vault/completion.rs':
                '688be8a375ba1300ff09752a7da5157f664ffb5cf439c2a9ce54ceacd9ef72fa',
        },
    },
    {
        'target': 'rotation_recovery_parity',
        'test': 'committed_rotation_metadata_matches_original_without_rewriting_generation',
        'symptom': 'Committed rotation metadata differs',
        'commit': '0147a3cafbab1c0912484db576bc5b5169b2dc75',
        'files': {
            'engine-rust/src/vault/rotation_recovery.rs':
                '113218c52d07a1af318ee0f05d073443334159612f561a335e66523f12738837',
        },
    },
    {
        'target': 'launch_http_parity',
        'test': 'scheduler_rows::skipped_malformed_schedule_ids_do_not_abort_later_occurrences',
        'symptom': 'Scheduler row isolation differs',
        'commit': '7d8108a31551330c34a7d183bb04335422b0bcb0',
        'files': {
            'engine-rust/src/http/scheduler.rs':
                '3e0e0e7fc208acc286c5006d15d172658df9e08078192a19c7c42e62c39d4a0c',
        },
    },
    {
        'target': 'command_capture_lifecycle',
        'test': 'bounded_capture_drains_both_pipes_and_settles_descendants_on_every_exit',
        'symptom': 'Missing timeout observation',
        'commit': 'b9d7399732c140d6141f37633486fffd90a2e30a',
        'files': {
            'engine-rust/src/ownership/capture.rs':
                'aa93ca78468acf0874bf58cf6e1fbd3ca70c7990146ce36e92ff4c3285c51e54',
        },
    },
    {
        'target': 'schedule_store_parity',
        'test': 'schedule_text_admission_matches_original_without_replacing_bytes',
        'symptom': 'Schedule text admission differs',
        'commit': 'b9d7399732c140d6141f37633486fffd90a2e30a',
        'files': {
            'engine-rust/src/storage/schedules.rs':
                '749856bedd3df0be79cbd3853014fc75d24bb8b9320dcc9a09c32747ccf47527',
        },
        'companions': {
            'engine-rust/tests/schedule_store_parity.rs': {
                'commit': '93db8f8b93d8b8e961bc548a3174da560c16c0b3',
                'sha256': '0419aa623eb9ec63e9d442af7e683756e7d4e04436c56ef0537691c6386504a4',
            },
            'engine-rust/src/cli/schedule_commands.rs': {
                'commit': '93db8f8b93d8b8e961bc548a3174da560c16c0b3',
                'sha256': '8ecf3e0a949f9eedef112c9c49b7661468d167072dc7230fdeb6ff9ae7f72b52',
            },
        },
    },
    {
        'target': 'validation_parity',
        'test': 'ascii_refusal_representation_matches_original_without_exposing_secret_values',
        'symptom': 'ASCII refusal representation differs',
        'commit': '65f5dfd15f2a4b6cc7fd375e858a76b153c83862',
        'files': {
            'engine-rust/src/validation.rs':
                'd505c76357437baee6a8ef76d7626f099adbf59b8f2829cd8b4dfc73c1e8d4e6',
        },
    },
    {
        'target': 'profile_read_parity',
        'test': 'read_view::legacy_read_views_match_original_core_http_and_name_admission',
        'symptom': 'Legacy HTTP read view differs',
        'commit': '65f5dfd15f2a4b6cc7fd375e858a76b153c83862',
        'files': {
            'engine-rust/src/http/profile_routes.rs':
                '9c3fcf9caf0bc6fa97ba48b62341b527fb1bc6f017e3859720a257b8d88bd202',
        },
    },
)

# These sources use current caller signatures and must follow compatibility overlays.
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
        'target': 'repo_batch_parity',
        'test': 'complete_batch_refusals_match_original_index_order_and_preserved_bytes',
        'symptom': 'Complete indexed batch refusal differs',
        'commit': 'd0d81cc6c173c302520f107061f53b57ff0ce885',
        'files': {
            'engine-rust/src/storage/repo_write.rs':
                'ce4d904503fd52f828896f9abd840e76aee1e0131f292064fd99a875a864c53f',
        },
    },
)
