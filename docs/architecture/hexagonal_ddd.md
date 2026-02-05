# Hexagonal (Ports & Adapters) + DDD-inspired Architecture (Implementation Guide)

This document defines the **target architecture** for Kairos Alloy and a **decision-complete** migration plan from the current MVP.

## Goals
- Keep domain rules deterministic, testable, and independent of IO.
- Make data sources and integrations replaceable via ports (traits).
- Preserve the TUI UX (menu-driven `kairos-alloy`) while refactoring internals.
- Improve testability: unit tests run without Postgres/HTTP; integration tests cover real adapters.

## Non-goals (for now)
- Full event sourcing / replay engine as a hard requirement (we’ll introduce domain events, but keep artifacts as the source of truth for MVP).
- Live trading (real money) and exchange order routing.
- Multi-asset portfolio and derivatives.

## Current state (MVP recap)
Historically, the MVP used a single “core” crate that mixed:
- domain logic (engine/portfolio/risk/metrics/strategy/types)
- IO concerns (Postgres OHLCV loader, filesystem sentiment loader, HTTP agent client)

**Status:** the workspace has since been split into `kairos-domain`, `kairos-infrastructure`, and `kairos-application`, and the old `kairos-core` crate has been removed. The TUI is the user-facing interface and the `kairos-alloy` binary is built from the `kairos-alloy` crate.

## Target workspace layout

```
apps/
  kairos-alloy/            # TUI interface (user-facing)
  kairos-ingest/           # ingestion tool
  kairos-bench/            # synthetic benchmark tool

platform/
  kairos-domain/           # pure domain (no IO)
  kairos-application/      # use cases / orchestration
  kairos-infrastructure/   # adapters (Postgres/filesystem/HTTP)
  ops/                     # configs, scripts, migrations, observability
```

### Dependency rules (strict)
```
kairos-alloy -> kairos-application -> kairos-domain
kairos-infrastructure -----------> kairos-domain
kairos-ingest -> kairos-infrastructure (optional) OR -> its own adapters
```

Enforcement (future): add `cargo deny` / clippy lint rules to avoid domain depending on IO crates.

## Domain model (what lives in `kairos-domain`)

### Value objects / types
Live in:
- `kairos-domain/src/value_objects/`:
  - `Bar`, `Trade`, `EquityPoint`, `Side`, `ActionType`, `Action`
  - `Timeframe` + parsing helpers

### Aggregates
Introduce an aggregate to keep invariants consistent:
- `TradingAccount` (aggregate root)
  - contains `Portfolio` + `RiskLimits`
  - provides methods that enforce invariants:
    - `place_action(...) -> Result<OrderIntent, DomainError>`
    - `apply_fill(...) -> Result<(), DomainError>`
    - `mark_to_market(...) -> EquityPoint`

### Domain services
Pure logic services:
- `ExecutionEngine` (pure execution semantics; no IO)
- `RiskManager` (risk checks; no IO)
- `MetricsCalculator` (pure metrics; no IO)

### Domain events (for auditability and replay)
Define:
- `DomainEvent` enum in `kairos-domain/src/events/domain_event.rs`
  - `OrderScheduled`
  - `OrderFilled`
  - `OrderCanceled`
  - `TradeExecuted`
  - `EquityRecorded`
  - `RiskHaltTriggered`

These events are for:
- deterministic debugging/replay in tests
- building `logs.jsonl` artifacts in the application layer

## Ports (traits) — the “hexagon boundary”

All ports live in `kairos-domain/src/repositories/` (traits only; implementations live in `kairos-infrastructure`).

### Market data (OHLCV)
`repositories/market_data.rs`
```rust
pub struct OhlcvQuery {
  pub exchange: String,
  pub market: String,
  pub symbol: String,
  pub timeframe: String,
  pub expected_step_seconds: Option<i64>,
}

pub trait MarketDataRepository {
  fn load_ohlcv(&self, query: &OhlcvQuery) -> Result<(Vec<Bar>, DataQualityReport), DomainError>;
}
```

### Sentiment
`repositories/sentiment.rs`
```rust
pub enum SentimentFormat { Csv, Json }

pub struct SentimentQuery {
  pub path: std::path::PathBuf,
  pub format: SentimentFormat,
  pub missing_policy: MissingValuePolicy,
}

pub trait SentimentRepository {
  fn load_sentiment(&self, query: &SentimentQuery) -> Result<(Vec<SentimentPoint>, SentimentReport), DomainError>;
}
```

Sentiment alignment (timestamp join + lag) is pure logic and lives in `kairos-domain/src/services/sentiment.rs` as `align_with_bars(...)` (the infrastructure module delegates to it).

### Agent inference
`repositories/agent.rs`
```rust
pub trait AgentClient {
  fn act(&self, request: &ActionRequest) -> Result<ActionResponse, DomainError>;
  fn act_batch(&self, request: &ActionBatchRequest) -> Result<ActionBatchResponse, DomainError>;
}
```

