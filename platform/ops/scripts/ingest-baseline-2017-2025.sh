#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is not installed / not on PATH" >&2
  exit 2
fi

if [[ -z "${KAIROS_DB_URL:-}" ]]; then
  echo "error: KAIROS_DB_URL is not set" >&2
  echo "hint: export KAIROS_DB_URL=\"postgres://kairos:<password>@localhost:5432/kairos\"" >&2
  exit 2
fi

SYMBOL="${KAIROS_SYMBOL:-BTC-USDT}"
MARKET="${KAIROS_MARKET:-spot}"
TIMEFRAME="${KAIROS_TIMEFRAME:-1min}"
START_YEAR="${KAIROS_START_YEAR:-2017}"
END_YEAR="${KAIROS_END_YEAR:-2025}"
SKIP_MIGRATE="${KAIROS_SKIP_MIGRATE:-0}"

if [[ "${START_YEAR}" -gt "${END_YEAR}" ]]; then
  echo "error: KAIROS_START_YEAR (${START_YEAR}) must be <= KAIROS_END_YEAR (${END_YEAR})" >&2
  exit 2
fi

if [[ "${SKIP_MIGRATE}" != "1" ]]; then
  echo "== migrate =="
  cargo run -p kairos-ingest -- migrate --db-url "${KAIROS_DB_URL}"
fi

echo "== ingest baseline =="
echo "symbol=${SYMBOL} market=${MARKET} timeframe=${TIMEFRAME} years=${START_YEAR}-${END_YEAR}"

for ((year = START_YEAR; year <= END_YEAR; year++)); do
  start="${year}-01-01T00:00:00Z"
  if [[ "${year}" -eq "${END_YEAR}" ]]; then
    end="${year}-12-31T23:59:59Z"
  else
    next_year=$((year + 1))
    end="${next_year}-01-01T00:00:00Z"
  fi

  echo "-- ingest ${year}: ${start} -> ${end}"
  cargo run -p kairos-ingest -- ingest-kucoin \
    --db-url "${KAIROS_DB_URL}" \
    --symbol "${SYMBOL}" \
    --market "${MARKET}" \
    --timeframe "${TIMEFRAME}" \
    --start "${start}" \
    --end "${end}"
done

echo "done: baseline ingestion complete."
