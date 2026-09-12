"""The shared proxy pool: list, import, edit, health-check, export, delete
and assign."""

from __future__ import annotations

import time
from .. import devices
from ..models import Proxy


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    dash, pool = ctx.dash, ctx.pool

    # ---- Proxy pool ------------------------------------------------------
    def _proxy_usage() -> dict:
        """How many profiles each pooled proxy is assigned to, keyed by host:port.

        The table shows it so a bulk delete is an informed decision rather than a
        surprise: removing a proxy three profiles are using is a different act
        from removing an unused one.
        """
        from .. import proxypool as pp

        used: dict = {}
        for p in dash.list():
            pr = (p or {}).get("proxy") or {}
            if pr.get("host") and pr.get("port"):
                key = pp.identity(pr)
                used[key] = used.get(key, 0) + 1
        return used

    @app.get("/api/proxies")
    def list_proxies():
        from .. import proxypool as pp

        used = _proxy_usage()
        return [{**p, "profiles": used.get(pp.identity(p), 0)} for p in pool.list()]

    @app.put("/api/proxies/{pid}")
    def edit_proxy(pid: str, body: dict):
        """Change one proxy in place.

        Editing rather than delete-and-reimport matters because the id is what
        profiles and the status history hang off; recreating it would lose the
        last check result and orphan nothing visibly.
        """
        if not pool.get(pid):
            raise HTTPException(404, "proxy not found")
        allowed = {"label", "type", "host", "port", "user", "pass", "country",
                   "rotateUrl", "archived"}
        patch = {k: v for k, v in body.items() if k in allowed}
        # Edit went straight to the store while create was validated, so an
        # unsupported type, a port of 70000 or a malformed country could be
        # persisted through this route and only fail later at launch. Validate
        # the merged record, the same way `POST /api/proxies` does.
        if patch:
            from .. import proxypool as pp
            merged = {**pool.get(pid), **patch}
            try:
                clean = pp.normalize(merged)
            except ValueError as e:
                raise HTTPException(400, str(e)) from None
            # `pass` is deliberately not written back. Normalisation is about
            # the canonical *data* shape and it maps `""` to None, which for a
            # secret is the wrong *intent*: `""` is an explicit clear and None
            # means "unchanged". Copying the normalised value over the caller's
            # made `PUT {"pass": ""}` answer 200 and leave the old password in
            # place. Nothing about `pass` is validated here anyway.
            for field in ("type", "host", "port", "user", "country"):
                if field in patch:
                    patch[field] = clean[field]
        if not patch:
            raise HTTPException(400, f"nothing to change - editable fields are {sorted(allowed)}")
        # A changed address invalidates the last health check; saying "OK" about
        # an address that was never tested is the same class of lie as a control
        # that reports instead of acting.
        if {"host", "port", "user", "pass", "type"} & set(patch):
            patch.update({"status": "untested", "lastCheck": None, "checkedAt": None})
        return pool.update(pid, patch)

    @app.post("/api/proxies/export")
    def export_proxies(body: dict):
        """Render proxies back to importable lines.

        Server-side on purpose. The line format is the parser's exact inverse and
        there is a round-trip test holding the two together; a second
        implementation in the browser would drift from it silently, and the first
        sign would be an export that no longer imports.
        """
        from .. import proxypool as pp
        ids = body.get("ids")
        items = pool.list()
        if ids:
            keep = set(ids)
            items = [p for p in items if p.get("id") in keep]
        # An export whose credentials cannot be read would render blank
        # passwords into a file that looks complete and re-imports as a set of
        # broken proxies. Refuse instead of writing a plausible wrong artifact.
        from ..vault import LOCKED_MESSAGE
        withheld = [p["id"] for p in items if pool.secret_withheld(p["id"])]
        if withheld:
            raise HTTPException(409, f"{len(withheld)} of these proxies have a "
                                     f"password that cannot be read: " + LOCKED_MESSAGE)
        lines = [pp.to_line(p) for p in items]
        return {"count": len(lines), "text": "\n".join(lines) + ("\n" if lines else "")}

    @app.post("/api/proxies/delete")
    def delete_proxies(body: dict):
        """Delete several at once. Ids that are already gone are not an error."""
        ids = body.get("ids") or []
        if not isinstance(ids, list) or not ids:
            raise HTTPException(400, "give a list of proxy ids to delete")
        removed = [i for i in ids if pool.delete(i)]
        return {"deleted": len(removed), "ids": removed, "total": len(pool.list())}

    @app.post("/api/proxies/test")
    def test_proxies(body: dict):
        """Health-check several proxies in one call.

        Concurrently: a serial sweep of a 40-proxy pool at ~1s each is a minute
        of staring at a spinner, and these are independent network calls.
        """
        from concurrent.futures import ThreadPoolExecutor

        from .. import proxypool as pp
        ids = body.get("ids") or [p["id"] for p in pool.list()]
        # One snapshot, not one full read per id: `pool.get()` re-reads and
        # re-decrypts the whole file every call (plan 011).
        by_id = {p["id"]: p for p in pool.list()}
        targets = [by_id[i] for i in ids if i in by_id]
        # A proxy whose password cannot be read is reported as skipped, not
        # tested against a blank credential and recorded as broken.
        withheld = [p for p in targets if pool.secret_withheld(p["id"])]
        targets = [p for p in targets if not pool.secret_withheld(p["id"])]
        if not targets and not withheld:
            raise HTTPException(400, "no proxies to test")

        def run(proxy):
            res = pp.check(proxy)
            status = "ok" if res.get("ok") else ("skipped" if res.get("ok") is None else "failed")
            return {"id": proxy["id"], "status": status, **res}

        # Only when there is something to run. `min(8, 0)` is 0 and
        # `ThreadPoolExecutor(max_workers=0)` raises, so a selection whose
        # credentials are all withheld answered 500 - and it raised *before* the
        # skipped results below, which are the whole answer to that request.
        results = []
        if targets:
            with ThreadPoolExecutor(max_workers=min(8, len(targets))) as ex:
                results = list(ex.map(run, targets))
        # One write for the whole sweep. Updating per proxy meant N locked
        # read-modify-write cycles over the entire pool file for one button.
        pool.update_many({r["id"]: {"status": r["status"],
                                    "lastCheck": {k: v for k, v in r.items()
                                                  if k not in ("id", "status")},
                                    "checkedAt": time.time()} for r in results})
        for p in withheld:
            results.append({"id": p["id"], "status": "skipped",
                            "error": "password locked; unlock to test this proxy"})
        return {"tested": len(results), "ok": sum(1 for r in results if r["status"] == "ok"),
                "skipped": len(withheld), "results": results}

    @app.post("/api/proxies")
    def add_proxies(body: dict):
        from .. import proxypool as pp
        incoming = body.get("proxies")
        # Both shapes through the same validator. Structured input used to go
        # straight to the pool and only text was normalised, so a malformed
        # object became a 500 or a stored record nothing could launch.
        try:
            parsed = (pp.normalize_many(incoming) if incoming is not None
                      else pp.normalize_many(pp.parse_many(body.get("text") or "")))
        except ValueError as e:
            raise HTTPException(400, str(e)) from None
        if not parsed:
            raise HTTPException(400, "no proxies could be parsed from that input")
        added = pool.add_many(parsed)
        return {"added": len(added), "total": len(pool.list()), "proxies": added}

    @app.delete("/api/proxies/{pid}")
    def delete_proxy(pid: str):
        if not pool.delete(pid):
            raise HTTPException(404, "proxy not found")
        return {"ok": True}

    @app.post("/api/proxies/{pid}/test")
    def test_proxy(pid: str):
        from .. import proxypool as pp
        from ..vault import LOCKED_MESSAGE
        proxy = pool.get(pid)
        if not proxy:
            raise HTTPException(404, "proxy not found")
        # A locked read gives `pass=None`, so the check would authenticate with
        # nothing and record a real "failed" against a proxy that is fine.
        if pool.secret_withheld(pid):
            raise HTTPException(409, "this proxy has a password that cannot be read: "
                                     + LOCKED_MESSAGE)
        res = pp.check(proxy)
        status = "ok" if res.get("ok") else ("skipped" if res.get("ok") is None else "failed")
        pool.update(pid, {"status": status, "lastCheck": res, "checkedAt": time.time()})
        return {**res, "status": status}

    @app.post("/api/proxies/{pid}/rotate")
    def rotate_proxy(pid: str):
        """Ask a rotating proxy for a new exit IP and record what came back."""
        from .. import proxypool as pp
        from ..vault import LOCKED_MESSAGE
        proxy = pool.get(pid)
        if not proxy:
            raise HTTPException(404, "proxy not found")
        # Rotating verifies the new exit IP through the proxy, which needs the
        # credential; without it the check after the rotate reports a failure
        # that says nothing about the provider.
        if pool.secret_withheld(pid):
            raise HTTPException(409, "this proxy has a password that cannot be read: "
                                     + LOCKED_MESSAGE)
        res = pp.rotate(proxy)
        patch = {}
        if res.get("attempted"):
            patch["rotateTriedAt"] = time.time()
        if res.get("rotated"):
            patch["rotatedAt"] = time.time()
        if res.get("ok") is not None:
            # A rotate carries a fresh health check with it; storing it means the
            # row stops showing the address it used to have.
            patch.update({"status": "ok" if res.get("ok") else "failed",
                          "lastCheck": res, "checkedAt": time.time()})
        if patch:
            pool.update(pid, patch)
        return res

    @app.post("/api/proxies/rotate")
    def rotate_proxies(body: dict):
        """Rotate several. Serial on purpose: these hit one provider's control
        plane, and firing eight at once is how a pool gets rate-limited."""
        from .. import proxypool as pp
        from ..vault import LOCKED_MESSAGE
        ids = body.get("ids") or []
        # One snapshot rather than a full re-read per id.
        by_id = {p["id"]: p for p in pool.list()}
        targets = [by_id[i] for i in ids if i in by_id]
        if not targets:
            raise HTTPException(400, "no proxies to rotate")
        # Rotating verifies the new exit IP *through* the proxy, so a hidden
        # credential would record a failure that says nothing about the provider.
        locked = [p["id"] for p in targets if pool.secret_withheld(p["id"])]
        if locked:
            raise HTTPException(409, f"{len(locked)} of these proxies have a "
                                     f"password that cannot be read: " + LOCKED_MESSAGE)
        results = []
        for proxy in targets:
            res = pp.rotate(proxy)
            patch = {}
            if res.get("attempted"):
                patch["rotateTriedAt"] = time.time()
            if res.get("rotated"):
                patch["rotatedAt"] = time.time()
            if res.get("ok") is not None:
                patch.update({"status": "ok" if res.get("ok") else "failed",
                              "lastCheck": res, "checkedAt": time.time()})
            if patch:
                pool.update(proxy["id"], patch)
            results.append({"id": proxy["id"], **res})
        return {"rotated": sum(1 for r in results if r.get("rotated")),
                "tried": len(results), "results": results}

    @app.post("/api/proxies/archive")
    def archive_proxies(body: dict):
        """Retire proxies without deleting them.

        Delete loses which profiles used a proxy and why it was dropped; archive
        keeps the row and its last check, and only takes it out of assignment.
        """
        ids = body.get("ids") or []
        if not isinstance(ids, list) or not ids:
            raise HTTPException(400, "give a list of proxy ids to archive")
        archived = bool(body.get("archived", True))
        changed = [pool.update(i, {"archived": archived}) for i in ids if pool.get(i)]
        return {"archived" if archived else "restored": len(changed),
                "ids": [c["id"] for c in changed if c]}

    @app.post("/api/proxies/assign")
    def assign_proxies(body: dict):
        """Assign pool proxies across profiles, round-robin, aligning each
        profile's locale/timezone to its proxy country."""
        proxies = [pool.get(i) for i in body.get("proxyIds", [])] if body.get("proxyIds") else pool.list()
        # An archived proxy is one the user retired. Handing it out again would
        # make archiving a label rather than a decision - and an explicit id list
        # is an explicit choice, so only the "whole pool" path filters.
        proxies = [p for p in proxies if p and (body.get("proxyIds") or not p.get("archived"))]
        # Assigning a proxy whose password is unreadable would write `pass=None`
        # onto the profile - and on a profile that already has one, the save
        # path preserves the *old* ciphertext while the host and user change,
        # producing a credential pair that belongs to no proxy at all.
        from ..vault import LOCKED_MESSAGE
        locked = [p["id"] for p in proxies if pool.secret_withheld(p["id"])]
        if locked:
            raise HTTPException(409, f"{len(locked)} of the selected proxies have a "
                                     f"password that cannot be read: " + LOCKED_MESSAGE)
        if not proxies:
            raise HTTPException(400, "the proxy pool is empty"
                                     if not pool.list() else
                                     "every proxy in the pool is archived")
        ids = body.get("profileIds")
        # The plan is built from ids, not from objects read now: `patch_many`
        # re-reads each record inside the transaction, so an edit made while
        # this request was deciding is not overwritten by a stale snapshot.
        target_ids = list(ids) if ids else [p["id"] for p in dash.list()]
        order = {pid: i for i, pid in enumerate(target_ids)}

        def merge(pid: str, current: dict) -> dict:
            pr = proxies[order[pid] % len(proxies)]
            current["proxy"] = {
                "type": pr.get("type", "HTTP"), "host": pr["host"],
                "port": str(pr["port"]), "user": pr.get("user") or "",
                "pass": pr.get("pass") or "", "country": pr.get("country") or ""}
            cc = (pr.get("country") or "").upper()
            loc = devices.COUNTRY_TO_LOCALE.get(cc)
            if loc:
                current["locale"] = loc
                current["timezone"] = devices.LOCALES[loc][0]
            return current

        # All of them or none. The loop used to save as it went, so a failure
        # on target N left 0..N-1 assigned while the request raised - an
        # unknown prefix committed under a generic error.
        saved = dash.patch_many(target_ids, merge)
        return {"assigned": len(saved), "pool": len(proxies)}
