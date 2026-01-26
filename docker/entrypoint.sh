#!/usr/bin/env bash
set -euo pipefail

CODEX_SRC="/codex-config"
CODEX_DST="/root/.codex"

if [ -d "$CODEX_SRC" ]; then
  mkdir -p "$CODEX_DST"
  cp -a "$CODEX_SRC"/. "$CODEX_DST"/
fi

exec "$@"
