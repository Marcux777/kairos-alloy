# Repository Guidelines

## Project Structure & Module Organization
- Product and delivery docs live at the root: `README.md`, `PRD.md`, `ARCHITECTURE.md`, `STARTUP_PLAN.md`, and `Kairos_Alloy_PRD_MVP_v0_2.pdf`.
- Rust workspace members are split into `apps/` (executables such as `kairos-alloy`, `kairos-ingest`, `kairos-bench`) and `platform/` (domain/application/infrastructure crates).
- Operational assets are under `platform/ops/` (`configs/`, `migrations/`, `scripts/`, `observability/`).
- Python agents and research adapters live under `apps/agents/`.
- Container and dev environment files live at `Dockerfile`, `docker/entrypoint.sh`, `docker-compose.yml`, and `.devcontainer/devcontainer.json`.

## Build, Test, and Development Commands
Preferred local workflow (mirrors CI):
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --locked`
- `cargo deny --locked check advisories bans licenses sources`

Docker-based workflow (recommended for consistent onboarding):
- `docker compose up -d db`
- `docker compose run --rm dev`
- Inside `dev`, run the same `cargo fmt`/`clippy`/`test` commands above.

If your host setup differs (for example WSL paths), adjust mounts in `.devcontainer/devcontainer.json` and compose overrides as needed.

## Coding Style & Naming Conventions
- Rust is the primary implementation language for core systems. Keep new production logic in Rust unless a component is explicitly Python-scoped (for agent servers/research tooling).
- Formatting and linting are enforced in CI. Run `cargo fmt` and `cargo clippy` before opening PRs.
- Prefer descriptive, lowercase file/module names and keep module boundaries aligned with `apps/` vs `platform/` responsibilities.

## Languages & Tooling (focus on Rust)
- Keep primary implementation, documentation, and validation centered on Rust workspace tooling.
- Maintain compatibility with committed toolchain/config files (`rust-toolchain.toml`, `rustfmt.toml`, `deny.toml`).
- If you introduce or change tooling, update both `README.md` and this file in the same PR.

## Config & Secrets
- Keep secrets out of the repo. Use local `.env` or host-level secrets; do not commit credentials.
- Document new config keys with safe defaults in sample files (`.env.example`, `platform/ops/configs/*.toml`) and in `README.md`.

## Testing Guidelines
- Add deterministic unit/integration tests for new behavior and bug fixes.
- Keep fixtures small and reproducible, aligned with PRD determinism requirements.
- For database-backed paths, prefer existing PostgreSQL integration patterns and document any required setup (`docker compose up -d db`, `KAIROS_DB_URL`).

## Data & Determinism
- Use fixed random seeds in tests and backtests unless explicitly testing stochastic behavior.
- Keep fixtures small and versioned; avoid relying on mutable external datasets without pinning versions.

## Repository Layout
- Top-level layout:
  - `apps/` for runnable applications (`kairos-alloy`, `kairos-ingest`, `kairos-bench`) and Python agents
  - `platform/` for Rust core crates and operational assets (`platform/ops/*`)
  - `tests/` for cross-crate notes/fixtures when needed
- Update this guide if the layout changes again.

## Commit & Pull Request Guidelines
- Follow Conventional Commits: `type(scope): short summary` when scope helps (for example `feat(engine): add slippage guard`).
- Keep subjects imperative and around 72 chars or less.
- Prefer small, focused commits; avoid mixing refactors with behavior changes, and include scope when helpful (e.g., `feat(engine): ...`).
- PRs should include a concise description, linked issues/PRD references, and explicit validation steps (commands and results).
- Include CLI logs/screenshots for user-facing behavior changes and call out breaking changes or follow-up work.
- For significant implementation work, use a dedicated branch, keep CI green, then merge to `main`.

## Documentation & Specs
- Treat `PRD.md` as the source of truth for MVP requirements and CLI behavior. Update it if implementation details change.
- Keep `README.md` current whenever build/run steps, observability, or tooling evolve.
- Update `AGENTS.md` whenever repository layout, workflows, or quality gates change.
