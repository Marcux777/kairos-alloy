# Architecture

## Overview
Kairos Alloy is a Rust-first system organized as a workspace with two crates:

- `kairos-core`: engine, data types, and backtest logic
- `kairos-cli`: command-line interface that drives core workflows

## Crate Responsibilities

### kairos-core
Core domain logic and engine boundaries.

Planned modules:
- `market_data`: market data types and loaders
- `portfolio`: positions, balances, and accounting
- `strategy`: strategy interfaces and execution hooks
- `backtest`: backtest runner and orchestration
- `risk`: risk controls and limits
- `metrics`: performance metrics and reports
- `types`: shared base types (bar, tick, order, fill, position)

### kairos-cli
Entry point for users to run backtests and manage configs.

Planned modules:
- `commands`: CLI subcommands and routing
- `config`: config loading and validation
- `output`: summaries and report formatting

## High-Level Flow
1. CLI loads config
2. CLI initializes core engine and strategy
3. Backtest runner processes market data
4. Metrics are computed and reported

## Invariants
- Core should not depend on CLI
- Data structures in `types` remain stable across the MVP
- Determinism is preserved via fixed seeds and versioned fixtures
