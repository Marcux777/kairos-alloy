# `agent-drl` (Inference Server)

This tool exposes an HTTP agent compatible with Kairos Alloy's `AgentClient`:

- `GET /health`
- `POST /v1/act`
- `POST /v1/act_batch`

It is intended for **playback** of trained DRL policies from Rust (TUI/headless backtest/paper).

## Mock mode (no dependencies)

```bash
python3 apps/agents/agent-drl/agent_drl.py --host 127.0.0.1 --port 8002 --runtime mock --mock-mode hold
```

## Stable-Baselines3 mode

Install your RL stack in a Python env (not committed to the repo), then:

```bash
python3 apps/agents/agent-drl/agent_drl.py \
  --host 127.0.0.1 --port 8002 \
  --runtime sb3 \
  --model-path /path/to/model.zip \
  --algo auto \
  --device cpu
```

## Pointing Rust to this agent

Set your config:

- `[agent].mode = "remote"`
- `[agent].url = "http://127.0.0.1:8002"`

Then run via TUI or headless backtest.

The agent returns `reason` in `ActionResponse`, which is persisted in `trades.csv` and audits.
