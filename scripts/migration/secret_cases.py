"""Source secret read/write intent, using deliberately non-sensitive inputs."""
from numera import vault, crypto


class FixtureVault:
    def __init__(self, enabled: bool, unlocked: bool):
        self._password = 'fixture-master'
        self._enabled, self._unlocked = enabled, unlocked

    def vault_enabled(self):
        return self._enabled

    def unlocked(self):
        return self._unlocked

    def vault(self):
        return {'salt': 'AAECAwQFBgcICQoLDA0ODw=='}


def cases() -> list[dict]:
    found = []
    # Replace only nondeterministic cipher output, not the vault policy being
    # observed. Cipher interoperability has its own source-generated seam.
    original = crypto.encrypt
    crypto.encrypt = lambda password, salt, value: 'fixture-randomized-envelope'
    try:
        for mode in ('absent', 'disabled', 'locked', 'unlocked'):
            engine = None if mode == 'absent' else FixtureVault(mode != 'disabled', mode == 'unlocked')
            for stored in (None, '', 'fixture-old', 'enc:v9:fixture-token'):
                for value in (None, '', 'fixture-new', 'enc:v9:fixture-token'):
                    try:
                        result, error = vault.encrypt_for_write(engine, value, stored), None
                    except crypto.VaultLocked as exc:
                        result, error = None, str(exc)
                    found.append({'mode': mode, 'stored': stored, 'value': value,
                                  'result': result, 'error': error})
    finally:
        crypto.encrypt = original
    return found
