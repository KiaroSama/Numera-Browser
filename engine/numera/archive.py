"""Moving a profile between this store and a file.

Four operations, one job: get an identity out of the store and back in again.
`export`/`import_file` carry the profile *config* - one JSON file, portable and
diffable. `backup`/`restore` carry the whole identity: the config **and** the
browser session under `user-data/`, zipped, which is what makes a warmed-up
profile transferable to another machine.

They live apart from `ProfileStore` because they are the only code here that
speaks zip, staging directories and rollback, and because none of them is a
store primitive - each is built out of the store's own `get`/`save` surface and
the profile coordinator. `ProfileStore.export/import_file/backup/restore` stay
as the public entry points and call straight through, so no caller changes.

Two rules run through all four:

* **Nothing leaves that cannot be read in full.** A locked vault yields
  `proxy.password = None`, so an export or a backup taken while locked would
  drop the credential silently and come back a working-looking profile that
  cannot reach its proxy. Both refuse instead.
* **Validate and stage before touching anything.** An archive is untrusted - it
  may have come from another machine, or from someone else entirely - so it is
  read, checked and unpacked into a staging directory first, and the profile's
  real session is only replaced once there is something complete to replace it
  with.
"""

from __future__ import annotations

import json
import shutil
import tempfile
import uuid
import zipfile
from pathlib import Path
from typing import Optional

from . import crypto
from . import vault
from .models import Profile
from .paths import safe_join, session_lease, transaction_lock
from .store import SessionStranded, name_clash, stored_names


def export(store, profile_id: str, dest: str | Path) -> Optional[Path]:
    prof = store.get(profile_id)
    if not prof:
        return None
    # An export of a profile whose password we cannot read would omit it
    # silently, and the file gives no hint that anything is missing.
    if store.secret_withheld(profile_id):
        raise crypto.VaultLocked(
            f"exporting '{prof.name}' would drop its proxy password: "
            + vault.LOCKED_MESSAGE)
    dest = Path(dest)
    dest.write_text(json.dumps(prof.to_dict(), indent=2), encoding="utf-8")
    return dest

def import_file(store, src: str | Path) -> Profile:
    """Import one exported profile, into BOTH copies.

    An existing id used to update only the engine record, leaving the
    dashboard describing the identity that was replaced - two copies under
    one id telling different stories.

    Under the name check, like every other way a profile enters the store. It
    was exempt, so the one contract every surface is supposed to share had a
    hole in it that `numera import` walked straight through: an export whose
    name a different profile already holds landed as a second profile with that
    name, and `numera delete <name>` then destroyed whichever the directory
    listed first. Re-importing over the *same* id is still fine - the check
    refuses a name another id holds, not a profile keeping its own.
    """
    from .repo import repo

    data = json.loads(Path(src).read_text(encoding="utf-8"))
    prof = Profile.from_dict(data)
    return repo(store).create(prof)

# ---- backup / restore ------------------------------------------------
# ``export``/``import_file`` move only the profile *config* (a JSON file).
# A backup is the whole identity: the config **and** its browser session
# (cookies, localStorage — everything under user-data), zipped into one
# portable file you can archive or carry to another machine.
def backup(store, profile_id: str, dest: str | Path) -> Optional[Path]:
    """Zip a profile and its entire browser session into one file.

    The profile JSON inside the archive holds any proxy password in
    *plaintext* (secrets are decrypted on read), so treat a backup file as
    sensitive — it grants everything the profile does. Returns the path, or
    ``None`` if no such profile exists."""
    # A live Chromium rewrites its databases continuously; zipping one
    # mid-write produces an archive that restores into a corrupt profile,
    # with nothing at backup time to say so.
    with session_lease(store.root, profile_id):
        prof = store.get(profile_id)
        if not prof:
            return None
        # The docstring above promises the archive carries the proxy password.
        # While locked it would not, and nothing in the file would say so - the
        # restore would come back a working-looking profile that cannot reach
        # its proxy.
        if store.secret_withheld(profile_id):
            raise crypto.VaultLocked(
                f"backing up '{prof.name}' would drop its proxy password: "
                + vault.LOCKED_MESSAGE)
        dest = Path(dest)
        with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("profile.json", json.dumps(prof.to_dict(), indent=2))
            ud = store.userdata_dir / profile_id
            if ud.exists():
                for f in ud.rglob("*"):
                    if f.is_file():
                        z.write(f, arcname=str(Path("user-data") / f.relative_to(ud)))
        return dest

