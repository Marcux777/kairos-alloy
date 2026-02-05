from __future__ import annotations

"""
Reality Check / SPA scaffolding (API stabilized).

This module will host implementations of:
- White's Reality Check
- Hansen's Superior Predictive Ability (SPA)

For now, we keep stable function signatures so notebooks can depend on them.
"""

from dataclasses import dataclass
from typing import Optional

import numpy as np


@dataclass(frozen=True)
class RcSpaResult:
    p_value: float
    statistic: float
    n: int
    method: str


def reality_check(
    candidate_returns: np.ndarray,
    benchmark_returns: np.ndarray,
    *,
    mean_block_len: int = 24,
    n_boot: int = 2000,
    seed: int = 0,
) -> RcSpaResult:
    """
    Placeholder; returns NaNs until implemented.
    """
    raise NotImplementedError("Reality Check implementation pending.")


def spa_test(
    candidate_returns: np.ndarray,
    benchmark_returns: np.ndarray,
    *,
    mean_block_len: int = 24,
    n_boot: int = 2000,
    seed: int = 0,
) -> RcSpaResult:
    """
    Placeholder; returns NaNs until implemented.
    """
    raise NotImplementedError("SPA implementation pending.")

