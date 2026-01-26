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

## CLI (MVP)

Exemplos:

```bash
cargo run -p kairos-cli -- backtest --config configs/sample.toml --out runs/
cargo run -p kairos-cli -- paper --config configs/sample.toml --out runs/
cargo run -p kairos-cli -- validate --config configs/sample.toml
cargo run -p kairos-cli -- report --input runs/<run_id>/
```

## Ambiente de construção (Docker)

Build da imagem:

```bash
docker build -t kairos-alloy-dev .
```

Rodar com o workspace e configuração do Codex:

```bash
docker run -it \
  -v ~/.codex:/codex-config \
  -v "$(pwd)":/workspaces/kairos-alloy \
  kairos-alloy-dev
```

### Dev Containers (VS Code / Antigravity)

Se você estiver no Windows usando WSL, o `devcontainer.json` já monta o Codex config a partir do caminho WSL:

- `\\wsl.localhost\\Ubuntu\\home\\marcux777\\.codex`

Se o seu distro/usuário for diferente, ajuste o mount em `.devcontainer/devcontainer.json`.
