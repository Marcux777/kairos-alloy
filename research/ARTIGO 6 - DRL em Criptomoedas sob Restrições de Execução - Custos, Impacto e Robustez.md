# ARTIGO 6 - DRL em Criptomoedas sob Restrições de Execução - Custos, Impacto e Robustez

> Proposta: um paper de **robustez por viabilidade**: mostrar como a política muda quando custos/impacto/turnover/risco são modelados de forma coerente e como isso afeta estabilidade, não só média.

## Título (para submissão; provisório)
**DRL sob restrições de execução em cripto**: custo, impacto e estabilidade sob protocolo congelado (BTC/ETH 1h, 2017–2024)

## Resumo (rascunho)
Backtests em cripto frequentemente assumem execução “barata” e ilimitada, permitindo políticas com turnover alto que colapsam com fricções realistas. Este paper trata trading como decisão sob restrições e pergunta se políticas “viáveis por construção” (camada explícita de restrições/otimização e/ou treino robusto a fricções) são mais estáveis do que RL “puro”. Usando o ambiente e protocolo rígidos do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]] (walk-forward, múltiplos seeds, regimes e custos em faixas, com correção formal de seleção), comparamos RL direto long/flat/short versus (i) RL + projeção sob limites de turnover/risco e (ii) treino robusto sob distribuição de custos/impacto. O resultado central é a robustez: degradação ao elevar fricções, variância entre seeds/janelas e pior-caso, com ranking tratado como variável aleatória e métricas “bruto vs ajustado”.

## 1. Introdução (problema real: execução irrealista fabrica alpha)
Uma política que troca demais pode parecer ótima em custo baixo e colapsar em custo realista; isso não é detalhe, é o fenômeno. Fechar as brechas de execução (turnover, impacto e limites) transforma “retorno” em hipótese testável sob viabilidade.

## 2. Trabalhos relacionados e lacuna (posicionamento sem exagero)
Há trabalho sobre custos e controle de risco, e há literatura de RL robusto. A lacuna aqui é o desenho confirmatório: no recorte cripto 1h BTC/ETH 2017–2024, falta um benchmark que trate **restrições e impacto** como parte do ambiente (não pós-processamento), compare RL puro vs políticas viáveis por construção e reporte estabilidade/robustez como resultado primário sob correção formal de seleção.

## 3. Questão de pesquisa e hipóteses (falsificáveis)
### 3.1 Questão central
Sob o protocolo do ARTIGO 2, políticas com restrições explícitas e/ou treino robusto a fricções exibem maior estabilidade (seeds/regimes/custos/impacto) e melhor pior-caso do que RL “puro”?

### 3.2 Hipóteses testáveis
**H1 (mudança de política):** ao impor restrições (turnover máximo, impacto convexo, orçamento de risco), a política ótima muda (menos troca) e o “alpha” de estratégias instáveis cai.

**H2 (robustez):** políticas viáveis por construção degradam menos ao aumentar custo/impacto e exibem menor variância entre seeds/janelas (melhor pior-caso).

**H3 (ranking instável):** o ranking entre métodos muda materialmente ao variar fricções/limites; essa instabilidade deve ser reportada como resultado.

## 4. Contribuições (artefato + inferência)
### 4.1 Artefato
- Extensão do ambiente do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]] com restrições explícitas (turnover/risco) e modelo de impacto/custo documentado como cenários.
- Rotinas de avaliação que tratam ranking como variável aleatória e reportam robustez por seed/regime/custo.

### 4.2 Inferência
- Evidência empírica sobre estabilidade e pior-caso versus RL puro e heurísticas, com “bruto vs ajustado” (PSR/DSR + RC/SPA).

## 5. Métodos: protocolo congelado (pré-registro interno)
### 5.1 Ambiente e avaliação (herdado)
Reusar universo, splits walk-forward, regimes, seeds e cenários de custo do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]].

### 5.2 Fricções/impacto e restrições (tratamentos)
Conjunto fechado de cenários (confirmatório):
- impacto convexo por turnover (ex.: \(\\propto |\\Delta p_t|^2\)) em faixas;
- limite de turnover por janela;
- orçamento de risco (ex.: restrição de exposição/vol ex-ante).

## 6. Métodos comparados (tratamentos)
- **T1 (RL puro):** política direta long/flat/short como no ARTIGO 2.
- **T2 (RL + projeção):** agente gera sinal \(s_t\); camada determinística projeta para \(p_t\) respeitando limites (turnover/risco/impacto).
- **T3 (treino robusto):** domain randomization sobre custos/impacto por episódio, mantendo avaliação separada por cenário.

## 7. Protocolo de avaliação (contribuição central)
- Reporte por janela e por seed; decomposição por regimes.
- Sensibilidade explícita a custos/impacto/limites; ranking como variável aleatória (prob(top-1), dispersão, Kendall τ).

## 8. Estatística: defesa contra seleção e data-snooping
Bootstrap dependente + PSR/DSR + Reality Check/SPA sobre o conjunto de variantes (tratamentos × tuning) consideradas antes de concluir “melhor método”.

## 9. Resultados (ordem de apresentação pré-definida)
1. Degradação ao elevar fricções e estabilidade por seed.
2. Robustez por regime e pior-caso (quantis/CVaR).
3. Contraste “bruto vs ajustado” e instabilidade do ranking.

## 10. Discussão (viabilidade como critério de verdade)
Se o “alpha” cair com execução realista, discutir como isso redefine o que é “melhor política” e por que estabilidade/pior-caso são outputs mais informativos do que retorno médio.

## 11. Ameaças à validade (críticas antecipadas)
- “Impacto inventado”: calibrar como cenários (faixas), documentar forma funcional e mostrar sensibilidade.
- “Restrições ad-hoc”: justificar limites e evitar ajuste pós-hoc; declarar confirmatório vs exploratório.

## 12. Reprodutibilidade (entregável verificável)
- Configs, seeds, parâmetros de impacto/limites e scripts para reproduzir tabelas/figuras sob o protocolo do ARTIGO 2.
