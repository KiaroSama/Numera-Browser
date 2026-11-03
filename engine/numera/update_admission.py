"""Durable update exclusion, serialized with the existing data-root transaction.

The gate is intentionally not a lease: it must survive engine exit while the
external worker replaces files. Only the exact recovery ticket reopens it.
"""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import secrets
import time
import uuid

from . import logs, proctree, sessions
from .paths import SessionBusy, transaction_lock, write_json

GATE = '.updates/admission.json'
_MAX_RECORD = 65536
_MAX_RECORDS = 10000


class UpdateBusy(SessionBusy):
    """An update or active/uncertain work prevents this operation."""


def _read(path: Path) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > _MAX_RECORD:
        raise UpdateBusy('update ownership evidence is unsafe or unreadable')
    try:
        record = json.loads(path.read_text(encoding='utf-8'))
    except (OSError, ValueError) as error:
        raise UpdateBusy('update ownership evidence cannot be read') from error
    if not isinstance(record, dict):
        raise UpdateBusy('update ownership evidence is malformed')
    return record


def _records(directory: Path, ignored_suffixes=()):
    if directory.is_symlink():
        raise UpdateBusy('active work ownership directory is unsafe')
    if not directory.exists():
        return []
    if not directory.is_dir():
        raise UpdateBusy('active work ownership directory is unsafe')
    paths = []
    for path in directory.iterdir():
        if path.is_symlink() or not path.is_file():
            raise UpdateBusy('active work ownership evidence is malformed')
        if path.suffix in ignored_suffixes:
            continue
        if path.suffix != '.json':
            raise UpdateBusy('active work ownership evidence is malformed')
        paths.append(path)
        if len(paths) > _MAX_RECORDS:
            raise UpdateBusy('too many ownership records to prove update quiescence')
    return paths


class Admission:
    def __init__(self, root):
        self.root = Path(root).resolve()
        self.gate = self.root / GATE
        self.work_dir = self.gate.parent / 'work'

    def assert_open(self):
        """Call inside transaction_lock before admitting or claiming work."""
        if (self.gate.parent.is_symlink()
                or (self.gate.parent.exists() and not self.gate.parent.is_dir())
                or self.gate.exists() or self.gate.is_symlink()):
            raise UpdateBusy('an update has closed admission; finish update recovery before retrying')

    @contextmanager
    def work(self, what: str, *, settled=lambda: True):
        """Reserve finite work before spawn; keep uncertainty on abnormal exit.

        Normal exceptions release the reservation, not browser/session claims.
        A killed owner leaves evidence rather than becoming an empty snapshot.
        """
        path = self.work_dir / f'{uuid.uuid4().hex}.json'
        with transaction_lock(self.root):
            self.assert_open()
            if self.work_dir.is_symlink():
                raise UpdateBusy('active work directory is unsafe')
            write_json(path, {'schema': 1, 'pid': os.getpid(),
                             'token': proctree.started_at(os.getpid()), 'what': what})
        try:
            yield
        finally:
            if settled():
                with transaction_lock(self.root):
                    path.unlink(missing_ok=True)

    def _idle(self, local_busy):
        if local_busy():
            raise UpdateBusy('active or uncertain owned work defers this update')
        # No stale-owner auto-prune: a killed helper may have surviving units.
        if _records(self.work_dir):
            raise UpdateBusy('active or uncertain admitted work defers this update')
        for path in _records(self.root / 'sessions'):
            record = _read(path)
            if not record.get('session') or sessions.state(record) != 'free':
                raise UpdateBusy('active or uncertain browser session defers this update')
        for path in _records(self.root / 'jobs', ignored_suffixes=('.log', '.exit')):
            from .jobs import _decorate, _read_record
            record = _read(path)
            tree = proctree.Tree.from_record(record)
            if not tree.verified or not tree.kind or record.get('pid') is None:
                raise UpdateBusy('uncertain job ownership defers this update')
            if _decorate(_read_record(path))['running'] or proctree.alive_tree(tree):
                raise UpdateBusy('active owned job defers this update')

    def quiesce(self, local_busy=lambda: False, ticket=None) -> str:
        ticket = secrets.token_hex(32) if ticket is None else ticket
        if not isinstance(ticket, str) or len(ticket) != 64 or any(c not in '0123456789abcdef' for c in ticket):
            raise UpdateBusy('invalid precommitted update recovery ticket')
        with transaction_lock(self.root):
            self.assert_open()
            write_json(self.gate, {
                'schema': 1, 'state': 'quiesced',
                'ticket_sha256': hashlib.sha256(ticket.encode('ascii')).hexdigest(),
                'owner_pid': os.getpid(), 'owner_token': proctree.started_at(os.getpid()),
                'created_at': time.time(),
            })
            try:
                self._idle(local_busy)
            except BaseException:
                self.gate.unlink()
                raise
            from .vault_recovery import fsync_path
            fsync_path(self.gate)
        logs.get('updates').info('update admission closed after idle proof')
        return ticket

    def resume(self, ticket: str):
        if not isinstance(ticket, str) or len(ticket) != 64 or any(
                c not in '0123456789abcdef' for c in ticket):
            raise UpdateBusy('invalid update recovery ticket')
        with transaction_lock(self.root):
            from .vault_recovery import fsync_path, sync_dir
            actual = hashlib.sha256(ticket.encode('ascii')).hexdigest()
            receipt = self.gate.parent / 'resumed.json'
            if not self.gate.exists() and not self.gate.is_symlink():
                record = _read(receipt) if receipt.exists() or receipt.is_symlink() else {}
                expected = record.get('ticket_sha256')
                if (record.get('schema') != 1 or record.get('state') != 'resumed'
                        or not isinstance(expected, str)
                        or not secrets.compare_digest(expected.encode('utf-8'), actual.encode('ascii'))):
                    raise UpdateBusy('there is no matching update transaction to resume')
                sync_dir(self.gate.parent)
                return
            record = _read(self.gate)
            expected = record.get('ticket_sha256')
            if (record.get('schema') != 1 or record.get('state') != 'quiesced'
                    or not isinstance(expected, str)
                    or not secrets.compare_digest(expected.encode('utf-8'), actual.encode('ascii'))):
                raise UpdateBusy('update recovery ticket does not match the closed transaction')
            # Persist before opening admission: a lost response or shell crash
            # can retry the same transaction, never an unrelated later gate.
            write_json(receipt, {'schema': 1, 'state': 'resumed', 'ticket_sha256': actual})
            fsync_path(receipt)
            self.gate.unlink()
            sync_dir(self.gate.parent)
        logs.get('updates').info('update admission reopened by matching recovery ticket')
