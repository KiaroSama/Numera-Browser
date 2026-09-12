"""Settings, engines, folders and extensions - what configures Numera
rather than a profile."""

from __future__ import annotations

import json
from pathlib import Path
from .. import __version__
from .. import folders
from ..store import unloadable_extensions
from .stores import DuplicateToken
from .convert import _parse_lines


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    engine_store, dash, tokens, pool = ctx.engine_store, ctx.dash, ctx.tokens, ctx.pool

    @app.get("/api/health")
    def health():
        return {"ok": True, "version": __version__}

    @app.get("/api/engines")
    def engines():
        from .. import drivers
        return drivers.available()

    def _folder_list() -> list[str]:
        """Every folder that exists: the ones the user created (persisted in
        meta) plus any a profile currently lives in, de-duplicated, order kept."""
        meta = dash.get_meta()
        used = sorted({p.get("folder") for p in dash.list() if p.get("folder")})
        return list(dict.fromkeys([*meta.get("folders", []), *used]))

    def _persist_folders(names: list[str]) -> list[str]:
        meta = dash.get_meta()
        meta["folders"] = list(dict.fromkeys(n for n in names if n))
        dash.set_meta(meta)
        return _folder_list()

    @app.get("/api/config")
    def get_config():
        return {
            "default_engine": engine_store.get_default_engine(),
            "default_extensions": engine_store.get_default_extensions(),
            "headless_launch": engine_store.get_headless_launch(),
            "folders": _folder_list(),
            "root": str(engine_store.root),
            "version": __version__,
            # How many proxies are *stored*, which is not the same as how many
            # profiles are using one. Settings said "0 proxies" beside a pool of
            # 27 because the only number the dashboard had was the in-use count.
            "proxyCount": len(pool.list()),
        }

    @app.post("/api/extensions/install")
    def install_extension(body: dict):
        """Download a Web Store extension and unpack it into the data directory.

        Chromium cannot install from the store with automation enabled, so the
        crx is fetched from Google's own update endpoint and unpacked - the same
        thing every commercial anti-detect browser does behind a 'paste a link'
        box."""
        from .. import webstore
        try:
            result = webstore.install(body.get("url") or body.get("id") or "",
                                      engine_store.root, force=bool(body.get("force")))
        except webstore.WebStoreError as e:
            raise HTTPException(400, str(e))

        if body.get("addToDefaults", True):
            current = engine_store.get_default_extensions()
            if result["path"] not in current:
                engine_store.set_default_extensions([*current, result["path"]])
        return {**result, "default_extensions": engine_store.get_default_extensions()}

    @app.get("/api/extensions/installed")
    def installed_extensions():
        """Every folder the UI has to draw a row for, named and with its icon.

        Not only the ones fetched from the store: a folder the user unpacked by
        hand has a manifest too, and a row showing a bare path tells them nothing
        about which extension it is.
        """
        from .. import webstore
        seen, out = set(), []
        for e in webstore.installed(engine_store.root):
            seen.add(e["path"])
            out.append(e)
        for entry in engine_store.get_default_extensions():
            path = Path(str(entry)).expanduser()
            if str(path) in seen or "://" in str(entry):
                continue
            if (path / "manifest.json").is_file():
                out.append({"id": path.name, **webstore.describe(path)})
        return {"extensions": out}

    @app.get("/api/extensions/icon")
    def extension_icon(path: str):
        """Serve one extension's own icon.

        Reading an arbitrary path off the filesystem on request would be a hole,
        so the folder has to be one the engine already knows: installed from the
        store, or listed in the default set. Anything else is refused rather than
        read.
        """
        from fastapi.responses import FileResponse

        from .. import webstore
        wanted = Path(path).expanduser().resolve()
        known = {Path(e["path"]).resolve() for e in webstore.installed(engine_store.root)}
        known |= {Path(str(e)).expanduser().resolve()
                  for e in engine_store.get_default_extensions() if "://" not in str(e)}
        if wanted not in known:
            raise HTTPException(404, "not an extension this engine knows about")

        ico = webstore.icon(wanted)
        if not ico:
            raise HTTPException(404, "that extension declares no usable icon")
        return FileResponse(ico)

    # The keys `PUT /api/config` will act on. Everything else `get_config()`
    # returns is derived or read-only (`folders`, `root`, `version`), and a
    # request naming one of those was silently ignored rather than refused.
    SETTABLE = ("default_engine", "default_extensions", "headless_launch")

    def _config_patch(cfg: dict) -> dict:
        """Every field validated, nothing written. Raises on the first problem.

        Separate from the write on purpose. This used to validate and write one
        field at a time, so a request whose *third* field was invalid had
        already committed the first two before answering 400 - a rejected
        request that changed the config anyway.
        """
        from .. import drivers

        if not isinstance(cfg, dict):
            raise HTTPException(400, f"config must be an object, got {type(cfg).__name__}")
        unknown = sorted(set(cfg) - set(SETTABLE))
        if unknown:
            raise HTTPException(400, f"unknown config key(s): {', '.join(unknown)}")

        patch: dict = {}

        name = cfg.get("default_engine")
        if name is not None:
            if name not in drivers.names():
                raise HTTPException(400, f"unknown engine '{name}'")
            patch["default_engine"] = name

        exts = cfg.get("default_extensions")
        if exts is not None:
            if isinstance(exts, str):
                exts = _parse_lines(exts)
            if not isinstance(exts, list):
                raise HTTPException(400, "default_extensions must be a list of folder paths")
            for bad in unloadable_extensions(exts):
                raise HTTPException(400, bad)
            patch["default_extensions"] = [str(x) for x in exts]

        headless = cfg.get("headless_launch")
        if headless is not None:
            # A real boolean. `bool("false")` is True, so the JSON string
            # "false" used to switch headless launching *on* - the opposite of
            # what was asked, with nothing to say so.
            if not isinstance(headless, bool):
                raise HTTPException(
                    400, f"'headless_launch' must be true or false, got "
                         f"{type(headless).__name__}")
            patch["headless_launch"] = headless

        return patch

    @app.put("/api/config")
    def set_config(cfg: dict):
        engine_store.set_config_many(_config_patch(cfg))
        return get_config()

    # These delegate to numera.folders so the CLI and the API cannot drift on
    # what a rename or a delete does to the profiles inside (B18).
    @app.post("/api/folders")
    def create_folder(body: dict):
        try:
            return {"folders": folders.create(engine_store.root, body.get("name"))}
        except ValueError as e:
            raise HTTPException(400, str(e))

    @app.post("/api/folders/rename")
    def rename_folder(body: dict):
        try:
            return {"folders": folders.rename(engine_store.root,
                                              body.get("from"), body.get("to"))}
        except ValueError as e:
            raise HTTPException(400, str(e))

    @app.post("/api/folders/delete")
    def delete_folder(body: dict):
        try:
            return {"folders": folders.delete(engine_store.root, body.get("name"),
                                              body.get("reassignTo"))}
        except ValueError as e:
            raise HTTPException(400, str(e))

    # ---- API tokens ------------------------------------------------------
    @app.get("/api/tokens")
    def list_tokens():
        """Never the token itself - only what it is called and when it was used.

        `envToken` says whether `NUMERA_API_TOKEN` is also in play, because a
        user who revokes every named token and still gets 401s needs to know
        the env var is the reason.
        """
        import os
        return {"tokens": [{k: v for k, v in t.items() if k != "hash"}
                           for t in tokens.list()],
                "envToken": bool((os.environ.get("NUMERA_API_TOKEN") or "").strip())}

    @app.post("/api/tokens")
    def create_token(body: dict):
        """Mint one. The value comes back here and is never retrievable again."""
        name = str(body.get("name") or "").strip()
        if not name:
            raise HTTPException(400, "give the token a name - an unnamed token is "
                                     "one nobody can safely revoke later")
        try:
            return tokens.create(name)
        except DuplicateToken as e:
            raise HTTPException(409, str(e)) from None

    @app.delete("/api/tokens/{tid}")
    def revoke_token(tid: str):
        """Remove one token, unless it is the only way into this server.

        Removing the last credential used to reopen the API to everyone, which
        is the opposite of what the person clicking "revoke" is asking for.
        The environment token counts as an alternative, because it still
        authenticates after this one is gone.
        """
        import os

        env_token = bool((os.environ.get("NUMERA_API_TOKEN") or "").strip())
        try:
            removed = tokens.revoke(tid, refuse_lockout=not env_token)
        except ValueError as locked_out:
            raise HTTPException(409, str(locked_out)) from None
        if not removed:
            raise HTTPException(404, "token not found")
        return {"ok": True, "remaining": len(tokens.list())}
