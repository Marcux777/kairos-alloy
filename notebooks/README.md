# Notebooks (Research)

This folder hosts one notebook per planned paper (see `research/`).

The notebooks are **read-only** over run artifacts written by Kairos Alloy under `runs/<run_id>/`
(`runs/` is gitignored). Notebooks should not require modifying Rust code to run.

## Setup

Inside the dev container:

```bash
python3 -m venv .venv-notebooks
. .venv-notebooks/bin/activate
pip install -r notebooks/requirements.txt
```

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

- Prefer writing derived tables/figures back into the run folder:
  - `runs/<run_id>/paper/tables/`
  - `runs/<run_id>/paper/figures/`
- Never store API keys or secrets in notebooks or artifacts.
- If you need more metadata, add it to `runs/<run_id>/manifest.json` (future) and keep notebooks backward compatible.

