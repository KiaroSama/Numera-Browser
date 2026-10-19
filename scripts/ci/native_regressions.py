"""Prove current native regressions detect their exact historical production defects."""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import logging
import os
from pathlib import Path
import re
import subprocess
import sys

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
)


def git(root: Path, *arguments: str) -> bytes:
    return subprocess.run(
        ['git', '-C', str(root), *arguments], stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=True,
        env={**os.environ, 'GIT_NO_REPLACE_OBJECTS': '1'},
    ).stdout


def require_hosted_source(root: Path) -> str:
    sha = os.environ.get('GITHUB_SHA', '')
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('GITHUB_REPOSITORY') != 'KiaroSama/Numera-Browser'
            or not re.fullmatch('[0-9a-f]{40}', sha)
            or git(root, 'rev-parse', 'HEAD').decode('ascii').strip() != sha):
        raise ValueError('Historical regression execution requires the exact owning hosted source')
    files = [name for case in REGRESSIONS for name in case['files']]
    git(root, 'diff', '--exit-code', 'HEAD', '--', *files)
    return sha


@contextmanager
def historical_sources(root: Path, case: dict):
    originals, replacements = {}, {}
    for name, expected in case['files'].items():
        raw = git(root, 'show', f"{case['commit']}:{name}")
        if len(raw) > 32768 or hashlib.sha256(raw).hexdigest() != expected:
            raise ValueError('Historical regression source integrity differs')
        raw.decode('utf-8', errors='strict')
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError('Historical regression source is not a regular maintained file')
        originals[path] = path.read_bytes()
        replacements[path] = raw
    try:
        for path, raw in replacements.items():
            path.write_bytes(raw)
        yield
    finally:
        failed = []
        for path, raw in originals.items():
            try:
                path.write_bytes(raw)
                if path.read_bytes() != raw:
                    failed.append(path.name)
            except OSError:
                failed.append(path.name)
        if failed:
            raise RuntimeError('Fixed native source restoration failed: ' + ', '.join(failed))


class FailureEvidence(logging.Handler):
    def __init__(self, case: dict):
        super().__init__()
        self.case = case
        self.started = self.named = self.symptom = self.summary = False

    def emit(self, record):
        text = record.getMessage()
        self.started |= text == 'running 1 test'
        self.named |= text == f"test {self.case['test']} ... FAILED"
        self.symptom |= self.case['symptom'] in text
        self.summary |= bool(re.fullmatch(
            r'test result: FAILED\. 0 passed; 1 failed; 0 ignored; 0 measured; '
            r'\d+ filtered out; finished in [0-9.]+s', text))

    def verify(self, code: int) -> None:
        if code != 101 or not all((self.started, self.named, self.symptom, self.summary)):
            raise ValueError('Historical native regression did not fail at its required production assertion: '
                             + self.case['target'])


def prove(root: Path, original: Path, logger) -> None:
    from run_owned import run
    from baseline_source import ENVIRONMENT

    sha = require_hosted_source(root)
    keys = (ENVIRONMENT, 'PYTHONDONTWRITEBYTECODE')
    previous = {key: os.environ.get(key) for key in keys}
    os.environ.update({ENVIRONMENT: str(original), 'PYTHONDONTWRITEBYTECODE': '1'})
    try:
        applicable = (case for case in REGRESSIONS
                      if case.get('platform', sys.platform) == sys.platform)
        for index, case in enumerate(applicable):
            evidence = FailureEvidence(case)
            logger.addHandler(evidence)
            try:
                with historical_sources(root, case):
                    if index == 0:
                        # Cold dependency compilation belongs to the outer build budget.
                        subprocess.run(['cargo', 'test', '--manifest-path', str(root / 'engine-rust/Cargo.toml'),
                                        '--locked', '--offline', '--test', case['target'], '--no-run'],
                                       stdin=subprocess.DEVNULL, check=True)
                    command = ['cargo', 'test', '--manifest-path', str(root / 'engine-rust/Cargo.toml'),
                               '--locked', '--offline', '--test', case['target'], '--', case['test'],
                               '--exact', '--test-threads=1', '--color=never']
                    code = run(command, 180, 90, logger)
                evidence.verify(code)
            finally:
                logger.removeHandler(evidence)
                evidence.close()
            logger.info('Historical regression caught target=%s test=%s source_sha=%s; fixed bytes restored',
                        case['target'], case['test'], sha)
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
