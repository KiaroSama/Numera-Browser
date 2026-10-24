"""Keep infallible historical settings callers coherent during source mutation proofs."""

COMMIT = '6c8a5bbc56c32ebf611d286422df3a94837abc7a'
SOURCES = {
    'engine-rust/src/storage/config.rs':
        'ba88fb20a4e5ac3ee40770330cde6e56c9a95fbbb94a1c51d2f0a29143652ade',
    'engine-rust/src/cli/application.rs':
        '071011af6da97ac45c80d8c917f78875d3a1e936097954dea8f6ed04718e90ae',
    'engine-rust/src/cli/bulk_commands.rs':
        'feb188e5b7408beedcf582f96e375008a87cf7d636fcb970ca7ad66b87e434ba',
    'engine-rust/src/cli/extension_commands.rs':
        '6b0583600cc710ed7aa0310b18e1109718ba5d1a1c6a8b2cce669bce9cf0e998',
    'engine-rust/src/cli/profile_commands.rs':
        '06aa261e8f4ba4c13d6f5ef12191bb81ae769a41deb2dfc4fe6c1bb0a345b5d8',
    'engine-rust/src/cli/settings_commands.rs':
        'b061d6a64dd454340956b51273f09b75a123b4889f35e34cef1d99b073a34b1b',
    'engine-rust/src/cli/vault_commands.rs':
        '553f6a4d6235ebaf221706765950065e2e090903286ab11f513341fd75e0cd37',
    'engine-rust/src/http/attach_routes.rs':
        'e2e6a8c163014808083de6feb5d8e0fe58c5a05cd100c3a96db2aae6f311aac3',
    'engine-rust/src/http/config_routes.rs':
        '48a2b0fc17097d0eb0fbc48342d00ab77ca51367c41b38e0d2b6dab7ccd4c16c',
    'engine-rust/src/http/extension_routes.rs':
        '41058fee6b7ad5cdae490e9273c224d2a827912813e44ee7ef4fc4fbcd0a78a9',
    'engine-rust/src/http/launch_routes.rs':
        '53c5d30e1091bfb20b133c1f7cdf8eb29b5cda666fe12c3a752897f4cb132a80',
    'engine-rust/src/http/profile_batch.rs':
        'd575b7eb3ed2e90ecd42177c91a823c8f3f12718d70dea8b6df223acdca08ef5',
    'engine-rust/src/http/profile_import.rs':
        'b43997955f5d1a76061d8d1d24dd2ff1ec33aeb881d138630929919728ba12b6',
    'engine-rust/src/http/profile_write.rs':
        'c93e11f085e07f22c9468af7ca06de6f66f4fdcde79808d65dcf664d5b03feb2',
    'engine-rust/tests/config_parity.rs':
        'a13add91f09aca8457fee8f40e50e2bc999808f9282c0a068a51a395afd24574',
    'engine-rust/tests/cli_profile_parity.rs':
        '656a8fc05ffd03f89914c586cb0fa2bc66ffde8c7f23d292269a4c702f904fe9',
    'engine-rust/tests/integer_seed_parity.rs':
        'a00dffeec7f9fc93972b58cc4696958b5f4f7d6a2e37b7b90fe708a7365107cb',
    'engine-rust/tests/lock_interop_parity.rs':
        '48b2c974fbf8761af26708db181fe5fae3e974216dbb6a73bc68a95a538b3b84',
}


def compatible(cases: tuple, storage_readers: dict) -> tuple:
    """Fill missing siblings only; selected defects and prior companions always win."""
    result = []
    for case in cases:
        touched = case['files'].keys() | case.get('companions', {}).keys()
        if touched & (SOURCES.keys() | storage_readers.keys()):
            case = dict(case, companions={
                **{name: {'commit': COMMIT, 'sha256': expected}
                   for name, expected in SOURCES.items() if name not in case['files']},
                **case.get('companions', {}),
            })
        result.append(case)
    return tuple(result) + ({
        'target': 'cli_settings_parity',
        'test': 'strict_settings::settings_commands_preserve_original_decoding_refusals_and_early_errors',
        'symptom': 'Strict settings CLI admission differs',
        'commit': COMMIT,
        'files': SOURCES,
    }, {
        'target': 'config_parity',
        'test': 'extension_admission::extension_setter_preserves_original_stored_set_admission_and_cache',
        'symptom': 'Stored extension set admission differs',
        'commit': 'b8e589f28e57e5734be2bacbd000bb11cc454ba2',
        'files': {
            'engine-rust/src/storage/config.rs':
                '2c037f3fcb63a1d210c84437a72285a371a9c1fe89d85d510fe3c649142c406f',
        },
    })
