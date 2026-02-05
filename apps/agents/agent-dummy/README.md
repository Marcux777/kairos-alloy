# Agent Dummy (Python)

Minimal external inference service compatible with the Kairos Alloy agent contract.

## Run

```bash
python3 tools/agent-dummy/agent_dummy.py --host 127.0.0.1 --port 8000
```

## Endpoints

- `GET /health` → `OK`
- `POST /v1/act` → returns a valid `ActionResponse` JSON

Defaults:
- `action_type`: `HOLD`
- `size`: `0.0`

