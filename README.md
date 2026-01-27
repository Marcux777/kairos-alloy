# Kairos Alloy

Backtesting e execucao em Rust com agente DRL + sentimento em Python.

## Arquitetura

Visao geral em `ARCHITECTURE.md`.

## Workspace Rust

Comandos locais (dentro do container):

```bash
cargo build
cargo run -p kairos-cli
```

## Ingestao OHLCV (PostgreSQL)

Migracao da tabela e ingestao de candles KuCoin:

```bash
cargo run -p kairos-ingest -- migrate --db-url postgres://kairos:secret@db:5432/kairos
cargo run -p kairos-ingest -- ingest-kucoin \
  --db-url postgres://kairos:secret@db:5432/kairos \
  --symbol BTC-USDT \
  --market spot \
  --timeframe 1min \
  --start 2024-01-01T00:00:00Z \
  --end 2024-02-01T00:00:00Z
```

## CLI (MVP)

Exemplos:

```bash
cargo run -p kairos-cli -- backtest --config configs/sample.toml --out runs/
cargo run -p kairos-cli -- paper --config configs/sample.toml --out runs/
cargo run -p kairos-cli -- validate --config configs/sample.toml
cargo run -p kairos-cli -- report --input runs/<run_id>/
```

## Ambiente de construção (Docker)

Subir o PostgreSQL (Docker separado):

```bash
docker compose up -d db
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

### Dev Containers (VS Code / Antigravity)

Se você estiver no Windows usando WSL, o `devcontainer.json` já monta o Codex config a partir do caminho WSL:

- `\\wsl.localhost\\Ubuntu\\home\\marcux777\\.codex`

Se o seu distro/usuário for diferente, ajuste o mount em `.devcontainer/devcontainer.json`.

## Testes

```bash
cargo test -p kairos-ingest
```
