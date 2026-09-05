"""Kept as a re-export: the conversion itself now lives in `numera.convert`.

It moved because it is core, not HTTP - the CLI needs the same mapping, and a
core coordinator importing into the web package would be the wrong direction.
Every `from .convert import X` in this package still resolves.
"""

from ..convert import *          # noqa: F401,F403  - the whole surface
from ..convert import (          # noqa: F401  - names `import *` skips
    CLEARABLE_KEYS, OS_TO_DASH, OS_TO_ENGINE, SWITCHABLE_KEYS, _build_user_agent,
    _parse_lines, _parse_startup_urls, _rel_time, _stable_seed,
    dashboard_coherence, to_dashboard_profile, to_engine_profile,
)
