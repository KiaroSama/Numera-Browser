"""
Profile probes: does the proxy work, does anything leak, does the identity hold?

Three questions you want answered *before* an account is on the line:

``check_proxy``   Does traffic actually leave through the proxy, and does the
                  exit country/timezone agree with the identity? Needs internet.
``check_leaks``   Does WebRTC hand out an IP the proxy was supposed to hide?
``check_tls``     What does the TLS/HTTP2 handshake look like, and does it read
                  as a real Chromium? This is the one layer JavaScript can never
                  touch — a server sees it *before* any page code runs — so it's
                  the honest test of whether the engine, not the injected script,
                  is doing the work. Needs internet.
``trust_report``  Would this browser pass an inspection right now? Runs the
                  checks a fingerprinting script would run (webdriver, plugins,
                  UA vs platform, WebGL, fonts, canvas stability…) and scores
                  the result. Fully offline — the page it grades is served from
                  memory, so nothing about the profile touches the network.

A "check" is always ``{"name", "ok", "detail"}`` so the CLI, the API and the
dashboard can render the same list without knowing what each check means.

The probes are grouped by what they cost and what they touch:

``network``  proxy exit, WebRTC, TLS - all need internet
``dns``      the DNS leak test, whose grading half is pure
``trust``    the offline fingerprint grade
``align``    the one probe that rewrites the profile

Import from ``numera.probe`` regardless; the split is an implementation detail.
"""

from .align import align_to_reality
from .dns import check_dns
from .network import check_leaks, check_proxy, check_tls
from .trust import trust_report

# The pure helpers, re-exported because they were importable from `numera.probe`
# before this was a package and the graders are worth testing on their own -
# `_grade_dns` and `_grade` take data and return checks, no browser involved.
# `_observe` is re-exported for reading only: patch it on `probe.trust`, which
# is where `align_to_reality` looks it up.
from .align import _chrome_from_ua, _os_from_platform, _vendor_from_renderer
from .dns import _grade_dns
from .network import _is_private
from .shared import PROBE_HTML, PROBE_URL, _check, _serve_probe_page
from .trust import _grade, _observe

__all__ = [
    "align_to_reality",
    "check_dns",
    "check_leaks",
    "check_proxy",
    "check_tls",
    "trust_report",
]
