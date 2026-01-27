# Configs

Place configuration templates and runtime defaults here.

- Add sample files (for example `.env.example`) when config keys are defined.
- `sample.toml` mirrors the PRD MVP config structure.

Notes:

- `orders.size_mode`: `"qty"` (default) interprets action `size` as quantity; `"pct_equity"` interprets `size` as a fraction (0..=1) of equity (BUY) or position (SELL).
- `features.sentiment_missing`: controls how missing/invalid sentiment values are handled: `"error"` (default), `"zero_fill"`, `"forward_fill"`, `"drop_row"`.
- Default `db.url` in `sample.toml` uses `db:5432` (the `docker compose` service name). If running outside compose, use `localhost:5432`.
