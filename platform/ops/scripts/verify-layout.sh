#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

SCAN_TARGETS=(
  README.md
  PRD.md
  ARCHITECTURE.md
  AGENTS.md
  STARTUP_PLAN.md
  tests/README.md
  docs
  notebooks
  .github/workflows
)

LEGACY_LITERALS=(
  "kairos-tui"
  "tools/"
  "crates/"
  "--config configs/"
)

REQUIRED_PATHS=(
  "apps/kairos-alloy"
  "apps/kairos-ingest"
  "platform/ops/configs"
  "platform/ops/scripts"
)

REQUIRED_WORKSPACE_MEMBERS=(
  "apps/kairos-alloy"
  "apps/kairos-ingest"
  "platform/kairos-domain"
  "platform/kairos-application"
  "platform/kairos-infrastructure"
)

LEGACY_REGEXES=(
  '(^|[^[:alnum:]_/])configs/(sample|quickstart|llm_gemini)\.toml'
  '(^|[^[:alnum:]_/])configs/sweeps/'
  '(^|[^[:alnum:]_/])migrations/0001_create_ohlcv_candles\.sql'
  '(^|[^[:alnum:]_/])scripts/(security-check\.sh|compare_runs\.py|cleanup-root-owned\.sh|ingest-baseline-2017-2025\.sh)'
  '(^|[^[:alnum:]_/])observability/docker-compose\.observability\.yml'
)

find_literal_matches() {
  local literal="$1"
  rg -n --fixed-strings --no-messages \
    --glob '!**/__pycache__/**' \
    --glob '!**/.ipynb_checkpoints/**' \
    -- "$literal" "${SCAN_TARGETS[@]}"
}

find_regex_matches() {
  local pattern="$1"
  rg -n -P --no-messages \
    --glob '!**/__pycache__/**' \
    --glob '!**/.ipynb_checkpoints/**' \
    -- "$pattern" "${SCAN_TARGETS[@]}"
}

fail=0

if ! command -v rg >/dev/null 2>&1; then
  echo "error: verify-layout.sh requires ripgrep (rg) on PATH." >&2
  exit 2
fi

for required_path in "${REQUIRED_PATHS[@]}"; do
  if [[ ! -e "${required_path}" ]]; then
    echo "error: required path not found: ${required_path}" >&2
    fail=1
  fi
done

for member in "${REQUIRED_WORKSPACE_MEMBERS[@]}"; do
  if ! grep -F "\"${member}\"" Cargo.toml >/dev/null 2>&1; then
    echo "error: workspace member missing from Cargo.toml: ${member}" >&2
    fail=1
  fi
done

for literal in "${LEGACY_LITERALS[@]}"; do
  matches="$(find_literal_matches "${literal}" || true)"
  if [[ -n "${matches}" ]]; then
    echo "error: found legacy layout reference: ${literal}" >&2
    echo "${matches}" >&2
    fail=1
  fi
done

for pattern in "${LEGACY_REGEXES[@]}"; do
  matches="$(find_regex_matches "${pattern}" || true)"
  if [[ -n "${matches}" ]]; then
    echo "error: found legacy layout reference regex: ${pattern}" >&2
    echo "${matches}" >&2
    fail=1
  fi
done

if [[ "${fail}" -ne 0 ]]; then
  echo "layout verification failed." >&2
  exit 1
fi

echo "layout verification passed."
