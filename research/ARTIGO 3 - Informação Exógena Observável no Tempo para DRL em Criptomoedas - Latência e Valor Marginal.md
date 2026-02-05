# ARTIGO 3 - Informação Exógena Observável no Tempo para DRL em Criptomoedas - Latência e Valor Marginal

> Proposta: um paper de **inferência sob restrições operacionais** em que o objeto científico é o **timestamp** (observabilidade/latência/missingness) e o resultado é o **valor marginal** de informação exógena além de um estado puramente de mercado.

## Título (para submissão; provisório)
**Informação exógena observável no tempo em DRL para trading horário de cripto**: valor marginal sob latência, regimes e fricções (BTC/ETH 1h, 2017–2024)

## Resumo (rascunho)
Ganhos de “alt data” em cripto frequentemente dependem de alinhamento temporal frouxo: timestamps imperfeitos, backfills/retcons e latência implícita podem virar look-ahead silencioso. Este trabalho testa, sob o protocolo rígido do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]], se existe ganho marginal robusto ao adicionar informação exógena (on-chain + sentimento/notícias) a um estado fixo de mercado, **impondo** regras conservadoras de observabilidade temporal (timestamp bruto → timestamp observável) e cenários de latência. O desenho inclui controles negativos (placebos temporais) como teste de sanidade, reporta distribuições por seed e sensibilidade a custos antes de qualquer agregado, e aplica correções formais contra seleção (PSR/DSR + Reality Check/SPA). O resultado central é operacional: **quanto (se algo) a informação exógena acrescenta quando ela é observável no tempo certo**.

## 1. Introdução (problema real: disciplina do timestamp)
Se uma fonte exógena é “boa” apenas quando você assume disponibilidade instantânea, ela não é uma hipótese científica; é um artefato de protocolo. Este paper transforma o problema em teste: sob alinhamento causal conservador, qual é o valor marginal de informação exógena — e em quais regimes/fricções esse valor existe (ou colapsa)?

> Nota de escopo: este paper não avalia “engenharia de preço” (catálogo de indicadores/transformadas). Representações endógenas do preço sob orçamento de complexidade são tratadas separadamente em [[ARTIGO 5 - Representação do Preço para DRL em Criptomoedas - Orçamento de Complexidade e Auditoria Anti-Seleção]].

## 2. Lacuna (precisa e defensável)
Existe literatura aplicando sentimento/on-chain em cripto; o que falta, no recorte 1h BTC/ETH 2017–2024, é um desenho confirmatório em que **observabilidade temporal** (timestamp, latência, revisões e missingness) é parte explícita do método e em que o ganho é reportado como **efeito incremental** com estabilidade por seed/regime/custo e correção formal de seleção.

## 3. Questão de pesquisa e hipóteses (falsificáveis)
### 3.1 Questão central
Sob alinhamento causal conservador e latência explícita, qual é o valor marginal de fontes exógenas além de um estado fixo de mercado em DRL para trading 1h de BTC/ETH?

### 3.2 Hipóteses testáveis
**H1 (valor marginal sob latência):** sob latência realista/conservadora, a inclusão de informação exógena não mantém ganho significativo (α=5%) após correção formal de seleção/data-snooping.

**H2 (ganho condicional a regime/fricção):** se houver ganho, ele se concentra em regimes de crise/alta volatilidade e degrada materialmente ao aumentar custos/slippage.

**H3 (custo da complexidade):** mesmo quando há ganho bruto marginal, a informação exógena aumenta instabilidade (variância entre seeds) sob o mesmo orçamento de treino.

## 4. Contribuições (artefato + inferência)
### 4.1 Artefato
- Pipeline reprodutível com **modelo explícito de observabilidade temporal** (timestamp bruto → observável), latência e tratamento determinístico de missingness/revisões.
- Controles negativos (placebos temporais) como auditoria de vazamento.

### 4.2 Inferência
- Efeito incremental (Δ) de “mercado” → “mercado + exógeno” com distribuição por seed, decomposição por regime e contraste “bruto vs ajustado” (PSR/DSR + RC/SPA).

## 5. Métodos: protocolo congelado (pré-registro interno)
### 5.1 O que é inviolável (herdado do ARTIGO 2)
Tudo abaixo é **fixo** e compartilhado com o benchmark:
- Dados base (BTC/ETH 1h, 2017–2024), splits walk-forward, regimes, seeds, custos/slippage e reward.
- Orçamento de treino e tuning (mesmo procedimento, mesmo budget, mesmas seeds de tuning).
- Métricas e estatística (IC por bootstrap dependente + PSR/DSR + Reality Check/SPA).

