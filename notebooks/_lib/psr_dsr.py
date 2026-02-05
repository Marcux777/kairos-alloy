from __future__ import annotations

"""
PSR / DSR scaffolding for research notebooks.

This module intentionally keeps a stable API while allowing us to implement
paper-ready versions iteratively.

References (planned):
- Bailey & Lopez de Prado: Probabilistic Sharpe Ratio (PSR)
- Bailey & Lopez de Prado: Deflated Sharpe Ratio (DSR)
"""

from dataclasses import dataclass
from typing import Optional

import math
import numpy as np


def _phi(x: float) -> float:
    # Standard normal CDF via erf.
    return 0.5 * (1.0 + math.erf(x / math.sqrt(2.0)))


@dataclass(frozen=True)
class PsrResult:
    psr: float
    sr: float
    sr_ref: float
    n: int


def probabilistic_sharpe_ratio(
    returns: np.ndarray,
    *,
    sr_ref: float = 0.0,
) -> PsrResult:
    """
    Compute a basic PSR estimate.

    This implementation assumes i.i.d. returns for now; for the final paper we
    will integrate dependence-aware adjustments and robust moments.
    """
    r = np.asarray(returns, dtype=float)
    n = int(r.size)
    if n < 2:
        return PsrResult(psr=0.0, sr=0.0, sr_ref=float(sr_ref), n=n)

    mu = float(np.mean(r))
    sd = float(np.std(r, ddof=1))
    if not np.isfinite(sd) or sd <= 0:
        return PsrResult(psr=0.0, sr=0.0, sr_ref=float(sr_ref), n=n)

    sr = mu / sd

    # Very lightweight moment estimates.
    z = (r - mu) / sd
    skew = float(np.mean(z**3))
    kurt = float(np.mean(z**4))

    denom = 1.0 - skew * sr + ((kurt - 1.0) / 4.0) * (sr**2)
    if denom <= 0.0:
        return PsrResult(psr=0.0, sr=float(sr), sr_ref=float(sr_ref), n=n)

    z_score = ((sr - float(sr_ref)) * math.sqrt(n - 1.0)) / math.sqrt(denom)
    return PsrResult(psr=float(_phi(z_score)), sr=float(sr), sr_ref=float(sr_ref), n=n)


def deflated_sharpe_ratio(
    returns: np.ndarray,
    *,
    trials: int,
    sr_ref: float = 0.0,
) -> float:
    """
    DSR placeholder (API stabilized).

    We will implement the full DSR threshold adjustment once we finalize the
    "number of trials" definition for each paper protocol.
    """
    raise NotImplementedError("DSR implementation pending (use PSR for now).")

