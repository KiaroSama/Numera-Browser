"""Server conversion validates numbers before the shared mapping can cast them.

The core mapping remains in `numera.convert`, including its tolerant legacy
read behavior. The HTTP adapter is also used by DashboardStore's engine twin
and every create/update/batch/import conversion. It refuses a lossy numeric
input instead of persisting a dashboard count and a different engine count.
Read-only overviews already handle an unconvertible record explicitly.
"""

from ..convert import *          # noqa: F401,F403 - compatibility surface
from ..convert import (          # noqa: F401 - names import * skips
    CLEARABLE_KEYS, OS_TO_DASH, OS_TO_ENGINE, SWITCHABLE_KEYS, _build_user_agent,
    _parse_lines, _parse_startup_urls, _rel_time, _stable_seed,
    dashboard_coherence, to_dashboard_profile,
    to_engine_profile as _core_to_engine_profile,
)
from ..write_contract import require_dashboard_numbers


def to_engine_profile(profile):
    """Validate before conversion; never silently truncate a fractional count."""
    require_dashboard_numbers(profile)
    return _core_to_engine_profile(profile)
