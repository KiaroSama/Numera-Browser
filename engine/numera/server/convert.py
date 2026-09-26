"""The server's conversion: `numera.convert`, with one write-side check.

The mapping itself lives in `numera.convert` - it is core, not HTTP, and the CLI
needs it too. Every `from .convert import X` in this package still resolves.

The one difference is `to_engine_profile`. Here it is a write boundary: the API
routes and `DashboardStore`'s engine twin both convert through it, and the core
converter casts counts with `int()`, so `cores = 1.5` became `1` in one copy and
stayed `1.5` in the other. The numbers are checked against the model's policy
first. Read-only overviews call it inside their own `try` and are unaffected.
"""

from ..convert import *          # noqa: F401,F403  - the whole surface
from ..convert import (          # noqa: F401  - names `import *` skips
    CLEARABLE_KEYS, OS_TO_DASH, OS_TO_ENGINE, SWITCHABLE_KEYS, _build_user_agent,
    _parse_lines, _parse_startup_urls, _rel_time, _stable_seed,
    dashboard_coherence, to_dashboard_profile,
)
from ..convert import to_engine_profile as _core_to_engine_profile
from ..write_contract import require_flat_numbers


def to_engine_profile(profile):
    """The engine shape of a dashboard profile, refusing a lossy number."""
    require_flat_numbers(profile)
    return _core_to_engine_profile(profile)
