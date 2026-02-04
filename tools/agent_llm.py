#!/usr/bin/env python3
"""
Kairos Alloy LLM agent entrypoint (compat wrapper).

The Rust quickstart config (`configs/quickstart.toml`) expects an agent at:
  http://127.0.0.1:8000

The full implementation lives at `tools/agent-llm/agent_llm.py`. This wrapper:
- Defaults to `--host 127.0.0.1 --port 8000` if not provided
- Forwards all args to the underlying implementation
"""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


def main() -> int:
    script = Path(__file__).resolve().parent / "agent-llm" / "agent_llm.py"
    argv = list(sys.argv[1:])

    if "--host" not in argv:
        argv = ["--host", "127.0.0.1", *argv]
    if "--port" not in argv:
        argv = ["--port", "8000", *argv]

    sys.argv = [str(script), *argv]
    runpy.run_path(str(script), run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