def _unclaimed_name(coordinator, prof: Profile) -> str:
    """`prof.name`, or the first `<name> (copy N)` no other profile holds.

    One name, one profile - and a restore is the one write that used to be
    allowed to break it. `restore --new-id` is a *clone*: the original is still
    there under the name in the archive, so keeping it produced two profiles
    the CLI could not tell apart, and `numera delete <name>` then destroyed one
    of them at random. The tests even documented that state as legitimate.

    A restore over the same id is untouched by this: the name it is asking for
    is the one it already holds, so nothing else can hold it. Only a genuine
    second claimant is renamed, and the caller is told the name it got back.

    Bounded by the number of stored records: at most that many names are taken,
    so one of that many plus one candidates is always free.
    """
    if not prof.name:
        return prof.name
    stored = stored_names(coordinator.store.profiles_dir, coordinator.dash_dir)
    if not name_clash(stored, [(prof.id, prof.name)]):
        return prof.name
    for n in range(1, len(stored) + 2):
        candidate = f"{prof.name} (copy)" if n == 1 else f"{prof.name} (copy {n})"
        if not name_clash(stored, [(prof.id, candidate)]):
            return candidate
    raise AssertionError("unreachable: more names taken than records stored")


def _put_the_session_back(ud: Path, aside, failure: BaseException) -> None:
    """Undo this restore's own change to `user-data/<id>` - and nothing else.

    The metadata is deliberately not touched here. It is written through the
    repository's own `write_batch`, which restores both records when its commit
    fails, and the version before this one stacked a second rollback on top of
    that: it snapshotted both metadata files *before* taking the session lease,
    then wrote those bytes back on any failure. Metadata edits need no lease, so
    an API or CLI edit committed while a large archive was still unpacking - and
    a restore that then failed overwrote that acknowledged edit with a snapshot
    taken minutes earlier. Restoring only what this restore actually changed is
    the fix; the transaction it now runs inside is what keeps the window shut.

    Raises `SessionStranded` naming both directories if the session cannot be
    put back, because "the profile is as it was but its browser session is
    under a name nothing points at" is the one outcome a caller cannot act on
    from the original error alone.
    """
    shutil.rmtree(ud, ignore_errors=True)
    if aside is None:
        return
    try:
        aside.rename(ud)
    except OSError as e:
        raise SessionStranded(
            f"the restore failed ({type(failure).__name__}: {failure}) and the "
            f"previous session could not be put back: it is still on disk as "
            f"{aside}. Move that directory to {ud} to finish recovering."
        ) from failure


