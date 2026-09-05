"""The DNS leak check: whose resolver saw the lookups.

Separate from `network.py` because the grading half is pure - `_grade_dns`
takes rows and returns checks, with no browser anywhere near it - and that is
what makes it testable offline.
"""

from __future__ import annotations

from typing import Optional

from ..drivers import session
from ..models import Profile
from ..store import ProfileStore
from .shared import _check

# --------------------------------------------------------------------------
# DNS leak
# --------------------------------------------------------------------------
# A DNS *leak* is when the browser's name lookups skip the proxy and go to your
# own ISP's resolver — so even with the proxy hiding your IP, the sites you
# resolve are visible to (and geo-located at) your real network. The honest way
# to see it is from outside the browser: bash.ws hands out a test id, watches
# which resolver IPs actually query a batch of unique subdomains, and reports
# each one's country/ASN. If a resolver sits in your real country instead of the
# proxy's, that's the leak. Needs internet.
_DNS_ID_URL = "https://bash.ws/id"
_DNS_SUB = "https://{i}.{id}.bash.ws/"
_DNS_RESULT = "https://bash.ws/dnsleak/test/{id}?json"

_DNS_JS = """
async ({idUrl, subTmpl, resultTmpl, n}) => {
  let id;
  try { id = (await (await fetch(idUrl, {cache:'no-store'})).text()).trim(); }
  catch (e) { return {error: 'could not reach the DNS test service'}; }
  if (!id) return {error: 'no test id issued'};
  // Fire N lookups at unique subdomains. We don't care about the responses
  // (no-cors, failures ignored) — the point is to make the browser *resolve*
  // each name so the service can see which resolver did it.
  const jobs = [];
  for (let i = 1; i <= n; i++) {
    const u = subTmpl.replace('{i}', i).replace('{id}', id);
    jobs.push(fetch(u, {mode:'no-cors', cache:'no-store'}).catch(() => {}));
  }
  await Promise.all(jobs);
  try {
    const rows = await (await fetch(resultTmpl.replace('{id}', id), {cache:'no-store'})).json();
    return {id, rows};
  } catch (e) { return {error: 'could not read the DNS result', id}; }
}
"""


def _grade_dns(rows: list, proxy_country: Optional[str], exit_ip: Optional[str] = None) -> dict:
    """Score observed resolvers against the proxy. Pure — no browser — so it can
    be tested without the network.

    bash.ws rows carry a ``type``: ``"ip"`` is the request's own exit IP,
    ``"dns"`` is a resolver that was seen doing the lookups, ``"conclusion"`` is
    a summary line we ignore. We grade the ``"dns"`` rows.
    """
    resolvers = [r for r in rows if (r.get("type") or "").lower() == "dns"]
    countries = sorted({(r.get("country_code") or r.get("country") or "?") for r in resolvers})
    asns = sorted({str(r.get("asn")) for r in resolvers if r.get("asn")})

    checks = [_check("DNS resolution was observed", bool(resolvers),
                     f"{len(resolvers)} resolver(s) seen"
                     + (f" in {', '.join(countries)}" if countries else ""))]

    want = (proxy_country or "").upper()
    if want and resolvers:
        # With a proxy, every resolver should sit in the proxy's country. Any
        # resolver elsewhere is a name lookup that escaped the tunnel.
        strays = [r for r in resolvers
                  if (r.get("country_code") or "").upper() and (r.get("country_code") or "").upper() != want]
        checks.append(_check(
            "DNS stays inside the proxy country (no ISP leak)",
            not strays,
            f"expected all in {want}, but saw {', '.join(countries)}"
            if strays else f"all resolvers in {want}"))
    elif resolvers:
        # No proxy country to compare against — there's nothing to "leak" past,
        # so this is just the baseline: which resolvers your traffic uses.
        checks.append(_check("DNS resolvers (baseline, no proxy country set)", True,
                             ", ".join(countries) or "unknown"))

    return {
        "ok": all(c["ok"] for c in checks),
        "resolvers": [{"ip": r.get("ip"), "country": r.get("country"),
                       "countryCode": r.get("country_code"), "asn": r.get("asn")}
                      for r in resolvers],
        "countries": countries,
        "asns": asns,
        "checks": checks,
    }


def check_dns(profile: Profile, store: ProfileStore, context=None,
              exit_ip: Optional[str] = None, engine: Optional[str] = None) -> dict:
    """Check whether the profile's DNS lookups leak past the proxy.

    Works without a proxy too — then it simply reports which resolvers your
    traffic uses, the baseline you compare a proxied run against.
    """
    def run(ctx):
        page = ctx.pages[0] if ctx.pages else ctx.new_page()
        try:
            return page.evaluate(_DNS_JS, {"idUrl": _DNS_ID_URL, "subTmpl": _DNS_SUB,
                                           "resultTmpl": _DNS_RESULT, "n": 8})
        except Exception as e:
            return {"error": str(e).splitlines()[0]}

    result = run(context) if context is not None else None
    if result is None:
        with session(profile, store, engine) as ctx:
            result = run(ctx)

    if not result or result.get("error") or not isinstance(result.get("rows"), list):
        return {"ok": False, "error": (result or {}).get("error") or "no DNS result",
                "checks": [_check("DNS leak test reachable", False,
                                  (result or {}).get("error") or "bash.ws unreachable")]}

    country = (profile.proxy.country if profile.proxy else None)
    return _grade_dns(result["rows"], country, exit_ip)
