# Agent LLM (Gemini) (Python)

External inference service compatible with the Kairos Alloy agent contract (`/v1/act`, `/v1/act_batch`),
implemented with the Python standard library.

## Run (mock mode, no API key)

```bash
python3 tools/agent_llm.py --llm-mode mock
```

## Run (live Gemini)

```bash
export GEMINI_API_KEY="CHANGE_ME"
python3 tools/agent_llm.py --llm-mode live --provider gemini
```

## Run (live OpenAI)

```bash
export OPENAI_API_KEY="CHANGE_ME"
python3 tools/agent_llm.py --llm-mode live --provider openai --model gpt-4o-mini
```

Defaults:
- eval cadence: call LLM every `--eval-every-n-bars` (default: `240`), return `HOLD` between evals.
- cache: `record_replay`, stored at `runs/<run_id>/agent_llm_cache.jsonl`
- model: `gemini-1.5-flash` (Gemini) / `gpt-4o-mini` (OpenAI)

## Endpoints

- `GET /health` → `OK`
- `POST /v1/act` → returns a valid `ActionResponse` JSON (may include optional `reason`)
- `POST /v1/act_batch` → returns a valid `ActionBatchResponse` JSON

## Notes (determinism)

Use `--cache-mode record_replay` (default). The first run records decisions; subsequent runs replay the same
responses for identical requests, making backtests deterministic and avoiding repeated LLM calls.

## TUI API key entry (headers)

If you enable the agent in `--llm-mode live`, it can also read provider/model/API key from request headers:

- `X-KAIROS-LLM-PROVIDER`: `gemini` | `openai`
- `X-KAIROS-LLM-MODEL`: model id (optional)
- `X-KAIROS-LLM-API-KEY`: API key (optional; overrides env)

This allows the Rust TUI to send the user's key at runtime without storing it in `config_snapshot.toml`.
