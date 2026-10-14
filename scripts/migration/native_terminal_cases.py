"""Shared source-derived vault scenarios for actual PTY and ConPTY entries."""
from __future__ import annotations
import json
from pathlib import Path


def followups(parent, executable, label, env, execute, newline, log):
    from numera import crypto

    root = Path(__file__).resolve().parents[2]
    case = json.loads((root / 'engine-rust/tests/fixtures/rotation-recovery.json').read_text(encoding='utf-8'))['cases'][0]
    old, new = case['old_password'], case['password']
    for verb in ('change', 'resume'):
        store = parent / f'{label}-{verb}'
        store.mkdir()
        files = {'config.json': case['config'], 'proxies.json': case['pool'] if verb == 'resume' else case['pool'][:1]}
        if verb == 'resume':
            files['vault-migration.json'] = case['journal']
        for name, value in files.items():
            (store / name).write_text(json.dumps(value), encoding='utf-8')
        stages = [('Master password:', old + newline),
                  ('New master password:' if verb == 'change' else 'The password the rotation was moving TO:', new + newline)]
        if verb == 'change':
            stages.append(('Confirm new master password:', new + newline))
        code, text = execute([*executable, '--data-dir', str(store), 'vault', verb], env, stages, wall=90, idle=60)
        assert code == 0, (label, verb, code)
        assert old not in text and new not in text, 'Terminal echoed rotation keys'
        assert not (store / 'vault-migration.json').exists(), 'Terminal rotation retained its journal'
        config = json.loads((store / 'config.json').read_text(encoding='utf-8'))
        descriptor = config['vault']
        assert crypto.check_password(new, descriptor['salt'], descriptor['verifier'])
        assert not crypto.check_password(old, descriptor['salt'], descriptor['verifier'])
        pool = json.loads((store / 'proxies.json').read_text(encoding='utf-8'))
        observed = [crypto.decrypt(new, descriptor['salt'], record['pass']) for record in pool]
        expected = case['observed'] if verb == 'resume' else case['observed'][:1]
        assert observed == [record['pass'] for record in expected]
        if verb == 'resume':
            assert config == case['persisted'], 'Resume changed unrelated source config'
        else:
            assert config['future'] == case['config']['future']
            assert 'Master password changed' in text
        log.info('Terminal rotation keys/no-echo/secret readback passed runtime=%s verb=%s', label, verb)

    store = parent / f'{label}-cancelled'
    code, text = execute([*executable, '--data-dir', str(store), 'vault', 'enable'], env,
                         [('Master password:', '\x03')], wall=15, idle=5)
    assert code == 130, (label, 'cancelled', code)
    assert 'Aborted.' in text, 'Terminal cancellation did not report its verdict'
    assert not (store / 'vault-migration.json').exists()
    assert not (store / 'config.json').exists(), 'Cancelled prompt created a vault descriptor'
    log.info('Terminal Ctrl-C cancelled without vault writes runtime=%s', label)

    for verb in ('enable', 'change'):
        store = parent / f'{label}-damaged-{verb}'
        store.mkdir()
        journal = store / 'vault-migration.json'
        journal.write_bytes(b'{')
        code, text = execute([*executable, '--data-dir', str(store), 'vault', verb], env, [], wall=15, idle=5)
        assert code == 1, (label, verb, code)
        assert 'Master password:' not in text and 'New master password:' not in text, 'Damaged journal prompted before refusal'
        assert journal.read_bytes() == b'{', 'Refusal changed the damaged journal'
        assert not (store / 'config.json').exists(), 'Refusal created a vault descriptor'
        log.info('Damaged journal refused before terminal prompt runtime=%s verb=%s', label, verb)
