# ARTIGO 5 - Representação do Preço para DRL em Criptomoedas - Orçamento de Complexidade e Auditoria Anti-Seleção

> Proposta: um paper de **inferência sob restrições operacionais** em que o objeto é a **representação endógena do preço** sob orçamento de complexidade e correção anti-seleção. O inimigo é o placebo de indicadores + seleção disfarçada.

## Título (para submissão; provisório)
**Representações endógenas do preço para DRL em cripto**: auditoria sob orçamento de complexidade e controles placebo (BTC/ETH 1h, 2017–2024)

## Resumo (rascunho)
Em DRL para trading, empilhar dezenas de indicadores costuma aumentar graus de liberdade e produzir ganhos frágeis que dependem de seeds e de seleção oportunista. Este paper faz uma auditoria confirmatória de **representações endógenas do preço** (retornos/normalizações, indicadores clássicos, filtros/transformadas contidas e estrutura temporal simples) sob um orçamento de complexidade congelado (mesma dimensionalidade do estado, mesma arquitetura, mesmo budget de treino) e sob o protocolo rígido do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]]. A única variável experimental é o “mapa do estado”. O resultado primário é estabilidade (seeds/janelas/anos) e sensibilidade a custos, com contraste “bruto vs ajustado” (bootstrap dependente, PSR/DSR e Reality Check/SPA) e controles placebo por dimensão. Dados exógenos (notícias/on-chain com latência) são escopo do [[ARTIGO 3 - Informação Exógena Observável no Tempo para DRL em Criptomoedas - Latência e Valor Marginal]].

## 1. Introdução (problema real: representação como fonte de seleção)
O debate útil não é “indicadores funcionam”, mas se uma representação reduz overfitting e aumenta estabilidade sob o mesmo protocolo. Se uma representação “vence” apenas porque tem mais dimensões ou mais tuning, isso é seleção, não evidência.

## 2. Lacuna (precisa e defensável)
Há literatura de indicadores técnicos e práticas “default” de empilhar features em DRL; o que falta é uma auditoria confirmatória, no recorte BTC/ETH 1h 2017–2024, que compare representações endógenas sob **orçamento de complexidade** e reporte “bruto vs ajustado” com controles placebo, tratando estabilidade como resultado primário.

## 3. Questão de pesquisa e hipóteses (falsificáveis)
### 3.1 Questão central
Sob o mesmo protocolo, qual representação endógena do preço minimiza overfitting e maximiza estabilidade (seeds/janelas/anos) sob custos/slippage realistas?

### 3.2 Hipóteses testáveis
**H1 (indicadores clássicos não dominam):** sob orçamento de complexidade fixo, indicadores clássicos não superam de forma robusta retornos/normalizações simples.

**H2 (placebo sob correção formal):** após PSR/DSR + Reality Check/SPA, parte relevante das representações torna-se indistinguível de controles placebo por dimensão.

**H3 (decaimento temporal):** representações baseadas em padrões endógenos simples exibem degradação ano a ano (2017→2024), indicando fragilidade sob não-estacionariedade.

## 4. Contribuições (artefato + inferência)
### 4.1 Artefato
- Biblioteca/pipeline de representações endógenas do preço (conjunto fechado) com dimensionalidade controlada.
- Controles placebo por dimensão + checklist anti-seleção, acoplados ao protocolo do ARTIGO 2.

### 4.2 Inferência
- Comparação por representação com distribuição por seed, decomposição por regime, sensibilidade a custos e contraste “bruto vs ajustado” (PSR/DSR + RC/SPA).

## 5. Métodos: protocolo congelado (pré-registro interno)
### 5.1 O que é inviolável (herdado do ARTIGO 2)
Tudo abaixo é **fixo**:
- Ambiente, splits walk-forward, regimes, seeds e cenários de custo.
- Arquitetura do agente, orçamento de treino e tuning (mesmo procedimento, mesmo budget).
- Métricas e estatística (bootstrap dependente + PSR/DSR + RC/SPA).

**Única variável experimental:** a representação endógena do preço (mapa do estado).

**Resumo mínimo do protocolo (para autocontenção):**
- Dados: BTC e ETH em 1h (2017–2024), sem vazamento temporal (normalizações por estatísticas do treino).
- Splits walk-forward: treino 24m / validação 6m / teste 6m, step 6m.
- Ações/posição: \(a_t \\in\\{-1,0,1\\}\), posição 1×, execução close-to-close.
- Reward principal: \(R_t = p_{t-1} r_t - c|p_t-p_{t-1}|\), com \(c\\in\\{2,10,25\\}\\) bps.
- Seeds: tuning (0,1) e avaliação no teste \(S=10\) seeds (2–11), fixos e compartilhados.
- Regimes: \(W=720h\), k-means \(k=3\) em (retorno \(R^{(W)}\), volatilidade \(\\sigma^{(W)}\)) ajustado no treino; bull/bear/sideways por ordenação do retorno.

### 5.2 Orçamento de complexidade (pré-fixado)
- Fixar dimensionalidade do estado: toda representação produz exatamente \(d=12\) features (mesmo \(d\) para todas), sem seleção pós-hoc (estado deliberadamente contido para reduzir graus de liberdade).
- Fixar janelas/horizontes e parametrizações **canônicas** (abaixo), sem grade/tuning de indicadores.
- Transformações **apenas causais**: cada feature em \(t\) usa somente dados até \(t\) (nenhum filtro centrado/simétrico).
- Normalização sem vazamento: padronização por estatísticas do treino (por fold) e aplicação idêntica em validação/teste.

