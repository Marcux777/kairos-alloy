# Agent Contract (v1)

Kairos Alloy integrates with an external inference service (usually Python) via HTTP/JSON.

## Endpoint

- `POST /v1/act`

## Versioning

Both request and response are versioned via fields:

- `api_version` (e.g. `"v1"`)
- `feature_version` (e.g. `"v1"`)

The engine will log agent diagnostics to `logs.jsonl` during execution (latency, status, retries, fallback).

## Schemas and examples

Schemas:
- `docs/agent/v1/request.schema.json`
- `docs/agent/v1/response.schema.json`

Examples:
- `docs/agent/v1/request.example.json`
- `docs/agent/v1/response.example.json`

