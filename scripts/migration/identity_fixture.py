"""Emit original process birth token for a native parity comparison."""
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'engine'))
from numera.proctree import started_at
if len(sys.argv)!=2:
    raise SystemExit('One PID is required')
value=started_at(int(sys.argv[1]))
if value is None:
    raise SystemExit('Process birth is unknown')
print(value)
