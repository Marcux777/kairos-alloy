use crate::types::{EquityPoint, Trade};

#[derive(Debug, Default)]
pub struct MetricsSummary {
    pub bars_processed: usize,
    pub trades: usize,
    pub win_rate: f64,
    pub net_profit: f64,
    pub sharpe: f64,
    pub max_drawdown: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct MetricsConfig {
    pub risk_free_rate: f64,
    pub annualization_factor: Option<f64>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            risk_free_rate: 0.0,
            annualization_factor: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct MetricsState {
    equity_curve: Vec<EquityPoint>,
    trades: Vec<Trade>,
    peak_equity: f64,
    max_drawdown: f64,
    config: MetricsConfig,
}

impl MetricsState {
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            equity_curve: Vec::new(),
            trades: Vec::new(),
            peak_equity: 0.0,
            max_drawdown: 0.0,
            config,
        }
    }

    pub fn record_equity(&mut self, point: EquityPoint) {
        if self.peak_equity == 0.0 || point.equity > self.peak_equity {
            self.peak_equity = point.equity;
        } else if self.peak_equity > 0.0 {
            let drawdown = (self.peak_equity - point.equity) / self.peak_equity;
            if drawdown > self.max_drawdown {
                self.max_drawdown = drawdown;
            }
        }
        self.equity_curve.push(point);
    }

    pub fn record_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
    }

    pub fn summary(&self) -> MetricsSummary {
        let trades = self.trades.len();
        let net_profit = self.net_profit();
        let sharpe = self.sharpe_ratio();
        let win_rate = self.win_rate();

        MetricsSummary {
            bars_processed: self.equity_curve.len(),
            trades,
            win_rate,
            net_profit,
            sharpe,
            max_drawdown: self.max_drawdown,
        }
    }

    pub fn equity_curve(&self) -> &[EquityPoint] {
        &self.equity_curve
    }

    pub fn trades(&self) -> &[Trade] {
        &self.trades
    }

    pub fn max_drawdown(&self) -> f64 {
        self.max_drawdown
    }

    pub fn into_parts(self) -> (Vec<EquityPoint>, Vec<Trade>, MetricsSummary) {
        let summary = self.summary();
        (self.equity_curve, self.trades, summary)
    }

    fn net_profit(&self) -> f64 {
        if self.equity_curve.is_empty() {
            return 0.0;
        }
        let first = self.equity_curve.first().unwrap().equity;
        let last = self.equity_curve.last().unwrap().equity;
        last - first
    }

    fn sharpe_ratio(&self) -> f64 {
        if self.equity_curve.len() < 2 {
            return 0.0;
        }

        let mut returns = Vec::with_capacity(self.equity_curve.len() - 1);
        for pair in self.equity_curve.windows(2) {
            let prev = pair[0].equity;
            let curr = pair[1].equity;
            if prev > 0.0 {
                let ret = curr / prev - 1.0;
                returns.push(ret - self.config.risk_free_rate);
            }
        }

        if returns.len() < 2 {
            return 0.0;
        }

        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let var = returns
            .iter()
            .map(|ret| {
                let diff = ret - mean;
                diff * diff
            })
            .sum::<f64>()
            / (returns.len() as f64 - 1.0);

        let std = var.sqrt();
        if std == 0.0 {
            0.0
        } else {
            let scale = self
                .config
                .annualization_factor
                .unwrap_or(returns.len() as f64);
            mean / std * scale.sqrt()
        }
    }

    fn win_rate(&self) -> f64 {
        if self.equity_curve.len() < 2 {
            return 0.0;
        }
        let mut wins = 0usize;
        let mut total = 0usize;
        for pair in self.equity_curve.windows(2) {
            let prev = pair[0].equity;
            let curr = pair[1].equity;
            if prev <= 0.0 {
                continue;
            }
            total += 1;
            if curr > prev {
                wins += 1;
            }
        }
        if total == 0 {
            0.0
        } else {
            wins as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MetricsConfig, MetricsState};
    use crate::types::EquityPoint;

    #[test]
    fn computes_net_profit_and_drawdown() {
        let mut metrics = MetricsState::new(MetricsConfig::default());
        metrics.record_equity(EquityPoint {
            timestamp: 1,
            equity: 100.0,
            cash: 100.0,
            position_qty: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
        });
        metrics.record_equity(EquityPoint {
            timestamp: 2,
            equity: 80.0,
            cash: 80.0,
            position_qty: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
        });
        metrics.record_equity(EquityPoint {
            timestamp: 3,
            equity: 120.0,
            cash: 120.0,
            position_qty: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
        });

        let summary = metrics.summary();
        assert_eq!(summary.net_profit, 20.0);
        assert!(summary.max_drawdown > 0.0);
    }
}
