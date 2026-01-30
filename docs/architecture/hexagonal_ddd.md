# Hexagonal (Ports & Adapters) + DDD-inspired Architecture (Implementation Guide)

This document defines the **target architecture** for Kairos Alloy and a **decision-complete** migration plan from the current MVP.

## Goals
- Keep domain rules deterministic, testable, and independent of IO.
- Make data sources and integrations replaceable via ports (traits).
- Preserve the existing CLI UX (`kairos-alloy backtest|paper|validate|report|bench`) while refactoring internals.
- Improve testability: unit tests run without Postgres/HTTP; integration tests cover real adapters.

## Non-goals (for now)
- Full event sourcing / replay engine as a hard requirement (we’ll introduce domain events, but keep artifacts as the source of truth for MVP).
- Live trading (real money) and exchange order routing.
- Multi-asset portfolio and derivatives.

## Current state (MVP recap)
Right now, `kairos-core` contains:
- Domain logic (engine/portfolio/risk/metrics/strategy/types)
- IO concerns (Postgres OHLCV loader, filesystem sentiment loader, HTTP agent client)

This is “good enough” for MVP but is the coupling we want to remove.

## Target workspace layout

```
crates/
  kairos-domain/           # pure domain (no IO)
  kairos-application/      # use cases / orchestration
  kairos-infrastructure/   # adapters (Postgres/filesystem/HTTP)
  kairos-cli/              # CLI interface (user-facing)
  kairos-ingest/           # ingestion tool (kept as tool crate)
```

### Dependency rules (strict)
```
kairos-cli  -> kairos-application -> kairos-domain
kairos-infrastructure -----------> kairos-domain
kairos-ingest -> kairos-infrastructure (optional) OR -> its own adapters
```

Enforcement (future): add `cargo deny` / clippy lint rules to avoid domain depending on IO crates.

## Domain model (what lives in `kairos-domain`)

### Value objects / types
Move (or re-export) from `crates/kairos-core/src/types/*` into:
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

All ports live in `kairos-domain/src/ports/`.

### Market data (OHLCV)
`market_data.rs`
```rust
pub struct OhlcvQuery {
  pub exchange: String,
  pub market: String,
  pub symbol: String,
  pub timeframe: String,
  pub expected_step_seconds: Option<i64>,
}

pub trait MarketDataRepository {
  fn load_ohlcv(&self, query: &OhlcvQuery) -> Result<Vec<Bar>, DomainError>;
}
```

### Sentiment
`sentiment.rs`
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

### Agent inference
`agent.rs`
```rust
pub trait AgentClient {
  fn act(&self, request: &ActionRequest) -> Result<ActionResponse, DomainError>;
  fn act_batch(&self, requests: &[ActionRequest]) -> Result<Vec<ActionResponse>, DomainError>;
}
```

### Artifact sink (run outputs)
`artifacts.rs`
```rust
pub trait ArtifactWriter {
  fn write_trades_csv(&self, path: &std::path::Path, trades: &[Trade]) -> Result<(), DomainError>;
  fn write_equity_csv(&self, path: &std::path::Path, points: &[EquityPoint]) -> Result<(), DomainError>;
  fn write_summary_json(&self, path: &std::path::Path, summary: &Summary) -> Result<(), DomainError>;
  fn write_logs_jsonl(&self, path: &std::path::Path, events: &[DomainEvent]) -> Result<(), DomainError>;
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

All use cases live in `kairos-application/src/use_cases/`.

### Use cases
- `RunBacktest`
- `RunPaper`
- `ValidateData`
- `GenerateReport`
- `BenchFeatures`

Each use case:
- receives input config DTOs (already validated)
- calls domain services/aggregates
- uses ports for IO
- returns a `RunResult` (summary + paths + events metadata)

## Infrastructure layer (adapters)

All adapters live in `kairos-infrastructure/src/adapters/`.

### Postgres OHLCV adapter
- Implements `MarketDataRepository`
- Uses `tokio-postgres` or `postgres` (choose one and standardize)

### Filesystem sentiment adapter
- Implements `SentimentRepository`
- Uses existing CSV/JSON parsing logic (moved from `kairos-core/data/sentiment.rs`)

### HTTP agent adapter
- Implements `AgentClient`
- Uses `reqwest` (blocking or async; keep blocking for MVP, but make it injectable)

### Artifact writer adapter
- Writes `trades.csv`, `equity.csv`, `summary.json`, `logs.jsonl`
- Should use robust CSV writing (already implemented in current repo)

## Migration plan (incremental, safe)

### Phase 0 — Preparation (no behavior changes)
1. Add crates skeletons:
   - `crates/kairos-domain`
   - `crates/kairos-application`
   - `crates/kairos-infrastructure`
2. Keep `kairos-core` as a compatibility façade for now (re-export existing modules), but stop adding new IO features there.

### Phase 1 — Extract ports + adapters
1. Create ports (traits) in `kairos-domain/src/ports/` as above.
2. Move the Postgres loader from `crates/kairos-core/src/data/ohlcv.rs` into `kairos-infrastructure` and implement `MarketDataRepository`.
3. Move sentiment loaders from `crates/kairos-core/src/data/sentiment.rs` into `kairos-infrastructure` and implement `SentimentRepository`.
4. Move HTTP agent client from `crates/kairos-core/src/agents/mod.rs` into `kairos-infrastructure` and implement `AgentClient`.

Acceptance criteria:
- `kairos-domain` does not depend on IO crates.
- Existing CLI commands still work with the same outputs.

### Phase 2 — Introduce application use cases
1. Implement `RunBacktest` and `ValidateData` in `kairos-application`.
2. Update `kairos-cli` to call application use cases instead of directly calling `kairos-core`.
3. Keep `kairos-core` temporarily, but gradually route through application layer.

Acceptance criteria:
- CLI behavior unchanged (golden files / integration tests remain green).

### Phase 3 — Domain aggregate + domain events
1. Introduce `TradingAccount` aggregate, replace direct portfolio/risk mutations with aggregate methods.
2. Emit `DomainEvent`s for order/trade/equity/risk decisions.
3. Write events to `logs.jsonl` via artifact writer adapter.

Acceptance criteria:
- Deterministic replay possible for a single run using stored events + market data.

### Phase 4 — Remove `kairos-core` façade (optional)
Once all consumers depend on domain/application/infrastructure:
- delete or freeze `kairos-core` (or rename it to `kairos-domain` if desired).

## Testing strategy (target)
- Unit tests for `kairos-domain`: pure deterministic tests, no DB/network/files.
- Unit tests for `kairos-application`: ports mocked in-memory.
- Integration tests (PRD20): real Postgres + mock KuCoin HTTP + end-to-end CLI.
- Contract tests for agent HTTP schemas (v1 + optional batch).

