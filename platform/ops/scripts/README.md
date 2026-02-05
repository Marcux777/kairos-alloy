# Scripts

Development and maintenance scripts.

- `platform/ops/scripts/ingest-baseline-2017-2025.sh`: ingere OHLCV KuCoin em janelas anuais (2017..2025) com timeframe base `1min` (configurável por env).
- `platform/ops/scripts/verify-layout.sh`: valida referências de layout em docs/workflows e falha se detectar caminhos legados (`crates/`, `tools/`, `kairos-tui`).
