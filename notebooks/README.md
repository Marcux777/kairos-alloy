# Notebooks (Research)

This folder hosts one notebook per planned paper (see `research/`).

The notebooks are **read-only** over run artifacts written by Kairos Alloy under `runs/<run_id>/`
(`runs/` is gitignored). Notebooks should not require modifying Rust code to run.

Data ingestion for research is handled by `notebooks/00_setup_data.ipynb` (Binance Vision → Postgres),
so the Rust engine can run backtests and experiments without missing OHLCV.

## Setup

Inside the dev container:

```bash
python3 -m venv .venv-notebooks
. .venv-notebooks/bin/activate
pip install -r notebooks/requirements.txt
```

If you plan to train DRL agents, install `gymnasium` + your RL stack (e.g. Stable-Baselines3)
in the same virtualenv. The Kairos engine remains the source of truth for execution and reward.

## Running

Start Jupyter:

```bash
python3 -m jupyterlab
```

Or run a notebook non-interactively (example):

```bash
python3 -m papermill notebooks/artigo_02_benchmark.ipynb /tmp/out.ipynb -p RUN_ID quickstart_btc_usdt_1min
```

## Conventions

- Figures should be shown inline in notebooks and must not be saved to disk.
- If you decide to export derived tables, prefer writing back into the run folder:
  - `runs/<run_id>/paper/tables/`
- Never store API keys or secrets in notebooks or artifacts.
- If you need more metadata, add it to `runs/<run_id>/manifest.json` (future) and keep notebooks backward compatible.

## CPCV (Sklearn)

Kairos Alloy can generate CPCV folds in Rust and expose them to notebooks as a sklearn
cross-validator:

```python
from pathlib import Path
from _lib import KairosCPCV

cv = KairosCPCV.generate(
    config=Path("configs/your_config.toml"),
    n_groups=6,
    k_test=2,
    horizon_bars=1,
    purge_bars=0,
    embargo_bars=0,
)

for train_idx, test_idx in cv.split(X):
    ...
```
