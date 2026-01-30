# Kairos Alloy

Backtesting e execucao em Rust com agente DRL + sentimento em Python.

## Arquitetura

Visao geral em `ARCHITECTURE.md`.

Documentacao suplementar: `docs/README.md`.

## Workspace Rust

Comandos locais (dentro do container):

```bash
cargo build
cargo run -p kairos-cli
```

## Instalacao via Releases

Os binarios oficiais sao publicados no GitHub Releases (Linux/Windows) com checksums SHA256.

Linux (x86_64):

```bash
sha256sum -c kairos-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf kairos-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz
./kairos-X.Y.Z-x86_64-unknown-linux-gnu/bin/kairos-alloy --version
./kairos-X.Y.Z-x86_64-unknown-linux-gnu/bin/kairos-ingest --help
```

Windows (x86_64, PowerShell):

```powershell
# Compare o hash do arquivo com o valor dentro do .sha256
Get-FileHash kairos-X.Y.Z-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Expand-Archive kairos-X.Y.Z-x86_64-pc-windows-msvc.zip -DestinationPath .
.\kairos-X.Y.Z-x86_64-pc-windows-msvc\bin\kairos-alloy.exe --version
.\kairos-X.Y.Z-x86_64-pc-windows-msvc\bin\kairos-ingest.exe --help
```

## Benchmark de performance (PRD)

Rodar benchmark sintético de 500k barras em `--release` (mede throughput do engine e pipeline de features):

```bash
cargo run -p kairos-cli --release -- bench --bars 500000 --mode features --json
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
cargo run -p kairos-cli -- --build-info
```

## Quickstart (10 minutos)

Em 2 terminais, dentro do ambiente `dev` (Docker):

```bash
docker compose up -d db
docker compose run --rm dev
```

Terminal A (migrate + ingest pequeno):

```bash
cargo run -p kairos-ingest -- migrate --db-url postgres://kairos:secret@db:5432/kairos
cargo run -p kairos-ingest -- ingest-kucoin \
  --db-url postgres://kairos:secret@db:5432/kairos \
  --symbol BTC-USDT \
  --market spot \
  --timeframe 1min \
  --start 2024-01-01T00:00:00Z \
  --end 2024-01-02T00:00:00Z
```

Terminal B (suba o agente dummy e rode o backtest via agente):

```bash
python3 tools/agent-dummy/agent_dummy.py --host 127.0.0.1 --port 8000 --mode tiny_buy &
cargo run -p kairos-cli -- backtest --config configs/quickstart.toml --out runs/
```

Artefatos em `runs/quickstart_btc_usdt_1min/`.

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

### Testes de integração (PRD §20)

Os testes E2E que cobrem Postgres (migrate + ingest-kucoin mock + backtest/paper + sentimento CSV/JSON)
ficam desabilitados por padrão. Para rodar:

```bash
export KAIROS_DB_RUN_TESTS=1
export KAIROS_DB_URL=postgres://kairos:secret@db:5432/kairos
cargo test --workspace
```
