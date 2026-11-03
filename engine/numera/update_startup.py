"""Finite startup exclusion and read-only recovery preflight before store creation."""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import os
import secrets

from .paths import transaction_lock
from .update_admission import Admission, UpdateBusy, _read


def _refuse(reason):
    raise UpdateBusy('update recovery startup refused: ' + reason
                     + '; finish update rollback/recovery before normal startup')


def _preflight(admission, *, first_import):
    if admission.gate.parent.is_symlink() or not admission.gate.parent.is_dir():
        _refuse('the durable admission directory is unsafe')
    ticket = os.environ.get('NUMERA_UPDATE_TICKET') or ''
    if len(ticket) != 64 or any(c not in '0123456789abcdef' for c in ticket):
        _refuse('the matching private recovery ticket is required')
    record = _read(admission.gate)
    expected = record.get('ticket_sha256')
    if (record.get('schema') != 1 or record.get('state') != 'quiesced'
            or not isinstance(expected, str)
            or not secrets.compare_digest(expected.encode('utf-8'),
                hashlib.sha256(ticket.encode('ascii')).hexdigest().encode('ascii'))):
        _refuse('the recovery ticket does not match the durable admission gate')
    root = admission.root
    for name in ('profiles', 'user-data', 'dashboard'):
        path = root / name
        if path.is_symlink() or not path.is_dir():
            _refuse(f'required existing {name} directory is missing or linked')
    # Even a nominally completed journal can cause constructor recovery work.
    # Never classify it as safe merely because its process is still alive.
    journal = root / 'vault-migration.json'
    if journal.exists() or journal.is_symlink():
        _refuse('vault-migration.json requires an independent vault recovery')
    if first_import:
        marker = root / '.imported'
        if marker.is_symlink() or not marker.is_file():
            _refuse('legacy import has not completed; .imported is missing or linked')
    from .server.stores import TokenStore
    from .server import auth_policy
    tokens = TokenStore(root)
    if (auth_policy.read(root) == auth_policy.ABSENT
            and tokens.state() in ('tokens', 'unreadable')):
        _refuse('existing API tokens still require policy adoption')


@contextmanager
def startup(root, *, first_import=False):
    """Yield recovery mode while mutation-safe startup owns its exclusion.

    Normal startup reserves finite work, not the whole service lifetime. A
    recovery startup holds the same root transaction during its read-only
    preflight and constructors, so no ordinary opener slips between them.
    """
    admission = Admission(root)
    with transaction_lock(admission.root):
        closed = (admission.gate.exists() or admission.gate.is_symlink()
                  or admission.gate.parent.is_symlink())
        if closed:
            _preflight(admission, first_import=first_import)
            yield True
            return
        work = admission.work('application startup')
        work.__enter__()
    try:
        yield False
    finally:
        work.__exit__(None, None, None)
