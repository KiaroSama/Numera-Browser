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
        used: dict = {}
        for p in dash.list():
            pr = (p or {}).get("proxy") or {}
            host, port = pr.get("host"), pr.get("port")
            if host and port:
                used[f"{host}:{port}"] = used.get(f"{host}:{port}", 0) + 1
        return used

    @app.get("/api/proxies")
    def list_proxies():
        used = _proxy_usage()
        return [{**p, "profiles": used.get(f"{p.get('host')}:{p.get('port')}", 0)}
                for p in pool.list()]

    @app.put("/api/proxies/{pid}")
    def edit_proxy(pid: str, body: dict):
        """Change one proxy in place.

        Editing rather than delete-and-reimport matters because the id is what
        profiles and the status history hang off; recreating it would lose the
        last check result and orphan nothing visibly.
        """
        if not pool.get(pid):
            raise HTTPException(404, "proxy not found")
        allowed = {"label", "type", "host", "port", "user", "pass", "country"}
        patch = {k: v for k, v in body.items() if k in allowed}
        if not patch:
            raise HTTPException(400, f"nothing to change - editable fields are {sorted(allowed)}")
        if "port" in patch:
            try:
                patch["port"] = int(patch["port"])
            except (TypeError, ValueError):
                raise HTTPException(400, f"port must be a number, got {patch['port']!r}")
        if "host" in patch and not str(patch["host"]).strip():
            raise HTTPException(400, "host cannot be empty")
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
        targets = [p for p in (pool.get(i) for i in ids) if p]
        if not targets:
            raise HTTPException(400, "no proxies to test")

        def run(proxy):
            res = pp.check(proxy)
            status = "ok" if res.get("ok") else ("skipped" if res.get("ok") is None else "failed")
            pool.update(proxy["id"], {"status": status, "lastCheck": res, "checkedAt": time.time()})
            return {"id": proxy["id"], "status": status, **res}

        with ThreadPoolExecutor(max_workers=min(8, len(targets))) as ex:
            results = list(ex.map(run, targets))
        return {"tested": len(results), "ok": sum(1 for r in results if r["status"] == "ok"),
                "results": results}

    @app.post("/api/proxies")
    def add_proxies(body: dict):
        from .. import proxypool as pp
        incoming = body.get("proxies")
        parsed = incoming if incoming else pp.parse_many(body.get("text") or "")
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
        proxy = pool.get(pid)
        if not proxy:
            raise HTTPException(404, "proxy not found")
        res = pp.check(proxy)
        status = "ok" if res.get("ok") else ("skipped" if res.get("ok") is None else "failed")
        pool.update(pid, {"status": status, "lastCheck": res, "checkedAt": time.time()})
        return {**res, "status": status}

    @app.post("/api/proxies/assign")
    def assign_proxies(body: dict):
        """Assign pool proxies across profiles, round-robin, aligning each
        profile's locale/timezone to its proxy country."""
        proxies = [pool.get(i) for i in body.get("proxyIds", [])] if body.get("proxyIds") else pool.list()
        proxies = [p for p in proxies if p]
        if not proxies:
            raise HTTPException(400, "the proxy pool is empty")
        ids = body.get("profileIds")
        targets = [dash.get(i) for i in ids] if ids else dash.list()
        targets = [t for t in targets if t]
        assigned = 0
        for i, prof in enumerate(targets):
            pr = proxies[i % len(proxies)]
            prof["proxy"] = {"type": pr.get("type", "HTTP"), "host": pr["host"], "port": str(pr["port"]),
                             "user": pr.get("user") or "", "pass": pr.get("pass") or "",
                             "country": pr.get("country") or ""}
            cc = (pr.get("country") or "").upper()
            loc = devices.COUNTRY_TO_LOCALE.get(cc)
            if loc:
                prof["locale"] = loc
                prof["timezone"] = devices.LOCALES[loc][0]
            dash.save(prof)
            assigned += 1
        return {"assigned": assigned, "pool": len(proxies)}
