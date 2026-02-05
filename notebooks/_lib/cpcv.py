from __future__ import annotations

import json
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterator, Optional

try:  # Optional at import-time; required by notebooks/requirements.txt
    import numpy as np
except Exception:  # pragma: no cover
    np = None  # type: ignore

import pandas as pd

try:  # Optional at import-time; required by notebooks/requirements.txt
    from sklearn.model_selection import BaseCrossValidator
except Exception:  # pragma: no cover
    BaseCrossValidator = object  # type: ignore


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


class KairosCPCV(BaseCrossValidator):
    """
    CPCV splitter backed by Kairos Alloy's Rust headless generator.

    The Rust CLI exports fold segments (ranges). This wrapper expands segments into
    index arrays on-demand to match sklearn's BaseCrossValidator contract.
    """

    def __init__(
        self,
        *,
        folds_df: pd.DataFrame,
        n_samples: int,
        meta_json: dict[str, Any],
        out_csv: Path,
        strict_len_check: bool = True,
        _tmpdir: Optional[tempfile.TemporaryDirectory[str]] = None,
    ) -> None:
        self._df = folds_df
        self._n_samples = int(n_samples)
        self._meta = meta_json
        self._out_csv = Path(out_csv)
        self._strict_len_check = bool(strict_len_check)
        self._tmpdir = _tmpdir

    @property
    def n_samples(self) -> int:
        return self._n_samples

    @property
    def out_csv(self) -> Path:
        return self._out_csv

    @property
    def meta(self) -> dict[str, Any]:
        return self._meta

    @property
    def folds_df(self) -> pd.DataFrame:
        return self._df

    @staticmethod
    def generate(
        *,
        config: Path,
        n_groups: int = 6,
        k_test: int = 2,
        horizon_bars: int = 1,
        purge_bars: int = 0,
        embargo_bars: int = 0,
        start: Optional[str] = None,
        end: Optional[str] = None,
        out_csv: Optional[Path] = None,
        cargo: str = "cargo",
        strict_len_check: bool = True,
    ) -> "KairosCPCV":
        if np is None:  # pragma: no cover
            raise RuntimeError(
                "numpy is required for KairosCPCV. Install notebooks/requirements.txt."
            )
        if BaseCrossValidator is object:  # pragma: no cover
            raise RuntimeError(
                "scikit-learn is required for KairosCPCV. Install notebooks/requirements.txt."
            )

        tmpdir: Optional[tempfile.TemporaryDirectory[str]] = None
        if out_csv is None:
            tmpdir = tempfile.TemporaryDirectory(prefix="kairos_cpcv_")
            out_csv = Path(tmpdir.name) / "folds.csv"

        run = run_cpcv(
            config=config,
            out_csv=out_csv,
            n_groups=n_groups,
            k_test=k_test,
            horizon_bars=horizon_bars,
            purge_bars=purge_bars,
            embargo_bars=embargo_bars,
            start=start,
            end=end,
            cargo=cargo,
        )
        df = read_cpcv_csv(run.out_csv)
        n_samples = int(run.json.get("rows", 0))
        if n_samples <= 0:
            raise RuntimeError(f"unexpected n_samples from rust: {n_samples}")

        return KairosCPCV(
            folds_df=df,
            n_samples=n_samples,
            meta_json=run.json,
            out_csv=run.out_csv,
            strict_len_check=strict_len_check,
            _tmpdir=tmpdir,
        )

    def get_n_splits(self, X=None, y=None, groups=None) -> int:  # noqa: N803
        return int(self._df["fold_id"].nunique())

    def split(self, X, y=None, groups=None) -> Iterator[tuple["np.ndarray", "np.ndarray"]]:  # noqa: N803
        if np is None:  # pragma: no cover
            raise RuntimeError(
                "numpy is required for KairosCPCV. Install notebooks/requirements.txt."
            )
        if self._strict_len_check:
            n = len(X)
            if n != self._n_samples:
                raise ValueError(
                    f"len(X) must match CPCV rows={self._n_samples} (got {n}). "
                    "Ensure X was built from the same time window used to generate CPCV."
                )

        fold_ids = sorted(int(v) for v in self._df["fold_id"].unique())
        for fold_id in fold_ids:
            train_segments = _segments_for_fold(self._df, fold_id, "train")
            test_segments = _segments_for_fold(self._df, fold_id, "test")
            train_idx = _expand_segments(train_segments)
            test_idx = _expand_segments(test_segments)
            yield train_idx, test_idx


def _segments_for_fold(
    df: pd.DataFrame,
    fold_id: int,
    set_name: str,
) -> list[tuple[int, int]]:
    subset = df[(df["fold_id"] == fold_id) & (df["set"] == set_name)].copy()
    if subset.empty:
        return []
    subset = subset.sort_values("segment_id", kind="stable")
    out: list[tuple[int, int]] = []
    for row in subset.itertuples(index=False):
        start = int(getattr(row, "start_idx"))
        end = int(getattr(row, "end_idx"))
        if end < start:
            raise ValueError(f"invalid segment: start_idx={start} end_idx={end}")
        out.append((start, end))
    return out


def _expand_segments(segments: list[tuple[int, int]]) -> "np.ndarray":
    if np is None:  # pragma: no cover
        raise RuntimeError(
            "numpy is required for KairosCPCV. Install notebooks/requirements.txt."
        )
    if not segments:
        return np.array([], dtype=np.int64)
    parts = [np.arange(s, e + 1, dtype=np.int64) for (s, e) in segments]
    return np.concatenate(parts, axis=0)
