# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.0] - 2026-01-28

### Added

- `kairos-alloy` CLI: backtest, paper, validate, report, bench
- PostgreSQL schema + KuCoin ingestion (`kairos-ingest`)
- Agent contract schemas/examples (`docs/agent/v1`)
- Deterministic PRD §20 integration tests (Postgres + mocked KuCoin + sentiment CSV/JSON)
- Resampling configurable via `db.source_timeframe`
- Audit logs (`logs.jsonl`) with `symbol` and `error` fields