**Única variável experimental:** o estado observado pelo agente (com ou sem informação exógena) e o modelo de observabilidade/latência dessa informação.

**Resumo mínimo do protocolo (para autocontenção):**
- Splits walk-forward: treino 24m / validação 6m / teste 6m, step 6m (2017–2024).
- Ações/posição: \(a_t \\in\\{-1,0,1\\}\) (short/flat/long), posição unidimensional 1× (perp linear como mundo-alvo), execução close-to-close.
- Reward principal: \(R_t = p_{t-1} r_t - c|p_t-p_{t-1}|\), com \(c\\in\\{2,10,25\\}\\) bps.
- Seeds: tuning (0,1) e avaliação no teste \(S=10\) seeds (2–11), fixos e compartilhados.
- Regimes (objetivo): \(W=720h\), k-means \(k=3\) em (retorno \(R^{(W)}\), volatilidade \(\\sigma^{(W)}\)) ajustado no treino; rótulos bull/bear/sideways por ordenação de retorno; teste classificado por centróide mais próximo.

### 5.2 Estados endógenos (controles; pré-fixados)
Para evitar “bater espantalho”, usar dois controles endógenos:
- **B (básico):** retornos/risco/volume (conjunto contido do ARTIGO 2).
- **T (técnico contido):** B + um conjunto fixo e pequeno de indicadores (ex.: RSI, MACD, ATR), sem grade/tuning de indicadores (baseline forte, não objeto do paper).

### 5.3 Tratamento exógeno (X): disciplina de timestamp
**Fontes exógenas (v1; conjunto fechado e auditável):**
- **On-chain (métricas agregadas, BTC e ETH):** `active_addresses`, `exchange_netflow` (inflow−outflow) e `transfer_volume`.
- **Notícias (event-level → agregado 1h):** contagem de headlines por hora (`news_count`) e sentimento médio por hora (`news_sent_mean`) a partir de um classificador fixo (ex.: FinBERT), com timestamps por headline.
  - Implementação (v1, fechada; para auditabilidade): on-chain via **Glassnode Studio**; notícias via **GDELT 2.1** (event/headline-level) filtrado por BTC/ETH (palavras-chave fixas), com sentimento por headline via FinBERT.

**Contrato temporal por fonte (pré-fixado):**
- On-chain: \(t_{\\text{raw}}\) é o timestamp do ponto no provedor; \(t_{\\text{seen}}\) é quando o pipeline baixou/registrou o ponto (snapshots versionados). Para impedir backfill/retcon, usar **regra first-seen**: \(t_{\\text{raw}}\\leftarrow t_{\\text{seen}}\) e congelar o valor observado na primeira aparição.
- Notícias: \(t_{\\text{raw}}\) é o timestamp de publicação do headline; \(t_{\\text{seen}}\) é o timestamp de ingestão do feed (logado). Para ser conservador, usar \(t_{\\text{raw}}\\leftarrow t_{\\text{seen}}\) (quando disponível); caso contrário, usar \(t_{\\text{raw}}\\leftarrow t_{\\text{pub}}\) e reportar explicitamente a limitação.

**Modelo de observabilidade (pré-fixado):**
- Cada observação tem um timestamp bruto \(t_{\\text{raw}}\) (publicação/extração).
- Impor uma latência mínima \(\\delta\) e projetar para o grid 1h: \(t_{\\text{obs}} = \\lceil t_{\\text{raw}} + \\delta \\rceil_{1h}\).
- A feature só pode entrar no estado da barra \(t\) se \(t_{\\text{obs}} \\le t\).

**Missingness e revisões (pré-fixado):**
- On-chain: forward-fill **limitado** (máximo 24h); após isso, valor = 0 e `mask_onchain=0`. Quando preenchido, `mask_onchain=1`.
- Notícias: ausência de headlines em uma hora é um valor válido (`news_count=0`); não imputar “sentimento” a partir de horas futuras. `news_sent_mean` é 0 quando `news_count=0` e a presença de sinal é carregada pelo próprio `news_count`.
- Revisões/retcons: snapshots versionados + regra “first-seen” (não permitir backfill entrar no passado).