### Artifact sink (run outputs)
`artifacts.rs`
```rust
pub trait ArtifactWriter {
  fn ensure_dir(&self, path: &std::path::Path) -> Result<(), DomainError>;
  fn write_trades_csv(&self, path: &std::path::Path, trades: &[Trade]) -> Result<(), DomainError>;
  fn write_equity_csv(&self, path: &std::path::Path, points: &[EquityPoint]) -> Result<(), DomainError>;
  fn write_summary_json(&self, path: &std::path::Path, summary: &Summary, meta: Option<&serde_json::Value>, config_snapshot: Option<&serde_json::Value>) -> Result<(), DomainError>;
  fn write_summary_html(&self, path: &std::path::Path, summary: &Summary, meta: Option<&serde_json::Value>) -> Result<(), DomainError>;
  fn write_audit_jsonl(&self, path: &std::path::Path, events: &[AuditEvent]) -> Result<(), DomainError>;
  fn write_config_snapshot_toml(&self, path: &std::path::Path, contents: &str) -> Result<(), DomainError>;
}
```

### Clock (optional)
For paper mode replay pacing:
`clock.rs`
```rust
pub trait Clock {
  fn sleep_ms(&self, ms: u64);
}
```

## Application layer (use cases)

Use cases live in:
- `kairos-application/src/backtesting/`
- `kairos-application/src/paper_trading/`
- `kairos-application/src/validation/`
- `kairos-application/src/reporting/`
- `kairos-application/src/benchmarking/`

### Use cases
- `RunBacktest` (implemented)
- `RunPaper` (implemented)
- `ValidateData` (implemented)
- `GenerateReport` (implemented)
- `RunBench` (implemented)

Each use case:
- receives input config DTOs (already validated)
- calls domain services/aggregates
- uses ports for IO
- returns a `RunResult` (summary + paths + events metadata)

## Infrastructure layer (adapters)

Adapters live in `kairos-infrastructure/src/`:
- `agents/` (HTTP agent client)
- `persistence/` (Postgres access)
- `market_data/` (OHLCV CSV + Postgres loader wiring)
- `sentiment/` (CSV/JSON loaders + alignment)
- `reporting/` (artifact writers/readers)

### Postgres OHLCV adapter
- Implements `MarketDataRepository`
- Uses `postgres` + a connection pool (`r2d2_postgres`) in `kairos-infrastructure` (sync adapter).
  (Note: `kairos-ingest` uses `tokio-postgres` today; standardization is a follow-up task.)

### Filesystem sentiment adapter
- Implements `SentimentRepository`
- Uses CSV/JSON parsing logic in `kairos-infrastructure/src/sentiment/`

### HTTP agent adapter
- Implements `AgentClient`
- Uses `reqwest` (blocking or async; keep blocking for MVP, but make it injectable)

### Artifact writer adapter
- Writes `trades.csv`, `equity.csv`, `summary.json`, `logs.jsonl`
- Should use robust CSV writing (already implemented in current repo)

## Migration plan (incremental, safe)

### Phase 0 — Preparation (completed)
- Crates created: `platform/kairos-domain`, `platform/kairos-application`, `platform/kairos-infrastructure`
- `kairos-core` removed from the workspace

### Phase 1 — Extract ports + adapters (completed)
- Ports are defined in `kairos-domain/src/repositories/`
- Postgres OHLCV loader lives in `kairos-infrastructure/src/persistence/`
- Sentiment loaders live in `kairos-infrastructure/src/sentiment/`
- HTTP agent adapter lives in `kairos-infrastructure/src/agents/`

Acceptance criteria:
- `kairos-domain` does not depend on IO crates.
- Existing CLI commands still work with the same outputs.

### Phase 2 — Introduce application use cases
1. Implement `RunBacktest` and `ValidateData` in `kairos-application`.
2. Update `kairos-alloy` to call application use cases instead of orchestrating directly.

Acceptance criteria:
- CLI behavior unchanged (golden files / integration tests remain green).

**Status:** completed for backtest/paper/validate/report. The remaining migration work is optional hardening (e.g., more ports, better typed errors, and more use-case tests with mocked adapters).

### Phase 3 — Domain aggregate + domain events
1. Introduce `TradingAccount` aggregate, replace direct portfolio/risk mutations with aggregate methods.
2. Emit `DomainEvent`s for order/trade/equity/risk decisions.
3. Write events to `logs.jsonl` via artifact writer adapter.

Acceptance criteria:
- Deterministic replay possible for a single run using stored events + market data.

### Phase 4 — Remove legacy core (completed)
- `kairos-core` has been removed from the workspace; consumers depend on `kairos-domain` / `kairos-infrastructure` (and `kairos-application` as it grows).

## Testing strategy (target)
- Unit tests for `kairos-domain`: pure deterministic tests, no DB/network/files.
- Unit tests for `kairos-application`: ports mocked in-memory.
- Integration tests (PRD20): real Postgres + mock KuCoin HTTP + end-to-end CLI.
- Contract tests for agent HTTP schemas (v1 + optional batch).