def restore(store, src: str | Path, new_id: bool = False) -> Profile:
    """Recreate a profile and its session from a :meth:`backup` archive.

    By default the profile keeps its original id, overwriting any profile
    already under that id (a true restore). Pass ``new_id=True`` to bring it
    in as a fresh copy instead — useful for cloning a warmed-up session
    onto a second machine without the two fighting over one id.

    The order is the whole design, and each step is where it is on purpose:

    1. **Validate and unpack into staging**, before any lock. Nothing live is
       touched, so a hostile or truncated archive fails having changed nothing
       - and the session lease is not held for however long a multi-gigabyte
       archive takes to unpack.
    2. **The session lease**, outside the data transaction and never inside it.
       A restore replaces a whole session directory and must refuse immediately
       if another process has that browser open; a lease taken while holding
       the transaction would make every write in the data root wait on a
       human closing a window.
    3. **A short root transaction** around the swap and the metadata commit,
       so the state this restore is about to replace cannot change under it and
       no concurrent writer lands inside its rollback.
    """
    src = Path(src)
    # Everything in the archive is untrusted: it may have come from another
    # machine, or from someone else entirely. Validate the whole thing and
    # stage it before touching a single profile file - the previous version
    # saved the profile first and wrote members as it went, so a hostile or
    # merely truncated archive left a half-made profile behind.
    try:
        zf = zipfile.ZipFile(src, "r")
    except zipfile.BadZipFile as e:
        raise ValueError(f"{src.name} is not a readable archive: {e}") from e

    with zf as z:
        try:
            data = json.loads(z.read("profile.json").decode("utf-8"))
        except KeyError:
            raise ValueError(f"{src.name} has no profile.json - not a Numera backup") from None
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            raise ValueError(f"{src.name} has an unreadable profile.json: {e}") from e
        try:
            prof = Profile.from_dict(data)
        except (KeyError, TypeError, ValueError) as e:
            raise ValueError(f"{src.name} does not contain a usable profile: {e}") from e
        if new_id:
            prof.id = uuid.uuid4().hex[:12]

        from .repo import repo

        coordinator = repo(store)
        # Staging first, and outside every lock: an archive member that escapes
        # `user-data/` raises here, having written only into a temporary
        # directory nothing reads.
        staging = Path(tempfile.mkdtemp(prefix="numera-restore-", dir=store.root))
        try:
            for info in z.infolist():
                name = info.filename
                if not name.startswith("user-data/") or name.endswith("/"):
                    continue
                if info.is_dir():
                    continue
                # Raises before anything is written if the member escapes.
                target = safe_join(staging, name[len("user-data/"):])
                target.parent.mkdir(parents=True, exist_ok=True)
                with z.open(info) as fh:
                    target.write_bytes(fh.read())

            # The lease, now that the archive has named the profile: a restore
            # replaces a whole session directory, and must not replace one
            # another process is using. Outside the transaction, always - see
            # the note in `_SessionLease`.
            with session_lease(store.root, prof.id):
                # And the transaction, held for the swap and the commit only.
                # Everything this restore is about to replace is read inside
                # it, so no concurrent write can land between the decision and
                # the change - which is what let a failing restore undo an edit
                # that had already been acknowledged.
                with transaction_lock(store.root):
                    # Replace, do not merge: a session that is half the old
                    # identity and half the restored one is worse than either.
                    #
                    # Commit order matters, and it used to be wrong. The profile
                    # was saved first, then the old session was removed with
                    # `ignore_errors=True`, then staging was moved in. On Windows
                    # a running Chromium holds its files, so the delete failed
                    # silently - and `shutil.move` onto a directory that still
                    # exists moves the source *inside* it, leaving
                    # `user-data/<id>/numera-restore-xxxx/`. The metadata was
                    # already updated, so the store described a session it did
                    # not have. Now the old session is renamed aside (which fails
                    # loudly if it is in use), the new one is moved into place,
                    # the profile is written last, and any failure puts the
                    # session back.
                    ud = store.user_data_path(prof.id)
                    prof.name = _unclaimed_name(coordinator, prof)
                    aside = None
                    if ud.exists():
                        aside = ud.with_name(f"{ud.name}.replaced-{uuid.uuid4().hex[:8]}")
                        ud.rename(aside)
                    try:
                        ud.parent.mkdir(parents=True, exist_ok=True)
                        shutil.move(str(staging), str(ud))
                        staging = None
                        # One metadata transaction for engine and dashboard.
                        # `save()` wrote only the engine record, so restoring
                        # onto an existing id replaced that identity and left
                        # the dashboard still describing the one it replaced -
                        # two copies under one id telling different stories.
                        # Dashboard-only fields the engine cannot carry (the
                        # folder, the trust stamp) survive through the merge.
                        # Its `write_batch` owns the metadata rollback, which is
                        # why nothing here snapshots those files: a second,
                        # older snapshot layered over it is what used to
                        # overwrite a concurrent edit.
                        prof = coordinator.create(prof)
                    except BaseException as failure:
                        _put_the_session_back(ud, aside, failure)
                        raise
                    if aside is not None:
                        shutil.rmtree(aside, ignore_errors=True)
        finally:
            if staging is not None:
                shutil.rmtree(staging, ignore_errors=True)
        return prof
