# Configs

Place configuration templates and runtime defaults here.

- Add sample files (for example `.env.example`) when config keys are defined.
- `sample.toml` mirrors the PRD MVP config structure.

Notes:

- MVP canonical base: store OHLCV at `1min`; derive `5min`/`15min`/`1h` runs via resampling.
- Recommended benchmark/reproducibility base window (UTC): `2017-01-01T00:00:00Z` to `2025-12-31T23:59:59Z`.
- `orders.size_mode`: `"qty"` (default) interprets action `size` as quantity; `"pct_equity"` interprets `size` as a fraction (0..=1) of equity (BUY) or position (SELL).
- `execution.*`: modela a semântica de execução. Em `model="complete"`, o engine suporta `market|limit|stop`, latência determinística em barras, TIF (GTC/IOC/FOK) e cap de liquidez via `bar.volume`.
- `features.sentiment_missing`: controls how missing/invalid sentiment values are handled: `"error"` (default), `"zero_fill"`, `"forward_fill"`, `"drop_row"`.
- `data_quality.*`: used by `validate --strict`. `max_gaps` limits the number of gap segments; `max_missing_bars` limits the number of missing bars inside gaps; `max_duplicates`/`max_out_of_order`/`max_invalid_close` limit those issues for OHLCV.
- Default `db.url` in `sample.toml` uses `db:5432` (the `docker compose` service name). If running outside compose, use `localhost:5432`.
- `db.pool_max_size` (optional, default: 8): max connections for the Postgres OHLCV connection pool.

## Sweeps (MVP+)

Sweep configs live under `platform/ops/configs/sweeps/` and define a grid search over a base `config.toml`.

Example:

- `platform/ops/configs/sweeps/sma_grid.toml`
