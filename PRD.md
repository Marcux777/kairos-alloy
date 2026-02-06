# Kairos Alloy PRD - MVP

**Sistema de backtesting e execução em Rust**
**Com integração a agente DRL e sentimento em Python**
**Versão:** v0.2 (rascunho)
**Data:** 26/01/2026
**Autor:** Marcus Vinicius Santos da Silva
**Orientação:** Maria Jose Pereira Dantas

**Mudanças na v0.2:** renomeação para Kairos Alloy; inclusão de mitigação para look-ahead bias em sentimento (sentiment_lag); recomendação de HTTP keep-alive e roadmap para gRPC/ZeroMQ; detalhamento de normalização no lado Python; divisão do milestone M2 em M2a/M2b.

## Sumário

- [1. Contexto e motivação](#1-contexto-e-motivação)
- [2. Problema a resolver](#2-problema-a-resolver)
- [3. Objetivos do produto](#3-objetivos-do-produto)
  - [3.1 Objetivo do MVP](#31-objetivo-do-mvp)
  - [3.2 Objetivos de pesquisa suportados](#32-objetivos-de-pesquisa-suportados)
- [4. Público-alvo e stakeholders](#4-público-alvo-e-stakeholders)
- [5. Escopo do MVP](#5-escopo-do-mvp)
  - [5.1 Em escopo (MVP)](#51-em-escopo-mvp)
  - [5.2 Fora de escopo (para depois do MVP)](#52-fora-de-escopo-para-depois-do-mvp)
- [6. Requisitos funcionais](#6-requisitos-funcionais)
  - [6.1 CLI e configuração](#61-cli-e-configuração)
  - [6.2 Ingestão de dados](#62-ingestão-de-dados)
  - [6.3 Pipeline de features (MVP)](#63-pipeline-de-features-mvp)
  - [6.4 Motor de backtesting e execução](#64-motor-de-backtesting-e-execução)
  - [6.5 Integração com agente (Python)](#65-integração-com-agente-python)
  - [6.6 Métricas e relatórios](#66-métricas-e-relatórios)
- [7. Requisitos não-funcionais](#7-requisitos-não-funcionais)
- [8. Arquitetura proposta (alto nível)](#8-arquitetura-proposta-alto-nível)
- [9. Critérios de sucesso do MVP](#9-critérios-de-sucesso-do-mvp)
- [10. Roadmap sugerido (até o MVP)](#10-roadmap-sugerido-até-o-mvp)
- [11. Riscos e mitigação](#11-riscos-e-mitigação)
- [12. Questões em aberto (para fechar na primeira sprint)](#12-questões-em-aberto-para-fechar-na-primeira-sprint)
- [13. Referência interna](#13-referência-interna)
- [14. Premissas e restrições](#14-premissas-e-restrições)
- [15. Dependências e integrações](#15-dependências-e-integrações)
- [16. Dados e formatos](#16-dados-e-formatos)
- [16.1 Formato OHLCV (PostgreSQL)](#161-formato-ohlcv-postgresql)
- [16.2 Formato de sentimento (CSV/JSON)](#162-formato-de-sentimento-csvjson)
- [16.3 Resampling e alinhamento temporal](#163-resampling-e-alinhamento-temporal)
- [16.4 Fonte OHLCV (KuCoin)](#164-fonte-ohlcv-kucoin)
- [17. Critérios de aceitação por módulo](#17-critérios-de-aceitação-por-módulo)
  - [17.1 CLI](#171-cli)
  - [17.2 Data](#172-data)
  - [17.3 Features](#173-features)
  - [17.4 Engine](#174-engine)
  - [17.5 Risk](#175-risk)
  - [17.6 Agents](#176-agents)
  - [17.7 Report](#177-report)
- [18. Requisitos não-funcionais detalhados](#18-requisitos-não-funcionais-detalhados)
- [19. Segurança e conformidade](#19-segurança-e-conformidade)
- [20. Testes e validação](#20-testes-e-validação)
- [21. Glossário](#21-glossário)
- [22. Direcionamento B2B e posicionamento](#22-direcionamento-b2b-e-posicionamento)
- [23. Oferta e packaging](#23-oferta-e-packaging)
- [24. Modelo comercial e precificação](#24-modelo-comercial-e-precificação)
- [25. Requisitos B2B (pós-MVP)](#25-requisitos-b2b-pós-mvp)
- [25.5 Plano de construção do produto B2B](#255-plano-de-construção-do-produto-b2b)
- [26. Notas regulatórias (Brasil)](#26-notas-regulatórias-brasil)
- [27. Go-to-market inicial (B2B)](#27-go-to-market-inicial-b2b)
- [28. Referências externas](#28-referências-externas)
- [29. Apêndice A — Exemplo de config.toml](#29-apêndice-a--exemplo-de-configtoml)
- [30. Apêndice B — Contrato do agente (exemplos)](#30-apêndice-b--contrato-do-agente-exemplos)
- [31. Apêndice C — Esquemas de artefatos](#31-apêndice-c--esquemas-de-artefatos)

## 1. Contexto e motivação

O projeto de IC investiga agentes de Deep Reinforcement Learning (DRL) para tomada de decisão no mercado de criptomoedas, incorporando indicadores derivados de um ensemble de LSTM/GRU e sinais de sentimento extraídos de redes sociais e tendências de busca. O plano de trabalho estabelece como resultado esperado a entrega de um software funcional e validado, capaz de operar em ambientes simulados e reais, com avaliação por métricas como Lucro líquido, Sharpe Ratio e Maximum Drawdown.

Este PRD define o MVP do sistema Kairos Alloy, implementado em Rust, responsável pela execução (simulada e real/paper), pelo motor de backtesting e pela integração com o agente em Python (treino/inferência).

## 2. Problema a resolver

No contexto do estudo, é necessário um sistema que permita: (i) executar backtests reprodutíveis com dados históricos; (ii) integrar sinais quantitativos e de sentimento; (iii) conectar um agente DRL treinado em Python a um motor de execução robusto; (iv) avaliar desempenho com métricas financeiras padronizadas e logs auditáveis.

## 3. Objetivos do produto

### 3.1 Objetivo do MVP

Entregar um aplicativo de Interface de Terminal (TUI) em Rust que centralize a execução de backtests e paper trading. A interação será puramente visual e interativa via teclado/mouse no terminal, eliminando a necessidade de decorare subcomandos de CLI. O sistema chamará o agente externo (Python), exibindo trades e métricas em tempo real em dashboards ricos.

### 3.2 Objetivos de pesquisa suportados

- Fornecer infraestrutura para testar e validar agentes DRL com variáveis de preço, indicadores técnicos, ensemble LSTM/GRU e sentimento.
- Permitir comparação contra estratégias baseline (ex.: buy-and-hold, média móvel) e contra benchmarks definidos no experimento.
- Gerar relatórios e artefatos para análise (curva de patrimônio, lista de operações, métricas).
- Habilitar otimização bayesiana de hiperparâmetros no pipeline de treino em Python, com avaliação paralela de trials para ampliar o espaço de busca de forma eficiente.

## 4. Público-alvo e stakeholders

- Usuário primário: estudante (pesquisador) executando experimentos e validações.
- Stakeholder acadêmico: orientador(a) e grupo de pesquisa.
- Usuário técnico: futuros contribuidores do laboratório (reuso do motor).

## 5. Escopo do MVP

### 5.1 Em escopo (MVP)

- Ingestão de dados históricos de preço (OHLCV) via API e armazenamento obrigatório em PostgreSQL (leitura do backtest somente pelo banco).
- Importação de dados de sentimento (séries temporais agregadas por janela) a partir de arquivos (CSV/JSON).
- Sincronização temporal entre preço e sentimento (resampling por timeframe configurável).
- Motor de backtesting baseado em barras (bar-based), com execução de ordens a mercado e custos (fee, slippage configuráveis).
- Gestão de portfólio (caixa, posição, PnL realizado/não-realizado).
- Regras de risco do MVP (limite de exposição, limite de drawdown, posição máxima).
- Integração com agente via API local (HTTP/JSON) com esquema de mensagens versionado.
- Estratégias baseline embutidas em Rust para sanity check (buy-and-hold e média móvel).
- Cálculo de métricas: Lucro líquido, Sharpe Ratio, Maximum Drawdown; exportação de curva de patrimônio e lista de trades.
- Relatório final em JSON/CSV (e opcionalmente HTML simples) + logs estruturados.
- **Interface TUI (Terminal User Interface):** Painel interativo para monitorar execução, PnL, logs e gráficos ASCII/Unicode em tempo real.

### 5.2 Fora de escopo (para depois do MVP)

- Execução com dinheiro real em corretora/exchange; integração direta com MetaTrader 5 em tempo real.
- Treino de modelos (DRL, LSTM/GRU, NLP) dentro do Rust.
- Otimização bayesiana de hiperparâmetros dentro do Rust (o caminho recomendado permanece no pipeline Python de treino).
- Suporte completo a múltiplos ativos e rebalanceamento de portfólio multiativo.
- Derivativos (futuros, margem, alavancagem), ordens limit/stop complexas.
- Qualquer interface alternativa fora da TUI.
- **WFA (Walk-Forward Analysis):** re-otimizar parâmetros periodicamente durante o teste (evitar overfitting).
- **Multi-exchange (Data Ingest):** abstrair a camada de conexão para plugar outras exchanges (ex.: Binance, Bybit).
- **Optimize (paralelo em Rust):** rodar backtests em massa (grid/random search) com paralelismo real para achar parâmetros ideais.

## 6. Requisitos funcionais

### 6.1 Interface de Usuário (TUI)

- O sistema deve iniciar diretamente em uma interface gráfica de terminal (TUI) baseada em `ratatui`.
- **Menu Principal:** Navegação intuitiva entre os modos: Backtest, Paper Trading, Validate, Reports, Settings.
- **Configuração Interativa:**
  - Carregar arquivos de configuração (`.toml`) via navegador de arquivos na TUI.
  - Editar parâmetros rápidos (ex.: capital inicial, timeframe) diretamente em formulários na tela antes de rodar.
- **Controle de Execução:** Botões ou atalhos para Start, Stop, Pause e Step-by-Step.

**Comportamento esperado**

- Ao executar o binário `kairos-alloy`, a TUI abre imediatamente.
- Argumentos de lançamento opcionais apenas para pré-carregar configs (ex.: `kairos-alloy --config config.toml` já abre a TUI com o setup pronto).
- Toda a saída de logs e status deve ser apresentada em painéis da TUI, não no stdout padrão (para não quebrar a interface).

### 6.2 Ingestão de dados

- Carregar OHLCV do PostgreSQL com timestamp UTC e campos: open, high, low, close, volume (e turnover quando disponível).
- Carregar série de sentimento com timestamp UTC e campos numéricos (ex.: score, volume_mencoes).
- Validar consistência: timestamps ordenados, duplicatas, gaps; gerar warnings e relatório de qualidade de dados.
- Aplicar regra de disponibilidade temporal para sentimento para evitar look-ahead bias: ao construir a observation no tempo t, usar apenas valores de sentimento com timestamp <= (t - sentiment_lag).
- A ingestão de OHLCV deve ser idempotente (upsert) com chave única por exchange/mercado/símbolo/timeframe/timestamp.

**Políticas sugeridas (MVP)**

- Duplicatas: registrar warning e manter o último registro por timestamp.
- Gaps: registrar warning; em modo “estrito” (`validate --strict`), falhar se gaps excederem um limite configurável.
- Timezone: qualquer input deve ser interpretado/convertido para UTC antes do processamento.

### 6.3 Pipeline de features (MVP)

- Gerar features básicas do preço: retorno log/percentual, volatilidade por janela, médias móveis simples (SMA), RSI (opcional).
- Acoplar features de sentimento já agregadas por janela (sem rodar NLP dentro do Rust).
- Gerar um vetor de observação (observation) padronizado para o agente externo.
- Normalização/escala: o Rust deve enviar features em formato cru ou em variações percentuais/retornos; o agente em Python aplica normalização usando os parâmetros salvos no treino (ex.: scaler).

### 6.4 Motor de backtesting e execução

- Simular execução a mercado na abertura da próxima barra (padrão), usando a action decidida no passo anterior; fees e slippage configuráveis.
- Suportar posição long-only no MVP (compra e venda do ativo), com fração do capital ou quantidade fixa.
- Registrar cada trade (timestamp, side, qty, price, fee, slippage, motivo/strategy_id).
- Calcular PnL e curva de patrimônio (equity curve) a cada barra.

### 6.5 Integração com agente (Python)

O MVP utilizará um serviço Python para inferência. O Rust envia observation e recebe action.

**Esquema mínimo de mensagens (JSON)**

- POST /v1/act: request contém run_id, timestamp, symbol, timeframe, observation[], portfolio_state{}; response contém action_type, size, confidence, model_version, latency_ms, reason.
- Versionamento por campo api_version e feature_version.
- Timeout configurável; fallback: HOLD e redução de risco em caso de falha.
- O cliente HTTP do Rust deve suportar conexão persistente (keep-alive) no MVP. Evoluções pós-MVP podem incluir gRPC ou ZeroMQ para reduzir overhead em backtests de alta velocidade.

**Campos (definição mínima)**

Request:

- `api_version` (string): versão da API (ex.: `"v1"`).
- `feature_version` (string): versão do vetor de features/observation.
- `run_id` (string): identificador da execução.
- `timestamp` (string): timestamp UTC (RFC3339 recomendado).
- `symbol` (string): ativo (ex.: `"BTCUSD"`).
- `timeframe` (string): granularidade (ex.: `"1m"`).
- `observation` (array[number]): vetor de features.
- `portfolio_state` (object): estado mínimo do portfólio.

`portfolio_state` (mínimo recomendado):

- `cash` (number): caixa atual.
- `position_qty` (number): quantidade do ativo em posição.
- `position_avg_price` (number): preço médio (se aplicável).
- `equity` (number): equity atual.

Response:

- `action_type` (string): `BUY`, `SELL`, `HOLD`.
- `size` (number): tamanho da ordem (interpretação definida pela estratégia/config: fração do capital ou qty).
- `confidence` (number, opcional): confiança do modelo (0–1).
- `reason` (string, opcional): justificativa curta (interpretabilidade/auditoria; não altera a execução).
- `model_version` (string, opcional): versão do modelo.
- `latency_ms` (number, opcional): latência observada no lado Python (para diagnóstico).

**Ferramentas oficiais de referência (repo)**

- `apps/agents/agent-dummy`: agente HTTP mínimo para smoke tests / golden path.
- `apps/agents/agent-llm`: agente LLM (ex.: Gemini) compatível com o contrato, com **cadência** (chamar o LLM a cada N barras) + cache **record/replay** para reduzir custo e tornar backtests determinísticos.

**LLM: desafios e mitigação (quando aplicável)**

- Latência: usar timeframes maiores e/ou cadência (ex.: 1 chamada a cada N barras).
- Custo: evitar chamada por barra; cache record/replay; amostragem por pontos-chave.
- Determinismo: temperature=0 e replay de cache por request-hash.
- Falhas: timeout + retry limitado + fallback HOLD + auditoria do erro.

### 6.7 Interface TUI (Core Experience)

- Bibliotecas: `ratatui` + `crossterm`.
- **Layout Fixo:**
  - **Sidebar:** Menu de navegação e status do sistema (DB connection, Agent status).
  - **Main Area:**
    - *Setup View:* Definição de parâmetros e escolha de dataset.
    - *Monitor View:* Gráficos de candlestick (ASCII/Braille), Equity Curve e lista de Trades recentes.
    - *Report View:* Tabelas de métricas e resumo pós-execução.
  - **Bottom Bar:** Logs em tempo real e barra de status/atalhos.
- **Input:** Navegação completa por teclado (setas, Tab, Enter, Esc) e suporte opcional a mouse.

### 6.6 Métricas e relatórios

- Lucro líquido: diferença entre capital final e capital inicial (considerando fees).
- Sharpe Ratio: baseado em retornos do portfólio; janela e taxa livre de risco configuráveis (padrão 0).
- Maximum Drawdown: maior queda pico-vale da curva de patrimônio.
- Exportar: trades.csv, equity.csv, summary.json, config_snapshot.toml, logs.jsonl.

**Definições operacionais (MVP)**

- Retorno por barra: `r_t = (equity_t / equity_{t-1}) - 1`.
- Sharpe (padrão): média/DP dos retornos por barra (ajuste anualizado configurável conforme timeframe).
- Drawdown: calculado sobre a curva de patrimônio (peak-to-trough).

## 7. Requisitos não-funcionais

- Reprodutibilidade: dado o mesmo dataset + config + respostas do agente, o backtest deve ser determinístico.
- Performance: processar pelo menos 500k barras (aprox. 1 ano de 1-min) em menos de 30s em modo release, em máquina padrão de notebook.
- Confiabilidade: falhas no agente externo não podem derrubar o processo; deve haver retry limitado e fallback.
- Observabilidade: logs estruturados (JSON) e métricas internas (tempo por etapa, latência do agente).
- Portabilidade: suporte a Linux e Windows.

## 8. Arquitetura proposta (alto nível)

Arquitetura modular em Rust, com separação clara entre domínio, infraestrutura e integração.

- core: tipos de domínio (Bar, Order, Trade, Portfolio, Metrics).
- data: ingestor de OHLCV (API) + persistência em PostgreSQL + validação.
- features: cálculo de indicadores e montagem do observation.
- engine: loop de backtest, execução simulada, geração de reward (opcional).
- risk: validação de ordens e limites.
- agents: estratégias internas + client HTTP para agente externo.
- report: exportação de artefatos (CSV/JSON/HTML).
- tui: camada de apresentação (Main Loop da interface, Widgets, Event Handling).
- main: entry-point que inicializa a TUI.
- (Removido: cli como driver principal).

## 9. Critérios de sucesso do MVP

- Rodar um backtest completo (ex.: BTCUSD) a partir de um dataset histórico no PostgreSQL, gerando artefatos de resultado.
- Rodar o mesmo backtest com estratégia baseline e com agente externo, permitindo comparação direta.
- Gerar métricas (Lucro líquido, Sharpe, Max Drawdown) e logs para auditoria.
- Executar paper trading com feed simulado em tempo real (replay) por no mínimo 1 hora sem falhas.

## 10. Roadmap sugerido (até o MVP)

Cronograma alinhado às fases de implementação/validação do plano de trabalho (implementação e testes até 06/2026; validação em ambiente real 07-08/2026).

| Milestone | Entrega | Conteúdo | Critério de aceite |
| --- | --- | --- | --- |
| M0 | Setup | Repo Rust + TUI base (App Main Loop) + logging | Compila e abre interface vazia com menu |
| M1 | Data | Ingestor OHLCV -> PostgreSQL + sentimento + validação | Carrega dataset e imprime resumo |
| M2a | Engine | Loop do backtest: iteração por barras + sincronização temporal + cálculo de indicadores/features | Itera sobre dataset e produz observation por barra (sem ordens) |
| M2b | Orders | Execução simulada + ordens (buy/sell/hold) + PnL + trades | Gera trades.csv e equity.csv com baseline |
| M3 | Metrics | Lucro, Sharpe, Max Drawdown + summary.json | Métricas batem com caso de teste |
| M4 | Agents | Integração na TUI: Status do agente e Latency Widget | TUI mostra status "Connected" e latência em tempo real |
| M5 | TUI Charts | Gráficos de Vela e Equity com `ratatui` widgets | Visualização gráfica funcional no terminal |
| M6 | Paper e Release | Integração final de paper trading com controles (Start/Stop) na TUI | Sistema completo controlado via interface |

## 11. Riscos e mitigação

- Alinhamento temporal dos dados (preço vs sentimento) e risco de look-ahead bias: mitigar com regras de disponibilidade, atraso configurável (sentiment_lag) e relatórios de sincronização.
- Diferença entre ambiente de treino (Python) e execução (Rust): mitigar com esquema de observation versionado e testes de equivalência.
- Latência/falhas do agente externo: mitigar com timeout, retry limitado e fallback (HOLD).
- Escopo grande para IC: mitigar com MVP estrito (long-only, 1 ativo, bar-based).

## 12. Questões em aberto (para fechar na primeira sprint)

Status em **2026-02-05**: decisões fechadas para o MVP.

- **Timeframe alvo do MVP:** `1min` como granularidade canônica de ingestão e execução. Timeframes maiores (`5min`, `15min`, `1h`) são derivados por resampling a partir de `1min`.
- **Fonte primária de OHLCV:** KuCoin Spot (`exchange=kucoin`, `market=spot`), com par padrão `BTC-USDT` no MVP; período-base recomendado para benchmark/reprodutibilidade: `2017-01-01T00:00:00Z` a `2025-12-31T23:59:59Z`.
- **Schema de banco (MVP):** manter `ohlcv_candles` conforme migration `0001`, sem particionamento no MVP, com `PRIMARY KEY (exchange, market, symbol, timeframe, timestamp_utc)` e índice `(symbol, timeframe, timestamp_utc)`.
- **Conjunto mínimo de features (MVP):** `return_mode=log`, `sma_windows=[10,50]`, `volatility_windows=[10]`, `rsi_enabled=false`, sentimento como `score` numérico único (com suporte a múltiplas colunas sem alterar contrato).
- **Política de custos (MVP):** fee e slippage percentuais em bps por execução (`costs.fee_bps`, `costs.slippage_bps`) e spread opcional (`execution.spread_bps`), com defaults recomendados `fee_bps=10`, `slippage_bps=5`, `spread_bps=0`.
- **Vetor de observation (v1):** ordem fixa `retorno`, `SMA` (na ordem de `sma_windows`), `volatilidade` (na ordem de `volatility_windows`), `RSI` (se habilitado), `sentimento` (na ordem do schema carregado). Tipo numérico: `f64`. Normalização permanece no agente Python.
- **Protocolo do agente (MVP):** HTTP/JSON com keep-alive (`POST /v1/act`, opcional `POST /v1/act_batch`), versionado por `api_version=v1` e `feature_version=v1`; fallback em falha: `HOLD`.
- **Regra final de sentiment_lag:** default operacional `5m`; para cada barra `t`, usar apenas sentimento com `timestamp <= (t - sentiment_lag)`.
- **HPO para treino (pós-MVP):** otimização bayesiana no pipeline Python, com avaliação paralela por `study.parallelism` (threads), artefato versionado com histórico de trials e melhores hiperparâmetros.

### Tabela de decisões fechadas

| Questão | Decisão (MVP) | Impacto | Responsável |
| --- | --- | --- | --- |
| Timeframe alvo do MVP (1-min, 5-min, 1h) para o backtest principal | `1min` canônico; timeframes maiores por resampling | Define granularidade, volume de dados e performance do engine | Produto + Engenharia Kairos |
| Fonte primária de OHLCV (MT5 vs KuCoin Spot/Futures), pares e período | KuCoin Spot, `BTC-USDT`, janela base 2017-01-01..2025-12-31 (UTC) | Define pipeline de coleta e volume de dados | Data/Infra Kairos |
| Schema do banco de OHLCV (campos, tipos, particionamento e índices) | `ohlcv_candles` da migration `0001`, sem particionamento no MVP | Afeta ingestão, performance e compatibilidade | Data/Infra Kairos |
| Conjunto mínimo de features para observation e representação de sentimento | Retorno log + SMA[10,50] + Vol[10] + sentimento `score` (RSI opcional) | Base da interface com o agente e comparabilidade de experimentos | Engine + Research Kairos |
| Política de slippage e fee (fixo, percentual, spread) | Custos percentuais em bps (`fee_bps`, `slippage_bps`) + `spread_bps` opcional | Impacta métricas e realismo do backtest | Engine Kairos |
| Definição final do vetor observation (campos, ordem, tipos, normalização) | Ordem fixa v1; `f64`; normalização no Python | Contrato entre Rust e Python; reduz risco de retrabalho | Engine + Agents Kairos |
| Protocolo do agente em produção (HTTP vs gRPC/ZeroMQ) | HTTP/JSON keep-alive no MVP; reavaliar pós-MVP via profiling/throughput | Latência e throughput em backtests rápidos | Agents/Platform Kairos |
| Definição de sentiment_lag e regra de alinhamento sentimento->barra | `sentiment_lag=5m` (default), com regra `ts_sent <= ts_bar - lag` | Evita look-ahead bias e garante reprodutibilidade | Research + Engine Kairos |
| Estratégia de HPO para treino de modelos | Otimização bayesiana em Python com avaliação paralela por threads e artefato de trials | Escalabilidade de busca e reprodutibilidade de tuning | Research + Agents Kairos |

## 13. Referência interna

Baseado no Plano de Trabalho da IC “Agentes de Deep Reinforcement Learning no Mercado de Criptomoedas” (seções: Objetivo Geral/Específicos, Métodos, Resultados Esperados e Cronograma).

## 14. Premissas e restrições

- O MVP é **single-asset**, **long-only** e **bar-based**.
- Integração com agente via **HTTP/JSON** no MVP.
- OHLCV é armazenado e consultado via **PostgreSQL** (obrigatório).
- Dados históricos e de sentimento são fornecidos pelo pesquisador (dataset externo).
- Não há execução com dinheiro real no MVP.
- O alvo principal é ambiente **Linux/Windows**.
- O processo deve ser reproduzível com o mesmo dataset + config.

## 15. Dependências e integrações

- Fonte de dados OHLCV: coleta via API (ex.: KuCoin) com ingestão obrigatória em PostgreSQL.
- Banco de dados: PostgreSQL (obrigatório no MVP).
- Fonte de dados de sentimento: arquivos CSV/JSON agregados por janela.
- Serviço Python de inferência do agente (HTTP local).
- Configuração central via arquivo TOML.

## 16. Dados e formatos

### 16.1 Formato OHLCV (PostgreSQL)

Tabela recomendada: `ohlcv_candles`.

Campos obrigatórios:

- `exchange` (text)
- `market` (text) — `spot` | `futures`
- `symbol` (text)
- `timeframe` (text)
- `timestamp_utc` (timestamptz, sempre em UTC)
- `open` (double precision)
- `high` (double precision)
- `low` (double precision)
- `close` (double precision)
- `volume` (double precision)
- `turnover` (double precision, opcional)
- `source` (text, ex.: `kucoin`, `mt5`)
- `ingested_at` (timestamptz, default `now()`)

Restrições e convenções (MVP):

- Chave única: `(exchange, market, symbol, timeframe, timestamp_utc)`.
- Ordenação por `timestamp_utc` deve ser estrita no consumo do backtest.
- Registros com `close` inválido: descartar com warning.
- Timestamps sempre convertidos para UTC antes de inserir.

Índices sugeridos:

- `UNIQUE(exchange, market, symbol, timeframe, timestamp_utc)`.
- `INDEX(symbol, timeframe, timestamp_utc)`.

### 16.2 Formato de sentimento (CSV/JSON)

Campos obrigatórios: `timestamp_utc` e métricas numéricas (ex.: `score`, `volume_mencoes`).

Restrições e convenções (MVP):

- `timestamp_utc` em UTC.
- Valores ausentes: registrar warning; política de preenchimento deve ser explícita (ex.: `forward_fill`/`zero_fill`/`drop`).

### 16.3 Resampling e alinhamento temporal

- Resampling por timeframe configurável.
- Alinhamento sentimento->barra usando `sentiment_lag`.
- Relatório de gaps e duplicatas.

### 16.4 Fonte OHLCV (KuCoin)

**Spot REST (histórico):**

- Endpoint: `GET /api/v1/market/candles` (base Spot).
- Parâmetros: `symbol`, `type`, `startAt`, `endAt` (epoch em segundos).
- Timeframes suportados: `1min`, `3min`, `5min`, `15min`, `30min`, `1hour`, `2hour`, `4hour`, `6hour`, `8hour`, `12hour`, `1day`, `1week`, `1month`.
- Limite por chamada: até 1500 candles; paginar por janela de tempo.
- Resposta: `time`, `open`, `close`, `high`, `low`, `volume`, `turnover`.

**Futures REST (opcional, se usado no MVP futuro):**

- Endpoint: `GET https://api-futures.kucoin.com/api/v1/kline/query`.
- A documentação mostra uso de `symbol`, `granularity`, `from`, `to` (epoch em milissegundos) no exemplo.
- Limite por chamada: até 500 candles; paginar por janela de tempo.

**Qualidade de dados:**

- Candles podem estar incompletos quando não há trades no intervalo; tratar como gap.
- Para dados em tempo real, preferir WebSocket; para histórico, usar REST com paginação.

**Observação operacional:**

- Klines (spot/futures) usam peso 3 no pool público; respeitar o rate limit e aplicar backoff em caso de 429.
- Persistir via upsert na tabela `ohlcv_candles` para garantir idempotência.

**Evolução pós-MVP (multi-exchange / conectores plugáveis)**

- Definir uma porta/interface de “Exchange OHLCV Provider” para isolar: paginação, timeframes suportados, rate limit/backoff e normalização do payload.
- KuCoin vira uma implementação; futuras: Binance (spot) e Bybit (spot/derivativos quando fizer sentido).
- Persistência no PostgreSQL permanece a mesma (chave única por exchange/market/symbol/timeframe/timestamp).

## 17. Critérios de aceitação por módulo

### 17.1 TUI/UX

- O binário abre o menu principal.
- É possível navegar para "Backtest", carregar um config e iniciar a execução.
- Durante a execução, o dashboard atualiza equity e trades sem "flicker".
- Logs de erro aparecem na área de logs da interface.

### 17.2 Data

- Ingere OHLCV no PostgreSQL e consulta para backtest sem erro.
- Carrega sentimento sem erro.
- Emite relatório de qualidade de dados (gaps, duplicatas, ordenação).

### 17.3 Features

- Gera vetor de observation por barra.
- Inclui indicadores básicos e sentimento conforme config.

### 17.4 Engine

- Itera por barras e produz ações determinísticas com baseline.
- Registra trades e curva de patrimônio.

### 17.5 Risk

- Aplica limites de exposição, drawdown e posição máxima.
- Bloqueia ordens inválidas e registra motivo.

### 17.6 Agents

- Baseline interno executa sem dependências externas.
- Cliente HTTP envia observation e recebe action.
- Fallback HOLD em timeout.
- O agente pode retornar `reason` (opcional) para interpretabilidade/auditoria; o engine usa apenas `action_type` e `size`.
- Existe “golden path” validado com `apps/agents/agent-dummy`.
- Existe agente LLM de referência (`apps/agents/agent-llm`) com modo mock (sem API key) e modo live.

### 17.7 Report

- Exporta `trades.csv`, `equity.csv`, `summary.json`, `config_snapshot.toml`, `logs.jsonl`.

## 18. Requisitos não-funcionais detalhados

- **Reprodutibilidade:** execução determinística com seed fixa quando aplicável.
- **Performance:** 500k barras em < 30s (release) em notebook padrão.
- **Confiabilidade:** retries limitados para o agente; processo não cai em falhas transitórias.
- **Observabilidade:** logs JSON estruturados e tempos por etapa.
- **Portabilidade:** Linux e Windows suportados.

## 19. Segurança e conformidade

- Sem credenciais sensíveis embutidas no MVP.
- Execução local sem exposição externa.
- Logs sem dados pessoais.

## 20. Testes e validação

- Testes unitários para métricas (Lucro, Sharpe, Max Drawdown).
- Testes de integração para ingestão em PostgreSQL e leitura de sentimento (CSV/JSON).
- Teste de fumaça do CLI (`backtest` e `paper` com dataset mínimo).

## 21. Glossário

- **OHLCV:** Open, High, Low, Close, Volume.
- **Backtest:** simulação de estratégia em dados históricos.
- **Paper trading:** execução simulada em tempo real.
- **Observation:** vetor de entrada enviado ao agente externo.
- **Sentiment lag:** atraso aplicado ao sinal de sentimento para evitar look-ahead bias.

## 22. Direcionamento B2B e posicionamento

Este documento assume o direcionamento **B2B de infraestrutura** (motor de backtest/paper/execução simulada), por apresentar menor atrito regulatório do que ofertas que **recomendam** investimentos ao investidor final.

### 22.1 O que o Kairos Alloy é (B2B)

- Motor de backtesting determinístico, com execução simulada bar-based, custos (fee/slippage) e regras de risco.
- Ferramenta para **validação e auditoria** de estratégias/modelos (inclusive DRL) com artefatos e logs.
- Integração padronizada Rust ↔ Python para inferência (contrato versionado).

### 22.2 O que o Kairos Alloy não é (no MVP e no posicionamento B2B)

- Não é um produto de recomendação personalizada ao investidor final.
- Não é um “robô-consultor” e não realiza consultoria automatizada.
- Não executa ordens com dinheiro real em nome de terceiros (fora de escopo do MVP).

### 22.3 ICP (Ideal Customer Profile) e stakeholders B2B

Clientes com alta aderência ao MVP/produto:

- Mesas proprietárias (prop desks) e times quant pequenos.
- Gestoras pequenas e consultorias quantitativas (backtest e auditoria).
- Research houses/fintechs que precisam de pipeline reprodutível e logs.

Papéis compradores/usuários típicos:

- Comprador: CTO/Head de Trading/Head de Research/Tech Lead.
- Usuário: quant researcher/engenheiro de dados/trader sistemático.

### 22.4 Proposta de valor (diferenciais)

- **Reprodutibilidade + auditoria**: configuração snapshot + logs JSONL + artefatos padronizados.
- **Integração pronta com modelos**: contrato de inferência versionado com Python.
- **Performance**: execução em Rust com foco em throughput e determinismo.
- **Sane defaults**: baselines internas para validação rápida e comparação.

## 23. Oferta e packaging

### 23.1 Componentes entregáveis (produto)

- Binário Único (`kairos-alloy`) que contém a aplicação TUI.
- Bibliotecas internas (crates) reutilizáveis (core/data/features/engine/risk/agents/report/cli).
- Esquema versionado do contrato com o agente (JSON) e exemplos de requests/responses.
- Agentes de referência (repo): `apps/agents/agent-dummy` e `apps/agents/agent-llm` (para validação e integração).
- Ferramentas de reprodutibilidade: `platform/ops/scripts/compare_runs.py` (comparação determinística de artefatos entre runs).
- Experimentos: `--mode sweep` (grid de parâmetros + splits + leaderboard).

### 23.2 Modos de entrega (B2B)

- **Self-hosted (prioritário no B2B):** cliente executa o motor localmente/on-prem e integra ao seu agente.
- **Distribuição por releases:** binários por plataforma + checksums, e (opcional) imagem Docker.

### 23.3 Artefatos gerados por execução

Estrutura recomendada de diretório por run (run_id):

- `summary.json` (métricas e metadados)
- `trades.csv` (trades executados)
- `equity.csv` (curva de patrimônio)
- `config_snapshot.toml` (config congelada)
- `logs.jsonl` (trilha auditável)

## 24. Modelo comercial e precificação

### 24.1 O que é vendido (B2B infra)

- Licença de uso do motor (self-hosted) + suporte.
- Serviços opcionais: onboarding técnico, integrações e hardening (observabilidade, segurança, performance).

### 24.2 Planos (sugestão pragmática)

- **Starter:** licença para 1 time/1 ambiente + suporte assíncrono.
- **Pro:** SLA básico + suporte prioritário + features avançadas (ex.: exportadores adicionais, validadores, profiling).
- **Enterprise:** SLA mais forte + suporte dedicado + requisitos de compliance/segurança e customizações.

### 24.3 Métricas de valor (para pricing)

- Tempo para executar um backtest reprodutível (setup → artefatos).
- Throughput de barras processadas/segundo e latência de integração com agente.
- Qualidade de auditoria (completude de logs, rastreabilidade de decisões).

## 25. Requisitos B2B (pós-MVP)

Se o objetivo for evoluir o MVP para um produto B2B comercializável, os requisitos abaixo tendem a virar prioridade.

### 25.1 Estabilidade e versionamento

- Versionamento semântico (SemVer) da Aplicação.
- Compatibilidade retroativa por janela (ex.: suportar N-1 versões do contrato).
- Changelog por release com breaking changes explícitas.

### 25.2 Operabilidade

- Observabilidade: métricas de latência do agente, tempo por etapa e contadores de falhas/retries.
- Logs estruturados com campos padronizados (run_id, timestamp, stage, symbol, action, error).
- Modo “diagnóstico” para perfis de performance (profiling e estatísticas por etapa).

### 25.3 Segurança (produto B2B)

- Assinatura/verificação de releases (checksums) e rastreabilidade de build.
- Política de tratamento de dados: onde ficam datasets, retention e controle de acesso (quando aplicável).
- Hardening da integração HTTP local (timeouts, limites, validação de schema).

### 25.4 Suporte a integração

- Agente “dummy” oficial para testes (healthcheck + respostas determinísticas).
- Ferramentas para validar equivalência de observation (Rust vs Python) e detectar drift.

### 25.5 Plano de construção do produto B2B

Este plano documenta a **construção do sistema** com foco no caminho mais simples para uma startup: **B2B infra self-hosted** (licença/on-prem), priorizando *reprodutibilidade, auditoria, integração e operabilidade* — e evitando entrar em oferta de recomendação ao investidor final.

#### 25.5.1 Princípios do produto (B2B infra)

- **Infra, não recomendação:** o produto não “indica oportunidades”, não otimiza para persuadir usuário final e não se posiciona como consultoria/gestão.
- **Determinismo primeiro:** mesma entrada + mesma config + mesmas respostas do agente ⇒ mesmos resultados.
- **Contrato estável:** o “API/feature contract” com o agente é versionado e retrocompatível por janela.
- **Observabilidade por padrão:** logs estruturados e métricas internas para diagnóstico e auditoria.
- **Self-hosted por padrão:** reduzir fricção (segurança de dados, compliance do cliente, e adoção).

#### 25.5.2 Requisitos de produto “vendável” (além do MVP acadêmico)

Checklist mínimo para “produto B2B”:

- Instalação reprodutível (binários/artefatos) e documentação de setup.
- “Golden path” com exemplo: rodar um backtest via TUI com dataset mínimo + agente dummy e gerar artefatos.
- Contrato do agente documentado com schema e exemplos (request/response).
- Logs/artefatos com formato estável e versionado (ou compatível por janela).
- Documentação operacional: parâmetros de performance, tuning, troubleshooting.

#### 25.5.3 Roadmap de construção (pós-MVP → B2B self-hosted)

Fases sugeridas (orientativas). Cada fase tem entregáveis e critérios de aceite.

**Fase A — Hardening do MVP (qualidade e “comercializável”)**

Entregáveis:

- Congelar `api_version` e `feature_version` do contrato do agente (ex.: `v1`).
- Modo `validate` completo (qualidade de dados + relatório).
- Saída padronizada por run (run_id + `config_snapshot.toml` + artefatos + logs).
- Baselines robustas (sanity check) e testes de métricas.

Critérios de aceite:

- Na TUI, o usuario consegue rodar um backtest de ponta a ponta (com agente dummy) e gerar `summary.json`, `trades.csv`, `equity.csv`, `logs.jsonl`.
- Na TUI, o usuario consegue rodar validacao de dados (duplicatas/gaps/ordenamento) e obter um relatorio consistente.
- O backtest é determinístico com o mesmo input.

**Fase B — Packaging e distribuição (self-hosted)**

Entregáveis:

- Releases com binários (Linux/Windows) e checksums.
- (Opcional) imagem Docker para padronizar runtime.
- Metadados de build (versão, commit, data) exibidos na TUI.
- “Quickstart” de 10 minutos para rodar um backtest.

Critérios de aceite:

- Um usuário novo consegue instalar e rodar o “golden path” seguindo apenas a doc.
- Releases têm integridade verificável (checksums publicados).

**Fase C — Operabilidade e suporte (nível B2B)**

Entregáveis:

- Logs estruturados com campos mínimos padronizados (run_id, timestamp, stage, symbol, action, error).
- Métricas internas por etapa (parsing, features, engine, latency do agente).
- Modo “diagnóstico” para profiling leve (contagens, tempos e percentis).
- Playbook de troubleshooting (erros comuns e como resolver).

Critérios de aceite:

- Em caso de falha do agente, o processo **não cai**, registra causa e aplica fallback definido.
- É possível identificar gargalo (ex.: latência do agente vs features) via logs/métricas.

**Fase D — Integração com clientes (design partners)**

Entregáveis:

- Template de `config.toml` por caso de uso (backtest/paper).
- Guia de integração do agente (schema + exemplos + checklist).
- Dataset de exemplo e agente dummy oficial para validação de ponta a ponta.
- Checklist de aceite do piloto com o cliente.

Critérios de aceite:

- O cliente integra seu agente com base no contrato `v1` e roda um piloto com logs auditáveis.

**Fase E — Experimentação avançada (Optimize + WFA)**

Entregáveis:

- Evoluir `sweep` para honrar `parallelism` com execução concorrente real (rodar backtests em massa com saturação de CPU).
- Otimização (grid/random search) com leaderboard, export dos melhores parâmetros e artefatos agregados.
- Adicionar otimização bayesiana para treino de modelos (pipeline Python), com aquisição EI e avaliação paralela por threads para explorar hiperparâmetros de forma mais eficiente.
- Implementar `walkforward` (WFA): por janela, rodar otimização in-sample, escolher best params, avaliar out-of-sample e repetir em janelas rolantes.
- Artefatos do WFA (por run/sweep): `wfa_manifest.json`, `folds.csv`, `leaderboard.csv`, `best_params_by_fold.toml` e `summary.json` agregado.

Critérios de aceite:

- Mesma entrada + mesmo grid + mesma seed ⇒ mesmos resultados (determinismo).
- Throughput melhora ao aumentar `parallelism` (até saturar CPU).
- Otimização bayesiana produz artefato com histórico de trials, melhor score e melhores hiperparâmetros para reproducibilidade do treino.
- WFA gera métricas por fold e agregado, e permite comparar “WFA vs parâmetros fixos”.

**Fase F — Conectores de dados (multi-exchange)**

Entregáveis:

- Abstrair a camada de conexão do ingestor (porta/interface) para suportar múltiplas exchanges com a mesma semântica (timeframes, paginação, backoff).
- KuCoin permanece como adapter inicial; adicionar Binance (spot) como segundo adapter; Bybit como terceiro (opcional).
- Normalização e validação padronizadas (campos, timezone UTC, gaps/duplicatas/out-of-order) antes de persistir.

Critérios de aceite:

- Ingest funciona para o mesmo período/símbolo/timeframe em pelo menos 2 exchanges, gerando OHLCV consistente no DB.
- Idempotência (upsert) preservada e validada (re-ingest não duplica candles).

#### 25.5.4 Requisitos de engenharia para “self-hosted B2B”

**Compatibilidade e upgrades**

- SemVer para o produto; breaking changes somente com major version.
- Política de compatibilidade do contrato do agente (ex.: suportar `v1` e `v1.1` por N releases).

**Empacotamento**

- Binários assinado/verificado (quando aplicável) + checksums.
- Dependências mínimas e documentação de requisitos de sistema.

**Operação**

- Flags/config para ajustar performance (buffers, batch size, paralelismo quando aplicável).
- Timeouts e limites para o cliente HTTP do agente (evitar travamentos).

**Segurança**

- Validação de inputs (CSV/JSON) e limites de tamanho.
- Validação de schema do JSON do agente (campos obrigatórios + tipos).

#### 25.5.5 Entregáveis de documentação (startup B2B)

Documentos mínimos recomendados (podem ficar no repositório):

- `PRD.md` (este documento): visão do produto e requisitos.
- Guia de uso: instalação, quickstart, exemplos, troubleshooting.
- Guia do contrato do agente: versões, schemas, exemplos e compatibilidade.
- Guia de artefatos: formatos, campos e estabilidade.
- Política de versionamento e changelog por release.

## 26. Notas regulatórias (Brasil)

Esta seção é **informativa** e não substitui orientação jurídica. O enquadramento regulatório pode depender do **desenho do produto**, da comunicação e do fluxo operacional.

Como diretriz de produto para reduzir atrito regulatório no início:

- Posicionar o Kairos Alloy como **infraestrutura** (execução/backtest/paper + logs), não como recomendação.
- Evitar mensagens de “oportunidade de compra/venda” ao investidor final.
- Fornecer transparência e trilha auditável (o que foi executado, quando, com quais parâmetros).

## 27. Go-to-market inicial (B2B)

### 27.1 Estratégia de validação

- Fechar 1–3 “design partners” (times quant/consultorias/gestoras pequenas) para validar dores e requisitos.
- Entregar um piloto com dataset e agente dummy, priorizando reprodutibilidade + relatórios.

### 27.2 Canal e aquisição

- Parcerias com consultorias e comunidades quant.
- Conteúdo técnico (benchmark de performance, guia de reprodutibilidade, exemplos Rust↔Python).

### 27.3 Entregáveis de venda (B2B)

- Demo reprodutível: “rodar backtest → gerar artefatos → comparar baseline vs agente”.
- Checklist de auditoria e qualidade de dados.
- Documento de integração do contrato do agente (schemas + exemplos).

## 28. Referências externas

- Robôs de investimentos — Portal do Investidor: https://www.gov.br/investidor/pt-br/investir/como-investir/profissionais-do-mercado/robos-de-investimentos
- Registrar Consultor — CVM (Gov.br): https://www.gov.br/pt-br/servicos/registrar-consultor-cvm
- ANBIMA: notícia sobre esclarecimentos da CVM para consultores (Ofício Circular 2/2026): https://www.anbima.com.br/pt_br/noticias/cvm-atende-pedido-da-anbima-e-esclarece-regras-para-consultores-de-valores-mobiliarios-em-novo-oficio.htm

## 29. Apêndice A — Exemplo de config.toml

```toml
[run]
run_id = "btc_1m_2017_2025"
symbol = "BTCUSD"
timeframe = "1m"
initial_capital = 10000.0

[db]
# Recommended: omit db.url from versioned configs and provide it via env:
# export KAIROS_DB_URL="postgres://kairos:<password>@localhost:5432/kairos"
# url = "postgres://kairos:CHANGE_ME@localhost:5432/kairos"
ohlcv_table = "ohlcv_candles"

[paths]
sentiment_path = "data/sentiment.csv"
out_dir = "runs/"

[costs]
fee_bps = 10.0        # 0.10%
slippage_bps = 5.0    # 0.05%

[risk]
max_position_qty = 1.0
max_drawdown_pct = 0.30
max_exposure_pct = 1.00

[features]
return_mode = "log"          # "log" | "pct"
sma_windows = [10, 50]
rsi_enabled = false
sentiment_lag = "5m"

[agent]
mode = "remote"              # "baseline" | "remote"
url = "http://127.0.0.1:8000"
timeout_ms = 200
retries = 1
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"
```

## 30. Apêndice B — Contrato do agente (exemplos)

### 30.1 Request (exemplo)

```json
{
  "api_version": "v1",
  "feature_version": "v1",
  "run_id": "btc_1m_2017_2025",
  "timestamp": "2026-01-01T00:00:00Z",
  "symbol": "BTCUSD",
  "timeframe": "1m",
  "observation": [0.0012, 0.0031, 0.12, 0.08],
  "portfolio_state": {
    "cash": 9950.0,
    "position_qty": 0.1,
    "position_avg_price": 45000.0,
    "equity": 10020.0
  }
}
```

### 30.2 Response (exemplo)

```json
{
  "action_type": "HOLD",
  "size": 0.0,
  "confidence": 0.62,
  "reason": "No trade: insufficient signal / uncertain regime.",
  "model_version": "drl-2026-01-01",
  "latency_ms": 12
}
```

## 31. Apêndice C — Esquemas de artefatos

### 31.1 `trades.csv` (colunas recomendadas)

- `timestamp_utc`
- `symbol`
- `side` (BUY/SELL)
- `qty`
- `price`
- `fee`
- `slippage`
- `strategy_id`
- `reason`

### 31.2 `equity.csv` (colunas recomendadas)

- `timestamp_utc`
- `equity`
- `cash`
- `position_qty`
- `unrealized_pnl`
- `realized_pnl`

### 31.3 `summary.json` (conteúdo mínimo)

- Metadados da execução: `run_id`, `symbol`, `timeframe`, `start`, `end`.
- Config de custos e risco (snapshot).
- Métricas: `net_profit`, `sharpe`, `max_drawdown`.
