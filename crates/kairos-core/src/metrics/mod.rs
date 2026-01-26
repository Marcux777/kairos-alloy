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

#[derive(Debug, Default)]
pub struct MetricsState {
    equity_curve: Vec<EquityPoint>,
    trades: Vec<Trade>,
    peak_equity: f64,
    max_drawdown: f64,
}

impl MetricsState {
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

        MetricsSummary {
            bars_processed: self.equity_curve.len(),
            trades,
            win_rate: 0.0,
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
                returns.push(curr / prev - 1.0);
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
            mean / std * (returns.len() as f64).sqrt()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MetricsState;
    use crate::types::EquityPoint;

    #[test]
    fn computes_net_profit_and_drawdown() {
        let mut metrics = MetricsState::default();
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
