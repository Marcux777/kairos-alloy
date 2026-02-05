# Tests

Place cross-crate integration tests and fixtures here.

Current workspace tests live inside workspace packages (`apps/` and `platform/`) as unit/integration tests. Cross-package integration tests can be added under `tests/` when needed.

PRD20 end-to-end tests live in `platform/kairos-application/tests/prd20_integration.rs` and are executed in CI by `.github/workflows/ci-postgres.yml`.
