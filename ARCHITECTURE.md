# Architecture

## Overview
Kairos Alloy is a Rust-first system organized as a workspace with three crates:

- `kairos-core`: engine, data types, and backtest logic
- `kairos-cli`: command-line interface that drives core workflows
- `kairos-ingest`: data ingestion (KuCoin OHLCV → PostgreSQL) + DB migrations

## Crate Responsibilities

### kairos-core
Core domain logic and engine boundaries.

Modules:
- `data`: OHLCV/sentiment loaders and alignment helpers
- `features`: feature pipeline producing `observation[]`
- `engine`: backtest runner + market data sources
- `portfolio`: positions, balances, and accounting
- `risk`: risk controls and limits
- `agents`: HTTP client + request/response types for the external agent
- `strategy`: baseline strategies + agent-backed strategy
- `metrics`: performance metrics (net profit, sharpe, max drawdown)
- `report`: artifacts (CSV/JSON/HTML) and audit logs (JSONL)
- `types`: shared base types (bar, tick, order, trade, equity point)

### kairos-cli
Entry point for users to run backtests and manage configs.

Modules:
- `commands`: CLI subcommands and routing
- `config`: config loading and validation
- `output`: summaries and report formatting

### kairos-ingest
Ingests OHLCV from KuCoin and persists it in PostgreSQL.

It also applies SQL migrations from `migrations/`.

## High-Level Flow
1. CLI loads config
2. CLI initializes core engine and strategy
3. Backtest runner processes market data
4. Metrics are computed and reported

## Invariants
- Core should not depend on CLI
- Data structures in `types` remain stable across the MVP
- Determinism is preserved via fixed seeds and versioned fixtures
