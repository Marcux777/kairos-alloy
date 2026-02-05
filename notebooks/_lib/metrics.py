from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

import numpy as np


def sharpe(returns: np.ndarray, *, risk_free: float = 0.0, eps: float = 1e-12) -> float:
    """
    Simple Sharpe ratio on a 1D return series.

    Notes:
    - This is intentionally minimal. For paper-ready statistics, prefer
      bootstrapped CIs and bias adjustments in later modules.
    """
    r = np.asarray(returns, dtype=float)
    if r.size == 0:
        return 0.0
    ex = r - float(risk_free)
    mu = float(np.mean(ex))
    sd = float(np.std(ex, ddof=1)) if ex.size > 1 else 0.0
    if not np.isfinite(sd) or sd < eps:
        return 0.0
    return mu / sd


def turnover(position: np.ndarray) -> float:
    """
    Turnover proxy from a position time series.
    position[t] is typically -1/0/1 (or continuous).
    """
    p = np.asarray(position, dtype=float)
    if p.size <= 1:
        return 0.0
    return float(np.sum(np.abs(np.diff(p))))


@dataclass(frozen=True)
class SummaryRow:
    run_id: str
    bars_processed: int
    trades: int
    net_profit: float
    sharpe: float
    max_drawdown: float

