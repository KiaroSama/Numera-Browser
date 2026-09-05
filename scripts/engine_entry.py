"""PyInstaller entry point: the numera CLI, nothing else.

Frozen by both build paths - `scripts/build-desktop.ps1` on Windows and the
POSIX job in `.github/workflows/build-desktop.yml`.

It exists because `engine/numera/__main__.py` cannot be the entry. PyInstaller
runs the entry script as top-level `__main__` with no package context, so that
file's `from .cli import main` fails with "attempted relative import with no
known parent package" and the frozen engine dies on its very first command.
The import here is absolute for exactly that reason.

The two builds used to define this separately - Windows generated it, POSIX
froze `__main__.py` - which is how one of them was broken for as long as it
existed without anyone noticing. One file, one definition, no drift.
"""
import sys

from numera.cli import main

sys.exit(main())
