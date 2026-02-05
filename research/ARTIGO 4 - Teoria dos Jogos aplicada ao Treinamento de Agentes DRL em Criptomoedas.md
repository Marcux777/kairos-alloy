# ARTIGO 4 - Teoria dos Jogos aplicada ao Treinamento de Agentes DRL em Criptomoedas

> Proposta: um paper empírico/metodológico em que o objetivo não é “ganho mágico”, mas **estabilidade/robustez**: treinar políticas menos frágeis via mecanismos inspirados em teoria dos jogos.

## Título (para submissão; provisório)
**Treinamento adversarial e payoff max-min para DRL em cripto**: estabilidade sob regimes, seeds e custos em BTC/ETH 1h (2017–2024)

## Resumo (rascunho)
Políticas de DRL em trading podem ser instáveis: variam por seed, mudam sob perturbações pequenas e frequentemente dependem de custos implícitos otimistas. Este trabalho avalia mecanismos inspirados em teoria dos jogos — self-play adversarial e payoff robusto (max-min) — no ambiente e protocolo rígidos do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]]. A proposta é treinar um agente trader contra um adversário que atua apenas sobre variáveis **modeladas** (ex.: custo/slippage/impacto dentro de faixas e ruído de execução), evitando alegar “reação do mercado” sem dados. O resultado central é se essas técnicas reduzem variância entre seeds, aumentam robustez por regime e melhoram pior-caso (ex.: quantis/CVaR) sem inflar seleção oportunista; métricas brutas e ajustadas (PSR/DSR + Reality Check/SPA) são reportadas lado a lado.

## 1. Introdução (problema real: políticas frágeis)
Em backtests, políticas instáveis podem explorar artefatos do protocolo (custos subestimados, tuning desigual, coincidências do split). Um desenho robusto precisa “forçar” o agente a sobreviver a perturbações plausíveis e medir estabilidade como output.

## 2. Trabalhos relacionados e lacuna (posicionamento sem exagero)
Há uma literatura ampla em robust RL, treinamento adversarial e jogos; há também benchmarks financeiros. A lacuna aqui é a integração: no recorte cripto 1h BTC/ETH 2017–2024, falta um protocolo que avalie explicitamente se mecanismos game-theoretic geram **políticas menos sensíveis** a seeds/regimes/custos, com correção formal de seleção e ranking tratado como variável aleatória.

## 3. Questão de pesquisa e hipóteses (falsificáveis)
### 3.1 Questão central
Sob o protocolo do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]], mecanismos adversariais/game-theoretic reduzem fragilidade e melhoram robustez (não só média) versus DRL padrão?

### 3.2 Hipóteses testáveis
**H1 (estabilidade):** self-play/adversarial reduz variância entre seeds e aumenta consistência entre janelas/regimes (menor dispersão e maior concordância de ranking).

**H2 (robustez a fricções):** políticas treinadas com payoff robusto degradam menos quando custos/slippage/impacto aumentam (melhor pior-caso).

**H3 (trade-off):** ganhos médios podem não aumentar; o benefício principal (se existir) é estabilidade e redução de fragilidade, detectável sob métricas ajustadas.

## 4. Contribuições (artefato + inferência)
### 4.1 Artefato
- Extensão do ambiente/harness do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]] com um adversário restrito a fricções de execução (custos/slippage/impacto) em faixas documentadas.
- Procedimento reprodutível de treinamento (self-play/payoff max-min) com budget e tuning equilibrados.

### 4.2 Inferência
- Evidência empírica de estabilidade/robustez (distribuições por seed, por regime e por custo) e contraste “bruto vs ajustado” (PSR/DSR + RC/SPA).

## 5. Métodos: protocolo congelado (pré-registro interno)
### 5.1 Ambiente e avaliação (herdado)
Usar o mesmo universo, splits walk-forward, regimes, seeds e cenários de custo do [[ARTIGO 2 - Benchmark de DRL em Cripto - PPO vs SAC vs DQN em Ambiente Padronizado]].

### 5.2 Tratamentos (mecanismos game-theoretic)
- **T1 (baseline):** DRL padrão (reward financeiro do ARTIGO 2).
- **T2 (domain randomization):** treino com custos/impactos amostrados por episódio dentro de faixas (robustez a execução).
- **T3 (self-play/max-min):** adversário escolhe parâmetros de fricção por episódio sob orçamento; trader maximiza payoff max-min (pior-caso dentro da faixa).

### 5.3 Restrições do adversário (para evitar “benchmark discutível”)
O adversário não altera preços históricos; ele atua apenas nas fricções explicitamente modeladas (custos/slippage/impacto) e em ruído de execução dentro de limites documentados.

## 6. Modelos e baselines
- Algoritmo base: PPO (ou outro fixado para comparabilidade com ARTIGO 2), com mesmo budget e tuning equilibrado.
- Baselines: buy-and-hold, SMA e baseline supervisionada forte (quando aplicável) sob os mesmos custos.

## 7. Protocolo de avaliação (contribuição central)
- Reportar resultados por janela e por seed; decompor por regime.
- Sensibilidade explícita a custos/impacto; ranking como variável aleatória (prob(top-1), dispersão, Kendall τ).

## 8. Estatística: defesa contra seleção e data-snooping
- IC via bootstrap dependente; PSR/DSR e Reality Check/SPA para controlar seleção (configs/seeds) ao reportar “melhor método”.

## 9. Resultados (ordem de apresentação pré-definida)
1. Estabilidade por seed e sensibilidade a custos/impacto.
2. Robustez por regime e pior-caso (quantis/CVaR).
3. Contraste “bruto vs ajustado” e instabilidade de ranking entre tratamentos.

## 10. Discussão (o que significa “robustez” aqui)
Quando o mecanismo ajuda, discutir *por que* (redução de turnover? mitigação de exploração de artefatos?) e em quais regimes/custos; quando não ajuda, discutir limites do modelo de fricção/adversário.

## 11. Ameaças à validade (críticas antecipadas)
- “Adversário artificial”: calibrar faixas e reportar sensibilidade; evitar alegar reação do mercado sem dados.
- Custos/impacto como cenário (faixa), não “verdade”; documentar escolhas e fontes.

## 12. Reprodutibilidade (entregável verificável)
- Código do adversário, configs, seeds, logs e scripts para reproduzir tabelas/figuras sob o protocolo do ARTIGO 2.
