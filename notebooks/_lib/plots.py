from __future__ import annotations

from pathlib import Path
from typing import Optional

import matplotlib.pyplot as plt
import pandas as pd


def plot_equity_curve(
    equity: pd.DataFrame,
    *,
    title: str = "Equity",
    save_path: Optional[Path] = None,
):
    if equity.empty:
        raise ValueError("equity dataframe is empty")

    x = equity["timestamp_utc"] if "timestamp_utc" in equity.columns else equity.index
    y = equity["equity"] if "equity" in equity.columns else None
    if y is None:
        raise ValueError("equity dataframe missing 'equity' column")

    fig, ax = plt.subplots(figsize=(10, 4))
    ax.plot(x, y, linewidth=1.5)
    ax.set_title(title)
    ax.set_xlabel("timestamp")
    ax.set_ylabel("equity")
    ax.grid(True, alpha=0.25)
    fig.tight_layout()

    if save_path is not None:
        save_path.parent.mkdir(parents=True, exist_ok=True)
        fig.savefig(save_path, dpi=160)
    return fig, ax

