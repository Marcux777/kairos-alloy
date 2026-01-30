# Architecture

## Overview
Kairos Alloy is a Rust-first system organized as a workspace with five crates:

- `kairos-domain`: domain types + pure engine logic (no IO)
- `kairos-application`: use cases / orchestration (WIP)
- `kairos-infrastructure`: adapters (Postgres, filesystem, HTTP)
- `kairos-cli`: command-line interface
- `kairos-ingest`: data ingestion (KuCoin OHLCV → PostgreSQL) + DB migrations

## Current vs. Target

### Current
The workspace is already structured as **Ports & Adapters**:
- domain logic and the backtest engine live in `kairos-domain`
- IO adapters live in `kairos-infrastructure`
- `kairos-cli` orchestrates workflows (until the application layer is fully implemented)

### Target (recommended): Hexagonal (Ports & Adapters) + DDD-inspired layers
We will keep evolving the workspace toward a stricter **Ports & Adapters** structure:

- **Domain**: pure business rules and invariants (no IO).
- **Application**: use cases orchestrating domain + ports.
- **Infrastructure**: concrete adapters (Postgres, filesystem, HTTP).
- **CLI**: user-facing interface that calls the application layer.

Implementation guide (decision-complete): `docs/architecture/hexagonal_ddd.md`.

## Crate Responsibilities

### kairos-domain
Domain model and deterministic engine logic.

Includes:
- `value_objects`: `Bar`, `Trade`, `EquityPoint`, `Side`, `ActionType`, `Action`, `Timeframe`
- `entities`: `Portfolio`, `RiskLimits`, metrics types
- `services`: feature pipeline, strategy trait + strategies, engine/backtest
- `repositories`: ports/traits for IO boundaries

### kairos-cli
Entry point for users to run backtests and manage configs.

Modules:
- `commands`: CLI subcommands and routing
- `config`: config loading and validation
- `output`: summaries and report formatting

### kairos-ingest
Ingests OHLCV from KuCoin and persists it in PostgreSQL.

It also applies SQL migrations from `migrations/`.

### kairos-application
Use cases (WIP): intended home for orchestration like `RunBacktest`, `RunPaper`, `ValidateData`, `GenerateReport`.

### kairos-infrastructure
Concrete adapters for Postgres, filesystem, HTTP agents, and artifact/report writing.

## High-Level Flow
1. CLI loads config
2. CLI initializes adapters + strategy
3. Backtest runner processes market data
4. Metrics are computed and reported

## Invariants
- Domain should not depend on CLI / adapters
- Domain data structures remain stable across the MVP
- Determinism is preserved via fixed seeds and versioned fixtures

## Dependency rules (target)
When the migration is complete:
- `kairos-domain` must not depend on any IO crates (reqwest/postgres/tokio-postgres/fs, etc.).
- `kairos-application` depends only on `kairos-domain`.
- `kairos-infrastructure` depends on `kairos-domain` (implements ports).
- `kairos-cli` depends on `kairos-application` (+ config/cli-only concerns).
- `kairos-ingest` may depend on `kairos-infrastructure` for Postgres adapters (or remain a separate tool crate).
