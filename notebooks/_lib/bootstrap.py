from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Optional

import numpy as np


def stationary_bootstrap_indices(
    n: int,
    *,
    mean_block_len: int,
    rng: np.random.Generator,
) -> np.ndarray:
    """
    Stationary bootstrap indices (Politis & Romano).

    Returns an index array of length n.
    """
    if n <= 0:
        return np.asarray([], dtype=int)
    if mean_block_len <= 1:
        return rng.integers(0, n, size=n, dtype=int)

    p = 1.0 / float(mean_block_len)
    idx = np.empty(n, dtype=int)
    idx[0] = int(rng.integers(0, n))
    for t in range(1, n):
        if float(rng.random()) < p:
            idx[t] = int(rng.integers(0, n))
        else:
            idx[t] = (idx[t - 1] + 1) % n
    return idx


def bootstrap_ci(
    x: np.ndarray,
    stat_fn: Callable[[np.ndarray], float],
    *,
    n_boot: int = 2000,
    alpha: float = 0.05,
    mean_block_len: int = 24,
    seed: int = 0,
) -> tuple[float, float]:
    """
    Bootstrap CI using stationary bootstrap by default (time-series friendly).
    """
    x = np.asarray(x, dtype=float)
    if x.size == 0:
        return (0.0, 0.0)

    rng = np.random.default_rng(seed)
    stats = np.empty(n_boot, dtype=float)
    for i in range(n_boot):
        idx = stationary_bootstrap_indices(x.size, mean_block_len=mean_block_len, rng=rng)
        stats[i] = float(stat_fn(x[idx]))
    lo = float(np.quantile(stats, alpha / 2.0))
    hi = float(np.quantile(stats, 1.0 - alpha / 2.0))
    return lo, hi

