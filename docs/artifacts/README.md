# Run Artifacts

Each execution writes a run directory (usually `runs/<run_id>/`) with stable artifacts:

- `trades.csv`
- `equity.csv`
- `summary.json`
- `logs.jsonl`
- `config_snapshot.toml`
- `summary.html` (optional, when enabled)

## trades.csv

Header:

```
timestamp_utc,symbol,side,qty,price,fee,slippage,strategy_id,reason
```

## equity.csv

Header:

```
timestamp_utc,equity,cash,position_qty,unrealized_pnl,realized_pnl
```

## summary.json

Schema:
- `docs/artifacts/summary.schema.json`

## logs.jsonl

Audit log entries (one JSON object per line). Common fields:

- `run_id`
- `timestamp`
- `stage`
- `symbol` (optional)
- `action`
- `error` (optional)
- `details` (object)
