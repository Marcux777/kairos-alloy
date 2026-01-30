# Architecture

## Overview
Kairos Alloy is a Rust-first system organized as a workspace with three crates:

- `kairos-core`: engine, data types, and backtest logic
- `kairos-cli`: command-line interface that drives core workflows
- `kairos-ingest`: data ingestion (KuCoin OHLCV → PostgreSQL) + DB migrations

## Current vs. Target

### Current (MVP)
The MVP is intentionally pragmatic: `kairos-core` includes both domain logic and some infrastructure concerns (PostgreSQL access for OHLCV, filesystem access for CSV/JSON sentiment, and HTTP for the external agent).

This is acceptable for MVP validation, but it couples the domain to IO details, which makes deeper unit testing and future adapter swaps (new exchanges, new storage engines, new agent transports) harder than necessary.

### Target (recommended): Hexagonal (Ports & Adapters) + DDD-inspired layers
We will evolve the workspace to a **Ports & Adapters** structure:

- **Domain**: pure business rules and invariants (no IO).
- **Application**: use cases orchestrating domain + ports.
- **Infrastructure**: concrete adapters (Postgres, filesystem, HTTP).
- **CLI**: user-facing interface that calls the application layer.

Implementation guide (decision-complete): `docs/architecture/hexagonal_ddd.md`.

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

## Dependency rules (target)
When the migration is complete:
- `kairos-domain` must not depend on any IO crates (reqwest/postgres/tokio-postgres/fs, etc.).
- `kairos-application` depends only on `kairos-domain`.
- `kairos-infrastructure` depends on `kairos-domain` (implements ports).
- `kairos-cli` depends on `kairos-application` (+ config/cli-only concerns).
- `kairos-ingest` may depend on `kairos-infrastructure` for Postgres adapters (or remain a separate tool crate).
