use crate::market_data::MarketDataSource;
use crate::metrics::{MetricsState, MetricsSummary};
use crate::portfolio::Portfolio;
use crate::risk::RiskLimits;
use crate::strategy::Strategy;
use crate::types::{ActionType, EquityPoint, Order, Side, Trade};

#[derive(Debug)]
pub struct BacktestRunner<S, D>
where
    S: Strategy,
    D: MarketDataSource,
{
    strategy: S,
    data: D,
    portfolio: Portfolio,
    risk_limits: RiskLimits,
    metrics: MetricsState,
    pending_order: Option<Order>,
    next_order_id: u64,
    fee_bps: f64,
    slippage_bps: f64,
    symbol: String,
    halt_trading: bool,
}

pub struct BacktestResults {
    pub summary: MetricsSummary,
    pub trades: Vec<Trade>,
    pub equity: Vec<EquityPoint>,
}

impl<S, D> BacktestRunner<S, D>
where
    S: Strategy,
    D: MarketDataSource,
{
    pub fn new(
        strategy: S,
        data: D,
        risk_limits: RiskLimits,
        initial_capital: f64,
        fee_bps: f64,
        slippage_bps: f64,
        symbol: String,
    ) -> Self {
        Self {
            strategy,
            data,
            portfolio: Portfolio::new_with_cash(initial_capital),
            risk_limits,
            metrics: MetricsState::default(),
            pending_order: None,
            next_order_id: 1,
            fee_bps,
            slippage_bps,
            symbol,
            halt_trading: false,
        }
    }

    pub fn run(&mut self) -> BacktestResults {
        while let Some(bar) = self.data.next_bar() {
            self.execute_pending_order(&bar);

            if !self.halt_trading {
                let action = self.strategy.on_bar(&bar, &self.portfolio);
                self.schedule_order(&bar, action);
            }

            self.record_equity(&bar);

            // Placeholder: extend with full risk/metrics/reporting.
        }

        let (equity, trades, summary) = std::mem::take(&mut self.metrics).into_parts();
        BacktestResults {
            summary,
            trades,
            equity,
        }
    }

    fn execute_pending_order(&mut self, bar: &crate::types::Bar) {
        let order = match self.pending_order.take() {
            Some(order) => order,
            None => return,
        };

        let mut price = bar.open;
        let slippage_cost = price * order.quantity * (self.slippage_bps / 10_000.0);
        match order.side {
            Side::Buy => {
                price *= 1.0 + self.slippage_bps / 10_000.0;
            }
            Side::Sell => {
                price *= 1.0 - self.slippage_bps / 10_000.0;
            }
        }

        let fee = price * order.quantity * (self.fee_bps / 10_000.0);
        self.portfolio
            .apply_fill(&self.symbol, order.side, order.quantity, price, fee);

        self.metrics.record_trade(Trade {
            timestamp: bar.timestamp,
            symbol: self.symbol.clone(),
            side: order.side,
            quantity: order.quantity,
            price,
            fee,
            slippage: slippage_cost,
            strategy_id: self.strategy.name().to_string(),
            reason: "strategy".to_string(),
        });
    }

    fn schedule_order(&mut self, bar: &crate::types::Bar, action: crate::types::Action) {
        match action.action_type {
            ActionType::Hold => return,
            ActionType::Buy => {
                if action.size <= 0.0 {
                    return;
                }
                if !self.risk_limits.allows_position(
                    self.portfolio.position_qty(&bar.symbol),
                    action.size,
                ) {
                    return;
                }
                let next_exposure =
                    (self.portfolio.position_qty(&bar.symbol) + action.size) * bar.close;
                let equity = self.portfolio.equity(&bar.symbol, bar.close);
                if !self
                    .risk_limits
                    .allows_exposure(equity, next_exposure)
                {
                    return;
                }
                self.pending_order = Some(Order {
                    id: self.next_order_id,
                    side: Side::Buy,
                    quantity: action.size,
                    limit_price: None,
                    timestamp: bar.timestamp,
                });
                self.next_order_id += 1;
            }
            ActionType::Sell => {
                if action.size <= 0.0 {
                    return;
                }
                let available = self.portfolio.position_qty(&bar.symbol);
                if available <= 0.0 {
                    return;
                }
                let qty = action.size.min(available);
                self.pending_order = Some(Order {
                    id: self.next_order_id,
                    side: Side::Sell,
                    quantity: qty,
                    limit_price: None,
                    timestamp: bar.timestamp,
                });
                self.next_order_id += 1;
            }
        }
    }

    fn record_equity(&mut self, bar: &crate::types::Bar) {
        let equity = self.portfolio.equity(&bar.symbol, bar.close);
        let unrealized = self.portfolio.unrealized_pnl(&bar.symbol, bar.close);
        let realized = self.portfolio.realized_pnl();

        self.metrics.record_equity(EquityPoint {
            timestamp: bar.timestamp,
            equity,
            cash: self.portfolio.cash(),
            position_qty: self.portfolio.position_qty(&bar.symbol),
            unrealized_pnl: unrealized,
            realized_pnl: realized,
        });

        let drawdown = self.metrics.max_drawdown();
        if !self.risk_limits.allows_drawdown(drawdown) {
            self.halt_trading = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BacktestRunner;
    use crate::market_data::MarketDataSource;
    use crate::risk::RiskLimits;
    use crate::strategy::Strategy;
    use crate::types::Bar;

    struct DummyDataSource {
        bars: Vec<Bar>,
        index: usize,
    }

    impl DummyDataSource {
        fn new(bars: Vec<Bar>) -> Self {
            Self { bars, index: 0 }
        }
    }

    impl MarketDataSource for DummyDataSource {
        fn next_bar(&mut self) -> Option<Bar> {
            if self.index >= self.bars.len() {
                return None;
            }

            let bar = self.bars[self.index].clone();
            self.index += 1;
            Some(bar)
        }
    }

    struct DummyStrategy;

    impl Strategy for DummyStrategy {
        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[test]
    fn run_counts_processed_bars() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            },
        ];

        let data = DummyDataSource::new(bars);
        let strategy = DummyStrategy;
        let mut runner = BacktestRunner::new(
            strategy,
            data,
            RiskLimits::default(),
            1000.0,
            0.0,
            0.0,
            "BTCUSD".to_string(),
        );
        let result = runner.run();

        assert_eq!(result.summary.bars_processed, 2);
    }
}
