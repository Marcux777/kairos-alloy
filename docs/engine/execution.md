# Modelo de Execução (Backtest Engine)

O engine do Kairos Alloy é **bar-based** (OHLCV). A execução é determinística e processada barra a barra.

Este documento descreve o modo `execution.model="complete"` (e como ele difere do modo `simple`).

## Ordem de eventos por barra

Para cada barra:

1. O engine processa ordens abertas (fills/cancels) na barra atual.
2. A estratégia (`Strategy::on_bar`) gera uma ação (BUY/SELL/HOLD).
3. O engine transforma a ação em uma ordem (conforme `execution.*`) e coloca na fila.
4. O engine registra o ponto de equity.

Isso preserva causalidade: a estratégia toma decisão **depois** de ver os fills na barra.

## Tipos de ordem (internos)

O engine usa apenas `action_type` e `size` para decidir ordens. Campos opcionais como
`confidence`, `model_version`, `latency_ms` e `reason` podem ser retornados pelo agente e
ficam disponíveis para auditoria/diagnóstico, mas não alteram a execução.

O tipo de ordem é decidido por configuração:

- `execution.buy_kind`: `market | limit | stop`
- `execution.sell_kind`: `market | limit | stop`

### Market

Executa (quando ativa) no `bar.open`.

### Limit

Regra determinística baseada em OHLC:

- BUY limit: fill se `bar.low <= limit_price`
  - se `bar.open <= limit_price`, preenche em `bar.open`
  - senão, preenche em `limit_price`
- SELL limit: fill se `bar.high >= limit_price`
  - se `bar.open >= limit_price`, preenche em `bar.open`
  - senão, preenche em `limit_price`

### Stop

Regra determinística baseada em OHLC:

- BUY stop: trigger se `bar.high >= stop_price`
  - se `bar.open >= stop_price`, preenche em `bar.open`
  - senão, preenche em `stop_price`
- SELL stop: trigger se `bar.low <= stop_price`
  - se `bar.open <= stop_price`, preenche em `bar.open`
  - senão, preenche em `stop_price`

## Referência de preço e offsets (bps)

O preço de `limit_price`/`stop_price` é calculado a partir de um preço de referência na barra onde a ordem é submetida:

- `execution.price_reference`: `close | open`

Offsets em bps:

- BUY limit: `ref * (1 - limit_offset_bps/10000)`
- SELL limit: `ref * (1 + limit_offset_bps/10000)`
- BUY stop: `ref * (1 + stop_offset_bps/10000)`
- SELL stop: `ref * (1 - stop_offset_bps/10000)`

## Latência (determinística)

`execution.latency_bars` define em quantas barras a ordem “aguarda” antes de ficar ativa.

Exemplo: `latency_bars=1` significa “next bar”.

`agent_latency_ms` (quando presente) é apenas auditado; não muda a execução.

## Custos: spread, slippage e fee

- `costs.fee_bps`: taxa (aplica sobre o valor financeiro da execução).
- `execution.spread_bps`: spread (aplica metade por lado).
- `costs.slippage_bps`: slippage (impacto adicional por lado).

Preço executado:

- BUY: `raw_price * (1 + (spread_bps/2 + slippage_bps)/10000)`
- SELL: `raw_price * (1 - (spread_bps/2 + slippage_bps)/10000)`

## Liquidez por volume (partial fills)

No modo `complete`, a quantidade máxima preenchida por barra é limitada por:

`bar.volume * execution.max_fill_pct_of_volume`

Se a ordem não puder ser preenchida por completo, ela pode gerar fills parciais ao longo de múltiplas barras.

## Time In Force (TIF) e expiração

`execution.tif`:

- `gtc`: mantém a ordem viva até preencher ou expirar
- `ioc`: preenche o que conseguir na primeira barra ativa e cancela o restante (ou cancela se não preencher nada)
- `fok`: preenche tudo na primeira barra ativa ou cancela (inclui checagem de volume/cash)

`execution.expire_after_bars` (opcional) define uma janela em barras (contando a partir da barra ativa) para a ordem ainda ser elegível.

## Limitações que continuam existindo

Mesmo no modo `complete`, o modelo ainda é simplificado:

- Sem order book/ticks (apenas OHLCV).
- Sem latência real baseada em tempo e sem fila por milissegundos (apenas “barras”).
- Sem ordens complexas (OCO/OTO/trailing/stop-limit).
- Long-only (sem short selling / margem).
