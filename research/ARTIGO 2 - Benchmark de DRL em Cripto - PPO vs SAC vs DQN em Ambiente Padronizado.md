# ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado

> Proposta: um paper de **protocolo + evidência empírica** em que o objeto é **robustez** (estabilidade sob regimes, seeds e custos) e a principal ameaça é **overfitting/data-snooping**.

## Título (para submissão; provisório)
**Robustez, overfitting e instabilidade de ranking em DRL para trading horário de cripto**: um protocolo padronizado e evidência empírica para PPO, SAC e DQN em BTC/ETH (2017–2024)

## Resumo (rascunho)
Backtests de DRL em cripto frequentemente exibem ganhos frágeis: sensíveis a seeds, dependentes de regimes favoráveis e amplificados por custos subestimados. Este trabalho propõe e fixa previamente um **protocolo de benchmark** para trading horário (1h) de BTC/ETH (2017–2024) em um ambiente Gym com **custos e slippage explícitos** e ações **long/flat/short** sob restrições claras de posição. O protocolo integra (i) **walk-forward** (treino 24m/val 6m/teste 6m; step 6m); (ii) **múltiplos seeds** por algoritmo e por janela (\(S=10\)); (iii) análise por **regimes de mercado** com definição objetiva (janela \(W=720\)h; *bull/bear/sideways* por clustering no treino) e testes de **generalização cruzada entre regimes**; (iv) **sensibilidade a custos** em níveis pré-definidos (\(c\\in\\{2,10,25\\}\\) bps); e (v) correções formais para overfitting de backtest (PSR/DSR e Reality Check/SPA), reportando lado a lado métricas brutas e ajustadas. O resultado central não é “quem ganhou”, mas **quanto do “alpha” aparente sobrevive ao protocolo** e **quão instável é o ranking** sob perturbações realistas. O artefato (ambiente, pipeline de dados, splits, configs e seeds) é entregue de forma reprodutível.

## 1. Introdução (problema real: fragilidade)
Trading em cripto é um ambiente atraente para DRL por ser líquido, 24/7 e altamente não-estacionário; mas, ao mesmo tempo, é um ambiente em que **ganhos de backtest são particularmente fáceis de fabricar**: pequenas decisões experimentais (split temporal, seed, custo) podem inverter conclusões, e a própria multiplicidade de tentativas (hiperparâmetros, features, variantes de reward) pode produzir “significância” por seleção.

Este artigo trata robustez e overfitting como **outputs centrais**. Em vez de perseguir um “retorno médio” alto, o objetivo é medir **fragilidade**: sob um ambiente padronizado, até que ponto ganhos de PPO/SAC/DQN existem fora do ruído experimental, fora de regimes favoráveis e fora do conforto de custos subestimados.

