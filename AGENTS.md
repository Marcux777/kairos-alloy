# Repository Guidelines

## Project Structure & Module Organization
- Root-level docs capture the product and delivery scope: `README.md`, `PRD.md`, `STARTUP_PLAN.md`, and `Kairos_Alloy_PRD_MVP_v0_2.pdf`.
- Container tooling lives at `Dockerfile` and `docker/entrypoint.sh` (copies Codex config into the container on startup).
- Devcontainer settings are in `.devcontainer/devcontainer.json` for VS Code/Antigravity workflows.
- Rust workspace lives under `crates/` (CLI, core, and ingestion tools).
- Database migrations live under `migrations/`.
- There is no application source tree yet. If you add code, introduce a clear top-level layout (for example `src/`, `tests/`, `configs/`) and update this guide.

## Build, Test, and Development Commands
Use the Docker-based environment described in `README.md`:
- Build the dev image:
  ```bash
  docker build -t kairos-alloy-dev .
  ```
- Run the container with the repo and Codex config mounted:
  ```bash
  docker run -it \
    -v ~/.codex:/codex-config \
    -v "$(pwd)":/workspaces/kairos-alloy \
    kairos-alloy-dev
  ```
- For WSL users, update the Codex mount path in `.devcontainer/devcontainer.json` if your distro/user differs.

## Coding Style & Naming Conventions
- No formatter or linter config is committed yet. When adding code, keep style consistent with the language defaults and add the relevant tooling config (e.g., `rustfmt.toml` for Rust).
- Prefer lowercase, descriptive filenames and consistent module naming. Keep paths short and aligned with the eventual CLI layout described in the PRD.

## Languages & Tooling (focus on Rust)
- This repository focuses on the Rust system. Keep primary implementation, docs, and tooling centered on Rust.
- When adding Rust code, include `rustfmt.toml` and configure `clippy` expectations; prefer `cargo fmt` and `cargo clippy` in docs.
- If you choose different Rust tooling, document the commands in `README.md` and update this guide.

## Config & Secrets
- Keep secrets out of the repo. Use local `.env` or host-level secrets; do not commit credentials.
- If the app needs config defaults, document the expected keys and example values in a sample file (for example `configs/.env.example`).

## Testing Guidelines
- There is no test framework configured yet. When introducing code, add deterministic tests (unit and/or integration) and document how to run them in `README.md`.
- Keep fixtures small and reproducible, aligned with the PRD’s determinism requirements.

## Data & Determinism
- Use fixed random seeds in tests and backtests unless explicitly testing stochastic behavior.
- Keep fixtures small and versioned; avoid relying on mutable external datasets without pinning versions.

## Repository Layout (when code is introduced)
- Consider a clear top-level layout once code exists, for example:
  - `crates/` for Rust workspace members
  - `tests/` for Rust tests or fixtures
  - `configs/` for config templates and runtime defaults
- Update this guide if a different layout is chosen.

## Commit & Pull Request Guidelines
- Git history currently shows a Conventional Commit-style message (`chore: initial commit`). Follow that pattern: `type: short summary` (e.g., `feat: add backtest CLI skeleton`), and keep the subject imperative and under ~72 chars.
- Prefer small, focused commits; avoid mixing refactors with behavior changes, and include scope when helpful (e.g., `feat(engine): ...`).
- PRs should include a concise description, linked issues/PRD references, and explicit validation steps (commands and results).
- Add screenshots/log excerpts for CLI output or UX changes, and call out breaking changes or follow-ups.
- For every significant implementation, create a dedicated branch for the work and use agents to complete required tasks. When finished and tests pass, merge into `main`.

## Documentation & Specs
- Treat `PRD.md` as the source of truth for MVP requirements and CLI behavior. Update it if implementation details change.
- Keep `README.md` current whenever build/run steps or tooling evolve.
