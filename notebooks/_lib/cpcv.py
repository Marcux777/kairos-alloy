from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

import pandas as pd


@dataclass(frozen=True)
class CpcvRun:
    out_csv: Path
    json: dict[str, Any]


def run_cpcv(
    *,
    config: Path,
    out_csv: Path,
    n_groups: int = 6,
    k_test: int = 2,
    horizon_bars: int = 1,
    purge_bars: int = 0,
    embargo_bars: int = 0,
    start: Optional[str] = None,
    end: Optional[str] = None,
    cargo: str = "cargo",
) -> CpcvRun:
    cmd = [
        cargo,
        "run",
        "-q",
        "-p",
        "kairos-tui",
        "--",
        "--headless",
        "--mode",
        "cpcv",
        "--config",
        str(config),
        "--cpcv-out",
        str(out_csv),
        "--cpcv-n-groups",
        str(int(n_groups)),
        "--cpcv-k-test",
        str(int(k_test)),
        "--cpcv-horizon-bars",
        str(int(horizon_bars)),
        "--cpcv-purge-bars",
        str(int(purge_bars)),
        "--cpcv-embargo-bars",
        str(int(embargo_bars)),
    ]
    if start is not None:
        cmd += ["--cpcv-start", start]
    if end is not None:
        cmd += ["--cpcv-end", end]

    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(
            "cpcv failed\n"
            f"cmd={' '.join(cmd)}\n"
            f"stdout={proc.stdout}\n"
            f"stderr={proc.stderr}"
        )
    try:
        payload = json.loads(proc.stdout)
    except Exception as exc:  # pragma: no cover
        raise RuntimeError(f"failed to parse JSON stdout: {exc}\nstdout={proc.stdout}") from exc

    return CpcvRun(out_csv=Path(payload["out_csv"]), json=payload)


def read_cpcv_csv(path: Path) -> pd.DataFrame:
    df = pd.read_csv(path)
    expected = [
        "fold_id",
        "set",
        "segment_id",
        "start_idx",
        "end_idx",
        "start_ts",
        "end_ts",
        "start_utc",
        "end_utc",
        "test_groups",
    ]
    missing = [c for c in expected if c not in df.columns]
    if missing:
        raise ValueError(f"cpcv csv missing columns: {missing}")
    return df

