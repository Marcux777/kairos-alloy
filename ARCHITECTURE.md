# Architecture

## Overview
Kairos Alloy is a Rust-first system organized as a workspace with six crates:

- `kairos-domain`: domain types + pure engine logic (no IO)
- `kairos-application`: use cases / orchestration (backtest/paper/validate/report)
- `kairos-infrastructure`: adapters (Postgres, filesystem, HTTP)
- `kairos-alloy`: terminal UI (user-facing)
- `kairos-bench`: synthetic benchmarking tool (dev)
- `kairos-ingest`: data ingestion (KuCoin OHLCV → PostgreSQL) + DB migrations

Workspace layout (canonical):
- `apps/`: executáveis (`kairos-alloy`, `kairos-ingest`, `kairos-bench`) e agentes Python de referência
- `platform/`: domínio/aplicação/infraestrutura + assets operacionais em `platform/ops/`

## Current vs. Target

### Current
The workspace is already structured as **Ports & Adapters**:
- domain logic and the backtest engine live in `kairos-domain`
- IO adapters live in `kairos-infrastructure`
- `kairos-alloy` is the user-facing interface and calls `kairos-application`

### Target (recommended): Hexagonal (Ports & Adapters) + DDD-inspired layers
We will keep evolving the workspace toward a stricter **Ports & Adapters** structure:

- **Domain**: pure business rules and invariants (no IO).
- **Application**: use cases orchestrating domain + ports.
- **Infrastructure**: concrete adapters (Postgres, filesystem, HTTP).
- **TUI**: user-facing interface that calls the application layer.

Implementation guide (decision-complete): `docs/architecture/hexagonal_ddd.md`.

## Crate Responsibilities

### kairos-domain
Domain model and deterministic engine logic.

Includes:
- `value_objects`: `Bar`, `Trade`, `EquityPoint`, `Side`, `ActionType`, `Action`, `Timeframe`
- `entities`: `Portfolio`, `RiskLimits`, metrics types
- `services`: feature pipeline, strategy trait + strategies, engine/backtest
- `repositories`: ports/traits for IO boundaries

### kairos-alloy
Entry point for users (terminal UI). Handles navigation, logging panels, and running use cases.

### kairos-bench
Synthetic benchmark tool for development and CI (Perf Bench).

### kairos-ingest
Ingests OHLCV from KuCoin and persists it in PostgreSQL.

It also applies SQL migrations from `platform/ops/migrations/`.

### kairos-application
Use cases / orchestration:
- `backtesting`: backtest run orchestration + artifact writing
- `paper_trading`: replay/paper orchestration + artifact writing
- `validation`: data quality validation report
- `reporting`: regenerate reports from existing run artifacts (`kairos-alloy report`)

### kairos-infrastructure
Concrete adapters for Postgres, filesystem, HTTP agents, and artifact/report writing.

## High-Level Flow
1. TUI loads config
2. TUI initializes adapters and calls `kairos-application` use cases
3. Domain engine processes market data
4. Artifacts are written via ports/adapters

## Invariants
- Domain should not depend on CLI / adapters
- Domain data structures remain stable across the MVP
- Determinism is preserved via fixed seeds and versioned fixtures

## Dependency rules (target)
When the migration is complete:
- `kairos-domain` must not depend on any IO crates (reqwest/postgres/tokio-postgres/fs, etc.).
- `kairos-application` depends only on `kairos-domain`.
- `kairos-infrastructure` depends on `kairos-domain` (implements ports).
- `kairos-alloy` depends on `kairos-application` (+ UI-only concerns).
- `kairos-ingest` may depend on `kairos-infrastructure` for Postgres adapters (or remain a separate tool crate).
