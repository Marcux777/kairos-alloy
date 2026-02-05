from __future__ import annotations

import json
from pathlib import Path

import pandas as pd


def read_trades_csv(path: Path) -> pd.DataFrame:
    df = pd.read_csv(path)
    expected = [
        "timestamp_utc",
        "symbol",
        "side",
        "qty",
        "price",
        "fee",
        "slippage",
        "strategy_id",
        "reason",
    ]
    missing = [c for c in expected if c not in df.columns]
    if missing:
        raise ValueError(f"trades.csv missing columns: {missing}")
    return df


def read_equity_csv(path: Path) -> pd.DataFrame:
    df = pd.read_csv(path)
    expected = [
        "timestamp_utc",
        "equity",
        "cash",
        "position_qty",
        "unrealized_pnl",
        "realized_pnl",
    ]
    missing = [c for c in expected if c not in df.columns]
    if missing:
        raise ValueError(f"equity.csv missing columns: {missing}")
    return df


def read_logs_jsonl(path: Path) -> pd.DataFrame:
    rows: list[dict] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        raw = raw.strip()
        if not raw:
            continue
        rows.append(json.loads(raw))
    if not rows:
        return pd.DataFrame()
    return pd.json_normalize(rows)


def maybe_head(df: pd.DataFrame, n: int = 5) -> pd.DataFrame:
    return df.head(n) if not df.empty else df