### 5.3 Representações endógenas (tratamentos confirmatórios)
Conjunto fechado e contido:
1. **R0 (retornos/risco/volume; \(d=12\)):**
   - Retornos log: \(r^{(1)}_t, r^{(4)}_t, r^{(24)}_t, r^{(168)}_t\).
   - Volatilidade rolling: \(\\sigma^{(24)}_t, \\sigma^{(168)}_t\).
   - Range intrabarra: \(\\log(H_t/L_t)\).
   - Volume: \(\\log(Vol_t)\), \(\\Delta\\log(Vol)_{1h}\\), \(\\Delta\\log(Vol)_{24h}\\).
   - Z-scores (janela 168h): \(z(r^{(1)}_t)\), \(z(\\log(Vol_t))\).
2. **R1 (indicadores clássicos; \(d=12\), parâmetros fixos):**
   - RSI: RSI(14), RSI(48), RSI(168).
   - MACD histograma: MACD(12,26,9), MACD(24,52,18).
   - ATR: ATR(14), ATR(48), ATR(168).
   - Bollinger: %b(20,2), bandwidth(20,2).
   - Estocástico: %K(14), %D(14,3).
3. **R2 (filtros causais multi-escala; \(d=12\), parâmetros fixos):**
   - EWMA de retorno \(r^{(1)}\): half-life 12h, 48h, 168h.
   - EWMA de \(|r^{(1)}|\): half-life 12h, 48h, 168h.
   - EWMA de \(\\Delta\\log(Vol)_{1h}\): half-life 12h, 48h, 168h.
   - Preço detrend (causal): \(\\log(P_t)-\\text{EMA}_{24h}(\\log P)\) e \(\\log(P_t)-\\text{EMA}_{168h}(\\log P)\).
   - Razão de vol: \(\\sigma^{(24)}_t/\\sigma^{(168)}_t\).
4. **R3 (estrutura temporal simples; \(d=12\)):**
   - Calendário: sin/cos da hora do dia; sin/cos do dia da semana (UTC).
   - Lags de retorno: \(r_{t-1}, r_{t-2}, r_{t-3}, r_{t-6}, r_{t-12}, r_{t-24}\).
   - Lags de risco: \(|r_{t-1}|\), \(|r_{t-24}|\).

### 5.4 Controles placebo por dimensão (anti-seleção; pré-fixados)
- **P0 (ruído i.i.d.; \(d=12\))**: \(x^{(j)}_t\\sim\\mathcal{N}(0,1)\\) com `random_state=0`, independente no tempo.
- **P1 (retornos embaralhados por blocos; \(d=12\))**: block-permute de \(r_t\) com \(B=168h\) e `random_state=0`, seguido do mesmo pipeline da R0 (mantém estrutura local multi-dia, destrói alinhamento causal com o retorno verdadeiro do reward).
- Placebos são **controles**, não candidatos a “otimização”: conjunto e seeds são fixos e entram no RC/SPA sem seleção do “melhor placebo”.

## 6. Modelos e baselines
- Agente principal: PPO (fixo, herdado do ARTIGO 2); replicação com segundo algoritmo é exploratória.
- Baselines: buy-and-hold, SMA e baseline supervisionada forte (mesmas features por cenário).

## 7. Protocolo de avaliação (contribuição central)
Reportar por janela, por seed, por regime, por custo e por ano; medir instabilidade como resultado (dispersão de ranks e variação entre janelas/anos).

### 7.1 Critério de conclusão (pré-fixado)
Definir \(M\\) como a métrica primária (Sharpe líquido no teste, \(c=10\\) bps). Para cada representação \(R_k\\):
- Efeito vs baseline: \(\\Delta_k = M(R_k)-M(R0)\), com IC 95% via bootstrap hierárquico (reamostrar janelas e seeds).
- Critério mínimo para alegar superioridade confirmatória: IC 95% de \(\\Delta_k\) \(>0\) **e** RC/SPA rejeita \(H_0\) a 5% no conjunto pré-definido (R0–R3 + P0–P1).
- Anti-“ganho por dimensão”: resultados de \(R_k\) devem ser materialmente melhores do que P0/P1 (placebos) sob as mesmas métricas ajustadas.
- Estabilidade como output: reportar \(\\text{IQR}(M)\\) entre seeds e a distribuição de ranks (R0–R3/P0–P1) por janela/custo, com Kendall \(\\tau\) entre rankings por cenário.

## 8. Estatística: defesa contra seleção e data-snooping
Bootstrap dependente + PSR/DSR + Reality Check/SPA sobre o conjunto de variantes (representações + placebos) consideradas antes de concluir “melhor”.

## 9. Resultados (ordem de apresentação pré-definida)
1. Distribuições por seed e sensibilidade a custos por representação.
2. Decomposição por regime e análise de decaimento ano a ano.
3. Contraste “bruto vs ajustado” + verificação com placebos.

## 10. Discussão (auditoria e implicações)
Conclusões possíveis e publicáveis: (i) “representações clássicas viram placebo sob correção formal” e/ou (ii) “um conjunto pequeno melhora robustez, não retorno médio”, sempre com trade-offs de turnover/custo e estabilidade.

## 11. Ameaças à validade (críticas antecipadas)
- Orçamento de complexidade precisa ser respeitado; qualquer expansão de graus de liberdade vira seleção.
- Não-estacionariedade: reporte por janelas/anos é obrigatório; evitar “alpha universal”.

## 12. Reprodutibilidade (entregável verificável)
- Lista fechada de representações (R0–R3), dimensionalidade \(d\), configs, seeds e scripts para reproduzir tabelas/figuras sob o protocolo do ARTIGO 2.
