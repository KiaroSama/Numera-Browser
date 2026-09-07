"""`<root>/config.json` - the store's small key/value settings.

The default engine, the default extension set, the headless-launch flag, the
folder list, and the vault descriptor that says whether secrets are encrypted
and with which salt. One file, read by everything and written by very little.

It lives apart from `store.py` because it is a different file with a different
lifecycle: nothing here reads or writes a profile record, and nothing in the
profile store needs to know how a setting is stored. What they do share is the
data root and the lock hierarchy - root transaction first, then the file's own
lock - so this is a base class of `ProfileStore` rather than a separate object
to pass around. Every caller already reaches these through the store
(`store.get_default_engine()`), and turning that into `store.config.get(...)`
would be churn for no one's benefit.

The subclass provides `root` and `config_path`; nothing else is assumed.
"""

from __future__ import annotations

import json
import time
from typing import Optional

from . import vault
from .paths import _lock_for, mutate_json, transaction_lock, write_json


class ConfigStore:
    """The settings half of a data root. See the module docstring."""

    root: "object"
    config_path: "object"

    # ---- config (small key/value settings) -------------------------------
    # Preference order for a fresh install, strongest first. `cloak` is NOT in
    # it: CloakBrowser needs a paid subscription and cannot be bundled, so
    # defaulting to it meant the very first launch click failed on every new
    # install. An explicit user choice still wins over this list.
    ENGINE_PREFERENCE = ("camoufox", "patchright", "playwright")
    DEFAULT_ENGINE = "playwright"

    def get_config(self) -> dict:
        """The stored config, re-read only when the file has actually changed.

        `vault()` and `unlocked()` both call this, and `vault.decrypt_for_read`
        calls both - so decrypting a single secret cost two full reads of
        `config.json`. Listing a 60-proxy pool did over a hundred, which is
        where a pool sweep's quadratic cost really came from; the PBKDF2 work
        was only half of it.

        Keyed on the file's size and modification time. Writes go through
        `write_json`, which replaces the file atomically, so any change moves
        one of those. A copy is returned because callers mutate what they get.
        """
        import copy

        try:
            stat = self.config_path.stat()
        except OSError:
            self._config_cache = None
            return {}
        key = (stat.st_mtime_ns, stat.st_size)
        cached = getattr(self, "_config_cache", None)
        if cached is not None and cached[0] == key:
            return copy.deepcopy(cached[1])
        try:
            data = json.loads(self.config_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}
        if not isinstance(data, dict):
            return {}
        self._config_cache = (key, data)
        return copy.deepcopy(data)

    def save_config(self, cfg: dict) -> dict:
        """Replace the whole config, under the hierarchy the rest of this obeys.

        Root transaction first, then the file lock - the order stated at the
        top of this class and followed by `set_config_many` and `_set_config`.
        This one took the file lock alone, which put it in its own lock domain:
        a profile-and-config transaction and an outside `save_config()` never
        waited for each other, so a rolled-back transaction could restore
        `config.json` over a settings change that had already been
        acknowledged. That is the exact bug the hierarchy was introduced to
        close, left open on the one public entry point that skipped it.

        The root lock is reentrant, so the vault callers already inside a
        transaction pay nothing for it.
        """
        with transaction_lock(self.root):
            with _lock_for(self.config_path):
                write_json(self.config_path, cfg)
        return cfg

    def get_default_engine(self) -> str:
        """The engine used for new profiles when none is given (persisted).

        An explicitly configured engine is returned as-is even when its package
        is missing - silently substituting another one would be the same class
        of bug as a UI control that does nothing, and the launcher already
        raises a clear "needs the 'x' package" error. The preference order only
        decides what a *fresh* install gets."""
        configured = self.get_config().get("default_engine")
        if configured:
            return configured
        from . import drivers  # local: keeps store importable without a driver stack
        for name in self.ENGINE_PREFERENCE:
            if drivers.is_installed(name):
                return name
        return self.DEFAULT_ENGINE

    def set_default_engine(self, name: str) -> str:
        # Read and write inside one lock. Reading outside it meant two settings
        # saved at once each wrote their own copy of the whole config, and the
        # second one silently dropped the first one's change.
        self._set_config("default_engine", name)
        return name

    def get_default_extensions(self) -> list:
        """Extension folders every new profile starts with.

        Chromium loads extensions per browser instance, and each profile is its
        own instance, so there is no "install once, applies everywhere" step to
        hook — a global set has to be copied onto each profile as it is created.
        """
        return list(self.get_config().get("default_extensions") or [])

    def set_default_extensions(self, paths) -> list:
        """Persist the set every new profile starts with, validated.

        Only entries that are *new* are checked. Validating the whole list
        would make an already-stored bad path - one whose folder was deleted
        after it was added - block the very call that removes it.
        """
        # Lazily, and from `store`: the validator lives beside the profile
        # code that also uses it, and importing it at module level here would
        # close a ring between the two modules.
        from .store import unloadable_extensions

        wanted = [str(p) for p in paths]
        with transaction_lock(self.root):
            current = set(self.get_default_extensions())
            problems = unloadable_extensions([p for p in wanted if p not in current])
            if problems:
                raise ValueError("; ".join(problems))
            return self._set_config("default_extensions", wanted)

    def get_headless_launch(self) -> bool:
        """Whether the API launches profiles without a window.

        A GUI-less server has no display for Chromium to open a window on, so
        the dashboard's launch button needs this on to work there at all.
        """
        return bool(self.get_config().get("headless_launch"))

    def set_headless_launch(self, on: bool) -> bool:
        return self._set_config("headless_launch", bool(on))

    def set_config_many(self, changes: dict) -> dict:
        """Apply several config keys as one read-modify-write.

        `PUT /api/config` used to call `set_default_engine`, then
        `set_default_extensions`, then `set_headless_launch` in turn - three
        separate writes. A request whose *third* field was invalid had already
        committed the first two and then answered 400, so a rejected request
        still changed the config. One write, after everything is validated.
        """
        if not changes:
            return self.get_config()

        def change(cfg):
            cfg.update(changes)
            return cfg

        # Root transaction lock first, then the config file lock `mutate_json`
        # takes. Config used to be its own lock domain: a profile transaction
        # wrote `config.json` while holding only the transaction lock, and an
        # ordinary settings change held only the file lock - so neither waited
        # for the other, and a rollback could restore config over a settings
        # change that had already succeeded.
        with transaction_lock(self.root):
            return mutate_json(self.config_path, change)

    def _set_config(self, key: str, value):
        """Change one config key without disturbing the others."""
        def change(cfg):
            cfg[key] = value
            return cfg

        # Same hierarchy as `set_config_many`; see the note there.
        with transaction_lock(self.root):
            return mutate_json(self.config_path, change)[key]
