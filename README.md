# Kairos Alloy

[![CI](https://github.com/Marcux777/kairos-alloy/actions/workflows/ci.yml/badge.svg)](https://github.com/Marcux777/kairos-alloy/actions/workflows/ci.yml)
[![CI (Postgres)](https://github.com/Marcux777/kairos-alloy/actions/workflows/ci-postgres.yml/badge.svg)](https://github.com/Marcux777/kairos-alloy/actions/workflows/ci-postgres.yml)
[![Coverage](https://github.com/Marcux777/kairos-alloy/actions/workflows/coverage.yml/badge.svg)](https://github.com/Marcux777/kairos-alloy/actions/workflows/coverage.yml)

Backtesting e execucao em Rust com agente DRL + sentimento em Python.

Links rapidos:

- Arquitetura (visao geral): `ARCHITECTURE.md`
- Documentacao suplementar: `docs/README.md`
- Semantica de execucao (simulacao): `docs/engine/execution.md`
- Especificacao do MVP: `PRD.md`

## Quickstart (10 minutos)

Dentro do ambiente `dev` (Docker), em 2 terminais:

### 1) Subir o Postgres

```bash
docker compose up -d db
```

O `db` deve ficar como `healthy` (veja via `docker compose ps`).

### 2) Entrar no container dev

```bash
docker compose run --rm dev
```

### 3) Migrar + ingerir um recorte pequeno (recomendado)

```bash
cargo run -p kairos-ingest -- migrate --db-url "$KAIROS_DB_URL"
cargo run -p kairos-ingest -- ingest-kucoin \
  --db-url "$KAIROS_DB_URL" \
  --symbol BTC-USDT \
  --market spot \
  --timeframe 1min \
  --start 2024-01-01T00:00:00Z \
  --end 2024-01-02T00:00:00Z
```

Esperado: o comando de ingestao termina sem erro e passa a existir OHLCV para o par/timeframe no DB.

### 4) Rodar um agente dummy + backtest via agente

```bash
python3 tools/agent-dummy/agent_dummy.py --host 127.0.0.1 --port 8000 --mode tiny_buy &
cargo run -p kairos-tui --
```

### 5) Ver os artefatos gerados

O run escreve em `runs/<run_id>/` (ex.: `runs/quickstart_btc_usdt_1min/`):

- `trades.csv`
- `equity.csv`
- `summary.json`
- `logs.jsonl`
- `config_snapshot.toml`
- `summary.html` (quando `report.html=true`)
- `dashboard.html` (quando `report.html=true`)

## TUI (MVP): comandos e exemplos

```bash
cargo run -p kairos-tui --
```

O que esperar:

- Ao iniciar `kairos-alloy`, voce cai direto em um menu interativo (TUI).
- Navegacao: `↑/↓` + `Enter`, `Esc` para voltar ao menu, `Ctrl-C` para sair.
- Em **Backtest**: `←/→` alterna entre Validate/Backtest/Paper; `r` roda; em Validate, `s` alterna strict.
- Gate opcional: `v` alterna "require validate" (quando on, Backtest/Paper só rodam após um Validate bem-sucedido).
- Artefatos: Backtest/Paper criam `runs/<run_id>/` e escrevem os arquivos listados acima; Reports lista os runs em `runs/`.

## Experimentos (determinismo)

Workflow recomendado:

1) Rode a TUI: `cargo run -p kairos-tui --`
2) Setup: selecione uma config em `configs/` (ou recente)
3) (Opcional) Ative o gate `v` e rode **Validate** antes de rodar Backtest/Paper
4) Rode Backtest/Paper e verifique os artefatos em `runs/<run_id>/`

Comparar dois runs:

```bash
scripts/compare_runs.py runs/<run_a> runs/<run_b>
```

O comparador valida `equity.csv`, `trades.csv` e `summary.json` (com normalizacao que ignora `run_id` e `config_snapshot`).

## Configuracao (`configs/*.toml`)

Arquivos prontos:

- `configs/quickstart.toml`: caminho mais curto para rodar (bom para onboarding).
- `configs/sample.toml`: modelo completo (espelha o PRD MVP).
- `configs/README.md`: notas sobre chaves e semantica (orders/execution/features).

Checklist rapido do que editar:

- `[run]`: `run_id`, `symbol`, `timeframe`, `initial_capital`
- `[db]`: `url` (ou omita e use `KAIROS_DB_URL`), `exchange`, `market`, `ohlcv_table`
- `[paths]`: `sentiment_path` (opcional), `out_dir`
- `[execution]`: `model`, `tif`, `latency_bars`, `max_fill_pct_of_volume`
- `[features]`: `return_mode`, `sma_windows`, `rsi_enabled`, `sentiment_lag`, `sentiment_missing`

`features.sentiment_missing` aceita:

- `error` (default)
- `zero_fill`
- `forward_fill`
- `drop_row`

## Arquitetura e docs

Visao geral em `ARCHITECTURE.md`.

Documentacao suplementar: `docs/README.md`.

## Instalacao via Releases

Os binarios oficiais sao publicados no GitHub Releases (Linux/Windows) com checksums SHA256.

Linux (x86_64):

```bash
sha256sum -c kairos-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf kairos-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz
./kairos-X.Y.Z-x86_64-unknown-linux-gnu/bin/kairos-alloy
./kairos-X.Y.Z-x86_64-unknown-linux-gnu/bin/kairos-ingest --help
```

Windows (x86_64, PowerShell):

```powershell
# Compare o hash do arquivo com o valor dentro do .sha256
Get-FileHash kairos-X.Y.Z-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Expand-Archive kairos-X.Y.Z-x86_64-pc-windows-msvc.zip -DestinationPath .
.\kairos-X.Y.Z-x86_64-pc-windows-msvc\bin\kairos-alloy.exe
.\kairos-X.Y.Z-x86_64-pc-windows-msvc\bin\kairos-ingest.exe --help
```

## Benchmark de performance (PRD)

Rodar benchmark sintético de 500k barras em `--release` (mede throughput do engine e pipeline de features):

```bash
cargo run -p kairos-bench --release -- --bars 500000 --mode features --json
```

### Profiling (CPU flamegraph)

Para gerar um flamegraph SVG do benchmark:

```bash
cargo run -p kairos-bench --release -- --bars 500000 --mode features --profile-svg runs/flamegraph.svg
```

## Observabilidade (logs + métricas)

Logs:

- Logs são exibidos na TUI (painel inferior).
- `KAIROS_LOG` controla o filtro do `tracing_subscriber::EnvFilter` (ex.: `KAIROS_LOG=debug,kairos_application=trace`).

Métricas (Prometheus):

- `KAIROS_METRICS_ADDR=127.0.0.1:9898` habilita um endpoint HTTP em `/metrics` ao rodar `kairos-alloy`.
- No `kairos-bench`, `--metrics-addr 127.0.0.1:9898` habilita o mesmo endpoint durante o benchmark.
- No `/metrics`, `.` vira `_` (ex.: `kairos.infra.postgres.query_ms` → `kairos_infra_postgres_query_ms_*`).
- Counters seguem a convenção Prometheus com sufixo `_total`.
- Métricas infra (principais; exemplos de nomes no `/metrics`):
  - Postgres OHLCV: `kairos_infra_postgres_pool_get_ms_bucket`, `kairos_infra_postgres_query_ms_bucket`, `kairos_infra_postgres_load_ohlcv_ms_bucket`, `kairos_infra_postgres_load_ohlcv_errors_total`
  - Agent HTTP: `kairos_infra_agent_call_ms_bucket`, `kairos_infra_agent_errors_total`, `kairos_infra_agent_retries_total`
  - Sentimento: `kairos_infra_sentiment_load_ms_bucket`, `kairos_infra_sentiment_load_errors_total`, `kairos_infra_sentiment_points_loaded_total`
  - Artifacts: `kairos_infra_artifacts_write_ms_bucket`, `kairos_infra_artifacts_write_calls_total`

### Grafana (dev)

Subir Prometheus + Grafana (dashboards provisionados):

```bash
docker compose -f observability/docker-compose.observability.yml up -d
```

Alertas:

- Prometheus carrega regras em `observability/prometheus/alerts.yml` (veja em `http://localhost:9090/alerts`).

Rodar o benchmark com métricas:

```bash
cargo run -p kairos-bench --release -- --metrics-addr 0.0.0.0:9898 --bars 50000000 --mode features --json
```

Abrir:

- Prometheus: `http://localhost:9090`
- Grafana (login padrão): `http://localhost:3000` (user/pass: `admin`/`admin`)

## Ingestao OHLCV (PostgreSQL)

Migracao da tabela e ingestao de candles KuCoin:

```bash
cargo run -p kairos-ingest -- migrate --db-url "$KAIROS_DB_URL"
cargo run -p kairos-ingest -- ingest-kucoin \
  --db-url "$KAIROS_DB_URL" \
  --symbol BTC-USDT \
  --market spot \
  --timeframe 1min \
  --start 2024-01-01T00:00:00Z \
  --end 2024-02-01T00:00:00Z
```

## Ambiente de construção (Docker)

Subir o PostgreSQL (Docker separado):

```bash
docker compose up -d db
```

### UID/GID (evitar arquivos como root)

Se você usa bind mount do repo (padrão do `docker compose`) e o container roda como root, é comum gerar arquivos no host com owner `root` (por exemplo `target/`), quebrando builds/edição fora do container.  
Para evitar isso, defina `KAIROS_UID`/`KAIROS_GID` (veja `.env.example`) e rebuild a imagem:

```bash
cp .env.example .env
docker compose build dev
```

### Limpeza de diretórios `*.root-owned`

Se você já rodou o repo com container como root, podem existir cópias `*.root-owned/` (ex.: `target.root-owned/`). Elas são ignoradas pelo git, mas podem ser removidas do filesystem:

```bash
sudo rm -rf \
  .configs.root-owned .crates.root-owned .docs.root-owned .github.root-owned .migrations.root-owned .scripts.root-owned .serena.root-owned .target.root-owned .tests.root-owned .tools.root-owned \
  configs.root-owned crates.root-owned docs.root-owned migrations.root-owned scripts.root-owned target.root-owned tests.root-owned tools.root-owned
```

Build da imagem:

```bash
docker build -t kairos-alloy-dev .
```

Rodar o ambiente dev (app + db via docker compose):

```bash
docker compose run --rm dev
```

Rodar manualmente com o workspace e configuração do Codex (sem compose):

```bash
docker run -it \
  -v ~/.codex:/codex-config \
  -v "$(pwd)":/workspaces/kairos-alloy \
  kairos-alloy-dev
```

### PostgreSQL (padrão)

Por padrão, o banco roda no serviço `db` do `docker-compose.yml` (host `db:5432` dentro do container) e também fica acessível no host em `localhost:5432`.

### DB URL via env (recomendado)

Para evitar colocar senha em arquivos versionados, prefira definir o DB URL via env:

```bash
cp .env.example .env
# edite KAIROS_DB_PASSWORD no .env
export KAIROS_DB_URL="postgres://$KAIROS_DB_USER:$KAIROS_DB_PASSWORD@db:5432/$KAIROS_DB_NAME"
```

### Dev Containers (VS Code / Antigravity)

Se você estiver no Windows usando WSL, o `devcontainer.json` já monta o Codex config a partir do caminho WSL:

- `\\wsl.localhost\\Ubuntu\\home\\marcux777\\.codex`

Se o seu distro/usuário for diferente, ajuste o mount em `.devcontainer/devcontainer.json`.

## Desenvolvimento sem Docker

Se você não tiver Docker disponível (por exemplo WSL sem integração do Docker Desktop), você ainda consegue:

```bash
rustup toolchain install 1.93.0
rustup default 1.93.0
rustup component add rustfmt clippy
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Para rodar `kairos-ingest`/`validate`/`backtest` com dados reais, você precisa de um PostgreSQL acessível e `db.url` ajustado no `configs/*.toml`.

## Testes

```bash
cargo test -p kairos-ingest
```

## Testes E2E (PRD20 / Postgres)

Os E2E PRD20 vivem em `crates/kairos-application/tests/prd20_integration.rs` e ficam desabilitados por padrao.
Para habilitar, exporte `KAIROS_DB_RUN_TESTS=1` e forneca `KAIROS_DB_URL`.

Dentro do compose (subindo o Postgres local):

```bash
docker compose up -d db

export KAIROS_DB_RUN_TESTS=1
export KAIROS_DB_URL="postgres://kairos:$KAIROS_DB_PASSWORD@localhost:5432/$KAIROS_DB_NAME"

cargo test -p kairos-application --test prd20_integration --locked
```

No GitHub Actions, esses testes rodam no workflow `CI (Postgres)` em `.github/workflows/ci-postgres.yml`.

### Testes de integração (PRD §20)

Os testes E2E que cobrem Postgres (migrate + ingest-kucoin mock + backtest/paper + sentimento CSV/JSON)
ficam desabilitados por padrão. Para rodar:

```bash
export KAIROS_DB_RUN_TESTS=1
export KAIROS_DB_URL="postgres://kairos:$KAIROS_DB_PASSWORD@db:5432/$KAIROS_DB_NAME"
cargo test --workspace
```

## Cobertura (CI)

O workflow `Coverage` (`.github/workflows/coverage.yml`) publica um relatorio HTML e um arquivo LCOV como artifact no GitHub Actions.

## Performance/Stress (CI)

O workflow `Perf Bench` (`.github/workflows/perf-bench.yml`) roda diariamente (scheduled) o `kairos-bench` em `--release` e publica `target/bench_engine.json` e `target/bench_features.json` como artifacts.

## Segurança (checks locais)

Para rodar os mesmos “gates” de supply-chain/segurança localmente (quando aplicável):

```bash
./scripts/security-check.sh
```

## Troubleshooting

- Postgres nao conecta: confirme `KAIROS_DB_URL` e se o host deve ser `db:5432` (dentro do compose) ou `localhost:5432` (fora).
- `agent.mode=remote` falha: garanta que o agente esteja rodando em `agent.url` e acessivel do ambiente onde o CLI roda.
- Arquivos `*.root-owned`: use `KAIROS_UID`/`KAIROS_GID` e/ou a limpeza descrita na secao UID/GID.
