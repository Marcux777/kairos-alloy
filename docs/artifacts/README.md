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

Nota:
- `timestamp_utc` é **epoch seconds** (UTC).

## equity.csv

Header:

```
timestamp_utc,equity,cash,position_qty,unrealized_pnl,realized_pnl
```

Nota:
- `timestamp_utc` é **epoch seconds** (UTC).
- `realized_pnl` é o PnL realizado acumulado do portfólio.
- O custo-base (`position_avg_price`) é tratado como **incluindo fees de BUY** (cost basis por unidade).

## summary.json

Schema:
- `docs/artifacts/summary.schema.json`

Nota:
- `win_rate` é calculado por trade de SELL (fração de SELL fills com PnL realizado > 0).

## logs.jsonl

Audit log entries (one JSON object per line). Common fields:

- `run_id`
- `timestamp`
- `stage`
- `symbol` (optional)
- `action`
- `error` (optional)
- `details` (object)