### 5.4 Cenários de latência (pré-fixados)
- **L0 (idealizado):** \(\\delta=0\) (sanidade; não é conclusão operacional).
- **L1 (realista):** \(\\delta=1h\).
- **L2 (conservador):** \(\\delta=6h\).

### 5.5 Controles negativos (sanidade; pré-fixados)
- **Shift positivo** (apenas atraso, sem look-ahead): gerar duas versões placebo com atrasos fixos \(\\Delta\\in\\{37h,241h\\}\\) aplicados apenas ao bloco exógeno \(X\) (criando missingness no início). Intuição: \(\\Delta\\) não múltiplo de 24h (quebra sazonalidade intradiária) e \(\\Delta\\) grande (excede horizonte causal plausível).
- **Block shuffle** (preserva distribuição e dependência local, destrói alinhamento causal): permutar blocos de tamanho \(B=168h\) (1 semana) de \(X\) com `random_state=0`. Intuição: preserva estrutura local multi-dia, mas remove o alinhamento temporal específico que sustentaria “informação no tempo certo”.
- Todos os placebos são **pré-fixados** (sem escolher o “melhor placebo”) e entram no conjunto de candidatos do RC/SPA.
- Lógica decisória (anti-“ganho por dimensão”): se \(T+X\) melhora mas \(T+X_{\\text{placebo}}\) melhora de forma comparável, a leitura é **graus de liberdade/seleção**, não “informação observável no tempo”.

## 6. Modelos e baselines
- Agente principal: PPO (fixo, herdado do ARTIGO 2).
- Baselines: buy-and-hold, SMA e baseline supervisionada forte, para medir se o ganho marginal é específico do DRL.

## 7. Protocolo de avaliação (contribuição central)
Comparações confirmatórias:
- **B vs T** (baseline forte) e **T vs T+X** sob L0/L1/L2.
- Reportar distribuição por seed e sensibilidade a custos por janela antes de agregados; decompor por regime.

### 7.1 Critério de conclusão (pré-fixado)
Definir \(M\\) como a métrica primária (Sharpe líquido no teste, \(c=10\\) bps). Para cada latência \(L\\in\\{L1,L2\\}\\):
- Efeito real: \(\\Delta_{\\text{real}}=M(T+X,L)-M(T)\).
- Efeito placebo: \(\\Delta_{\\text{pl}}=M(T+X_{\\text{placebo}},L)-M(T)\).
- Critério mínimo para alegar “valor marginal de informação observável”: o IC 95% (bootstrap hierárquico: reamostrar janelas e seeds) de \(\\Delta_{\\text{real}}-\\Delta_{\\text{pl}}\\) deve ser \(>0\) e o Reality Check/SPA (sobre o conjunto pré-definido) deve rejeitar a hipótese nula ao nível de 5%.
- Estabilidade como output: reportar \(\\text{IQR}(M)\\) entre seeds e a distribuição de ranks (B/T/T+X) por janela/custo, com Kendall \(\\tau\) entre rankings por cenário.

## 8. Estatística: defesa contra seleção e data-snooping
- IC via bootstrap dependente.
- PSR/DSR e Reality Check/SPA aplicados ao conjunto de variantes consideradas (B, T, T+X em L0/L1/L2 e placebos).

## 9. Resultados (ordem de apresentação pré-definida)
1. Distribuições por seed e sensibilidade a custos (B vs T vs T+X sob latências).
2. Efeito por regime (crise vs não-crise) e generalização entre janelas.
3. Contraste “bruto vs ajustado” + verificação com placebos.

## 10. Discussão (interpretação operacional)
Dois resultados são igualmente publicáveis: (i) “não há ganho incremental sob latência conservadora” (latência/ruído destrói utilidade prática) e (ii) “há ganho condicional” (regime-específico e frágil a fricções). A interpretação é sempre condicionada a observabilidade e custos.

## 11. Ameaças à validade (críticas antecipadas)
- Definição de \(t_{\\text{raw}}\) (publicação vs agregação) e erros de timestamp.
- Revisões/retcons e backfills; versão de dataset e regra “first-seen”.
- Missingness e risco de imputação virar sinal; necessidade de regra determinística + máscara.

## 12. Reprodutibilidade (entregável verificável)
- Especificação completa do modelo de observabilidade/latência por fonte, configs, seeds e scripts para reproduzir tabelas/figuras sob o protocolo do ARTIGO 2.
