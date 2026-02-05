#!/usr/bin/env python3
"""
Kairos Alloy DRL agent entrypoint (compat wrapper).

The full implementation lives at `tools/agent-drl/agent_drl.py`.
"""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


def main() -> int:
    script = Path(__file__).resolve().parent / "agent-drl" / "agent_drl.py"
    sys.argv = [str(script), *sys.argv[1:]]
    runpy.run_path(str(script), run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

