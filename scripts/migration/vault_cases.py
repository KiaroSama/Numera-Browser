"""Deterministic non-secret source ciphertexts for the vault interoperability seam."""
from __future__ import annotations

import base64
from numera import crypto
from cryptography.fernet import Fernet


def cases() -> list[dict]:
    result = []
    password = 'fixture-only-password'
    salt = base64.b64encode(bytes(range(16))).decode('ascii')
    for version, rounds in crypto.ROUNDS_BY_VERSION.items():
        cipher = Fernet(crypto._derive_key(password, salt, rounds))
        for plaintext in ('', 'fixture-value', 'Unicode α فارسی', 'numera-vault-ok'):
            # Fixed IV/time is permitted ONLY for source test fixtures, never
            # production encryption. Target encryption keeps OS randomness.
            token = cipher._encrypt_from_parts(plaintext.encode('utf-8'), 1700000000, bytes(range(16)))
            value = f'enc:{version}:' + token.decode('ascii')
            result.append({'name': f'{version}-{len(result)}', 'password': password,
                           'salt': salt, 'token': value, 'expected': crypto.decrypt(password,salt,value)})
            try:
                crypto.decrypt('wrong-fixture-password', salt, value)
            except crypto.VaultLocked as error:
                result.append({'name': f'wrong-key-{len(result)}', 'password': 'wrong-fixture-password',
                               'salt': salt, 'token': value, 'error': str(error)})
            else:
                raise AssertionError('Source accepted incorrect fixture key')
    try:
        crypto.decrypt(password, salt, 'enc:v9:opaque-fixture')
    except crypto.VaultLocked as error:
        result.append({'name': 'future-generation', 'password': password, 'salt':salt,
                       'token':'enc:v9:opaque-fixture', 'error': str(error)})
    else:
        raise AssertionError('Source accepted unsupported fixture generation')
    return result