## 2. Trabalhos relacionados e lacuna (posicionamento sem exagero)
Não é que “ninguém fala de overfitting”. Já há trabalho em DRL para cripto que ataca explicitamente **backtest overfitting**, formulando detecção como teste de hipótese e filtrando políticas “menos overfitted” (ex.: https://arxiv.org/abs/2209.05559). Além disso, o ferramental estatístico para **seleção/data-snooping** em backtests (PSR/DSR e Reality Check/SPA) é clássico e amplamente discutido (ex.: https://www.researchgate.net/publication/286121118_The_Deflated_Sharpe_Ratio_Correcting_for_Selection_Bias_Backtest_Overfitting_and_Non-Normality).

Também existem esforços de benchmark e competição visando padronização e reprodutibilidade (ex.: FinRL Contests, https://arxiv.org/html/2504.02281v3), inclusive com tarefas de cripto; porém, o recorte e o protocolo são diferentes (ex.: cripto em dados de livro de ordens/altíssima frequência e janelas curtas) e, tipicamente, **não** tratam “instabilidade do ranking” como tese central nem integram, no mesmo desenho, regimes + múltiplos seeds + custos/slippage + correções formais de seleção no estilo “bruto vs ajustado”.

**Lacuna (defensável e precisa):** falta um protocolo de benchmark para cripto em que a **instabilidade** seja tratada como resultado primário (não como ruído), integrando **no mesmo desenho** — no recorte 1h BTC/ETH 2017–2024 e com protocolo congelado — os seguintes componentes:
- walk-forward multi-janela (sem vazamento entre tuning e teste);
- múltiplos seeds por (algoritmo, janela) como dado de inferência;
- custos/slippage em faixas pré-fixadas e análise de sensibilidade;
- regimes com definição objetiva e testes de generalização cruzada entre regimes;
- correções formais contra seleção/data-snooping, com reporte “bruto vs ajustado”;
- ranking reportado como **variável aleatória** (probabilidade de top-1, dispersão de ranks e concordância entre cenários).

## 3. Questão de pesquisa e hipóteses (falsificáveis)
### 3.1 Questão central (núcleo; versão cortante)
Até que ponto ganhos de PPO, SAC e DQN no trading horário de BTC/ETH permanecem **estatisticamente robustos** e **estáveis** quando o protocolo força o modelo a sobreviver a perturbações realistas — **regimes**, **múltiplos seeds** e **custos/slippage** — em vez de sobreviver apenas a um split temporal conveniente?

### 3.2 Tese e hipóteses testáveis
**Tese (afirmação geral):** uma fração relevante do “alpha” aparente de PPO/SAC/DQN **não resiste** a um protocolo rigoroso que combina regimes, múltiplos seeds, custos realistas e testes formais de overfitting; e, mesmo quando algum ganho persiste, o **ranking entre algoritmos é instável** sob essas perturbações.

**H1 (robustez do ganho):** sob walk-forward multi-janela, múltiplos seeds e custo “realista”, o outperformance de DRL sobre baselines **não mantém significância** após correção formal de seleção/data-snooping (nível α=5%), em uma fração substancial dos cenários (janelas × regimes).

**H2 (instabilidade de ranking):** o ranking (por métricas primárias) de {PPO, SAC, DQN} apresenta **baixa concordância** entre cenários (janela × regime × custo), com Kendall τ tipicamente baixo e alta dispersão de ranks; nenhum algoritmo domina como “top-1” de forma consistente ao variar seed/regime/custo.

**H3 (sensibilidade a custos):** o ganho (quando existe) é **frágil a custos**, degradando materialmente ao passar de custo “baixo” para “realista/alto”, com trade-off explícito de turnover e drawdown.

## 4. Contribuições (artefato + inferência)
### 4.1 Artefato (independe do “resultado final”)
- Ambiente Gym padronizado com ações **long/flat/short**, restrições de posição e custos/slippage explícitos e documentados.
- Pipeline determinístico de dados (BTC/ETH 1h), splits walk-forward, rotulagem de regimes e configs reproduzíveis.
- Harness de avaliação com múltiplos seeds, cenários de custo e rotinas estatísticas (IC + testes de data-snooping).

### 4.2 Inferência (depende dos resultados; sempre com variância)
- Evidência empírica reportada como **distribuições por seed**, decomposição por **regime**, **sensibilidade a custos** e **métricas ajustadas** (PSR/DSR, Reality Check/SPA), não apenas “um número médio”.
- Quantificação de **instabilidade do ranking** (ranking como variável aleatória, com medidas de concordância e probabilidade de top-1).

## 5. Métodos: protocolo congelado (pré-registro interno)
Para reduzir “mineração” pós-hoc, esta seção descreve decisões experimentais fixadas **antes** de inspecionar resultados:
- Universo: **BTC e ETH**, frequência **1h**, período **2017–2024**.
- Conjunto de features **contido e fechado** (sem adicionar indicadores após ver teste).
- Ambiente com ações {short, flat, long}, posição unidimensional 1× e custos/slippage parametrizados.
- No máximo **duas** variantes de reward (fórmulas explícitas).
- PPO, SAC e DQN com **mesmo orçamento de interações** e procedimento explícito de tuning.
- Walk-forward multi-janela + múltiplos seeds; relatórios por regime e por custo.
- Critério de reporte: sempre mostrar **bruto vs ajustado** e sempre reportar incerteza (IC/intervalos) e estabilidade de ranking.

### 5.1 Dados (BTC/ETH 1h, 2017–2024)
- OHLCV em 1h; limpeza de gaps/outliers e padronização **sem vazamento temporal**.
- Normalização/standardização com estatísticas do **treino** (ou normalização online), aplicada a validação/teste sem “olhar o futuro”.
- Datas, fuso, regras de agregação e filtros documentados para garantir determinismo do pipeline.

### 5.2 Regimes de mercado (definição objetiva e sem leak)
Regimes precisam ser definidos por regra objetiva para não virarem rótulo conveniente:
- Janela de regime: \(W=720\) horas (30 dias).
- Variáveis de regime (sempre usando apenas o passado): retorno acumulado \(R^{(W)}_t=\\log(P_t/P_{t-W})\) e volatilidade realizada \(\\sigma^{(W)}_t=\\text{std}(r_{t-W+1:t})\).
- Rotulagem (pré-fixada): em cada fold externo, ajustar **k-means com \(k=3\)** em \((R^{(W)}_t, \\sigma^{(W)}_t)\) do **treino** (com padronização por estatísticas do treino e `random_state=0`). Rotular clusters por ordenação do retorno médio do cluster: maior → *bull*, menor → *bear*, intermediário → *sideways*. No teste, classificar cada \(t\) pelo centróide mais próximo (sem smoothing com futuro).
- Uso 1 (análise): decompor performance por regime dentro de cada janela de teste.
- Uso 2 (generalização): cenários “treina em um regime, testa em outro” **apenas para bull→bear e bear→bull**. Treino restrito a segmentos do regime alvo no período de treino (cada segmento vira um episódio); descartar segmentos com duração < 168h.

### 5.3 Features (contenção deliberada)
Evitar transformar benchmark em mineração de indicadores:
- Retornos log em múltiplos horizontes fixos: 1h, 4h, 24h e 168h.
- Volatilidade rolling em 24h e 168h, e medidas simples de range (ex.: log(H/L)).
- Volume (log-volume e variações) e proxies simples de liquidez, quando disponíveis.
- Calendário (hora/dia) apenas se pré-fixado e justificado.

### 5.4 Ambiente padronizado (o “mundo” do agente)
#### Instrumento e microestrutura (para não virar benchmark “discutível”)
Se há short, o artigo **fixa** a suposição do instrumento:
- Mundo-alvo: **perp linear** (ex.: BTCUSDT/ETHUSDT perp), com short simétrico e posição unidimensional com **notional 1×** (sem alavancagem “criativa”).
- Execução por barra (1h): decisão em \(t\) e aplicação em preço de referência **close-to-close** (retorno entre fechamentos), com custo/slippage proporcional ao turnover; sem mecânicas ocultas (ex.: sem reinvestimento ad-hoc, sem limites de liquidez implícitos).
- Funding/borrow: não modelado explicitamente na versão base; tratado como **limitação** (e, por prudência, incorporado indiretamente via faixas de custo).
- Margem/liquidação: não modeladas; assume-se exposição pequena o suficiente para evitar chamadas de margem (escopo de benchmark, não de execução real).

#### Ações, posição e custos (fórmula explícita)
- Ações: \(a_t \in \\{-1,0,1\\}\) (short/flat/long).
- Posição: \(p_t = a_t\), com troca de posição gerando custo.
- Custo por troca: \(\\text{cost}_t = c\\,|p_t - p_{t-1}|\), onde \(c\) agrega taxa + slippage.
- Cenários de custo (all-in, por unidade de turnover): \(c\\in\\{2,10,25\\}\\) bps (baixo/realista/alto). Mudanças -1→1 implicam \(|\\Delta p|=2\) e custam \(2c\).
  - Convenção: \(1\\) bp = \(10^{-4}\) (fração do notional), aplicado no P&L em unidades compatíveis com retorno log.
  - Justificativa: as faixas cobrem (i) taxas *taker* típicas em grandes exchanges + (ii) slippage conservador em barras de 1h; o paper deve citar explicitamente as fontes usadas para definir “realista”.

#### Recompensas (no máximo duas; fórmulas explícitas)
Seja \(r_t = \\log(P_t/P_{t-1})\).
- **Reward A (retorno log líquido de custos):**
  \[
  R_t = p_{t-1}\, r_t - \\text{cost}_t
  \]
- **Reward B (retorno + penalização de turnover):**
  \[
  R_t = p_{t-1}\, r_t - \\text{cost}_t - \\lambda\\,|p_t - p_{t-1}|
  \]
Com \(\\lambda=5\\) bps fixo (não tunado). Reward A é confirmatório (principal); Reward B é análise de sensibilidade (robustez).

## 6. Modelos: PPO, SAC, DQN e baselines
### 6.1 PPO, SAC, DQN (orçamento e tuning “justo”)
- Implementação em framework padrão com suporte consistente (incluindo SAC em ação discreta, quando aplicável).
- Mesma classe de rede: MLP (2 camadas, 64 unidades, ReLU) e **mesmo orçamento de interações** por janela para todos.
- Orçamento de treino por fold: \(T=5\\times N_{\\text{train}}\) interações (5 passagens pela janela de treino), idêntico para PPO/SAC/DQN.
- **Tuning equilibrado (procedimento explícito):** por (ativo, fold), realizar random search com \(N=20\) configurações por algoritmo. Cada configuração é treinada no treino e avaliada na validação pela **mediana** do Sharpe líquido (Reward A; custo realista \(c=10\\) bps) em 2 seeds de tuning (0 e 1). Selecionar a melhor configuração por algoritmo; então, re-treinar em treino+validação e avaliar no teste com \(S=10\) seeds **disjuntos** (2–11).
- Seeds fixos e registrados (ambiente + bibliotecas numéricas + framework de RL).

### 6.2 Baselines (para não “bater espantalho”)
Além de buy-and-hold, incluir baselines que representem oponentes reais:
- **Buy-and-hold** (por ativo).
- **SMA crossover**: grade pré-definida (fast ∈ {24, 48, 72}h; slow ∈ {168, 336, 720}h); seleção por validação sob o mesmo custo.
- **Baseline supervisionada forte**: LightGBM (classificação) para prever \(\\Pr(r_{t+1}>0)\), com regra simples (long/flat/short por limiares fixos na validação) e controle explícito de turnover, sob o mesmo custo.
  - Regra (pré-fixada): long se \(\\Pr(r_{t+1}>0)\\ge 0{,}55\); short se \(\\Pr(r_{t+1}>0)\\le 0{,}45\); caso contrário, flat.

## 7. Protocolo de avaliação (contribuição central)
Esta seção é desenhada como mecanismo de robustez e de controle de seleção; não é “detalhe metodológico”.

### 7.1 Splits temporais e walk-forward
- Walk-forward com janelas fixas: treino = 24 meses, validação = 6 meses, teste = 6 meses; avanço (step) = 6 meses ao longo de 2017–2024.
- Reporte por janela e agregação com incerteza, sem misturar tuning com teste.

### 7.2 Múltiplos seeds por janela
- Para cada (algoritmo, janela), rodar \(S=10\) seeds (2–11), fixos e compartilhados entre algoritmos.
- Reportar distribuição (mediana, quantis, IC) e não apenas média.

### 7.3 Regimes e generalização cruzada
- Reportar performance **por regime** no teste.
- Rodar cenários “treina em um regime, testa em outro” com regras pré-definidas de seleção de segmentos e tamanho mínimo (para evitar cherry-picking).

### 7.4 Sensibilidade a custos (poucos níveis, pré-fixados)
- Rodar os 3 cenários \(c\\in\\{2,10,25\\}\\) bps (baixo/realista/alto).
- Mostrar quando o ganho desaparece ao elevar custo.

### 7.5 Métricas e ranking como variável aleatória
- Métricas brutas: retorno, Sharpe/Sortino, MDD/Calmar, turnover, tempo em posição/exposição.
- Métricas de robustez: IC via bootstrap (preferência por block/stationary bootstrap), PSR/DSR e Reality Check/SPA (ou equivalente).
- **Estabilidade de ranking** como métrica primária: Kendall τ entre rankings por (janela, regime, custo), distribuição de ranks por seed e taxa de “mesmo vencedor”.

### 7.6 Controle de complexidade (anti data-snooping disfarçado)
- Limitar combinações experimentais (ativos, rewards, custos, features) para que o desenho não vire mineração por multiplicidade.
- Separar explicitamente o que é **confirmatório** (pré-registrado) do que é **exploratório** (marcado como tal).

## 8. Estatística: defesas formais contra seleção e overfitting
O contraste “bruto vs ajustado” é parte do resultado:
- IC por bootstrap com dependência temporal (block/stationary bootstrap; comprimento esperado do bloco = 24h; 2.000 reamostragens; IC 95%).
- Probabilistic Sharpe Ratio (PSR) e Deflated Sharpe Ratio (DSR) para ajustar por não-normalidade e multiplicidade (número de tentativas conservador por ativo/fold: \(3\\times 20\\times 2=120\) candidatos de DRL, mais variantes de baseline quando aplicável).
- Reality Check/SPA (ou equivalente) aplicado ao conjunto de estratégias candidatas (todas as configs avaliadas na validação + o(s) “vencedor(es)” por algoritmo), usando buy-and-hold como benchmark (e, como verificação, o melhor baseline não-RL).
- Reportar métricas ajustadas lado a lado com as brutas, com interpretação cautelosa e foco em replicabilidade entre janelas/regimes/custos.

## 9. Resultados (ordem de apresentação pré-definida)
1. **Distribuições por seed** (por janela) e sensibilidade a custos.
2. Decomposição por **regime** + generalização cruzada entre regimes.
3. Apenas no fim: agregados globais (com incerteza) e ranking “médio” acompanhado da sua instabilidade.

## 10. Discussão (implicações; robustez como conclusão)
- Se o resultado principal for “não passa no rigor”, isso é evidência empírica de **fragilidade**: o foco da discussão é identificar *onde* o ganho colapsa (custos, regimes, seeds) e como isso muda a interpretação de “alpha”.
- Se algum ganho persistir, a discussão deve ser condicionada: **quais regimes**, **sob quais custos** e com **qual estabilidade entre seeds**; evitar narrativa de dominância “universal”.
- A instabilidade do ranking (e sua quantificação) vira resultado: quando PPO/SAC/DQN alternam posições sob perturbações pequenas, o “vencedor” é parte do ruído experimental e não um fato fixo.

## 11. Ameaças à validade (críticas antecipadas)
- **Short e instrumento**: spot vs perp vs margem; funding/borrow e risco operacional. Fixar suposições e reportar limitação/sensibilidade.
- **Custos**: “realista” precisa ser definido e documentado (faixas e fontes/justificativas, não afirmações vagas).
- **Tuning desigual**: procedimento explícito e orçamento equilibrado; publicação de configs e seeds.
- **Features e leak**: toda feature e regime definido sem futuro; normalização sem vazamento.
- **Não-estacionariedade**: resultados condicionados a regimes e janelas; evitar narrativa de “alpha universal”.

## 12. Reprodutibilidade (entregável verificável)
O paper termina com materiais que permitem reproduzir os gráficos:
- Código do ambiente + pipeline de dados + splits/regimes.
- Arquivos de configuração (hiperparâmetros) e lista de seeds.
- Versões fixadas (dependências), scripts de execução e logs.
- Checklist de reprodutibilidade: “rodar e reproduzir Tabelas/Figuras X–Y” sem intervenção manual.
