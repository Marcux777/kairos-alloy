#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is not installed / not on PATH" >&2
  exit 2
fi

if ! cargo deny --version >/dev/null 2>&1; then
  echo "error: cargo-deny is not installed (needed for supply-chain checks)" >&2
  echo "hint: install with: cargo install cargo-deny" >&2
  exit 2
fi

echo "== cargo-deny =="
cargo deny --locked check advisories bans licenses sources

if cargo audit --version >/dev/null 2>&1; then
  echo "== cargo-audit =="
  cargo audit
else
  echo "== cargo-audit (skipped) =="
  echo "hint: install with: cargo install cargo-audit" >&2
fi
