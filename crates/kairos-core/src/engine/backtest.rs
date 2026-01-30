use crate::market_data::MarketDataSource;
use crate::metrics::{MetricsConfig, MetricsState, MetricsSummary};
use crate::portfolio::Portfolio;
use crate::report::AuditEvent;
use crate::risk::RiskLimits;
use crate::strategy::Strategy;
use crate::types::{ActionType, EquityPoint, Order, Side, Trade};
use serde_json::json;

#[derive(Debug, Clone, Copy)]
pub enum OrderSizeMode {
    Quantity,
    PctEquity,
}

#[derive(Debug)]
pub struct BacktestRunner<S, D>
where
    S: Strategy,
    D: MarketDataSource,
{
    run_id: String,
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
    size_mode: OrderSizeMode,
    audit_events: Vec<AuditEvent>,
}

pub struct BacktestResults {
    pub summary: MetricsSummary,
    pub trades: Vec<Trade>,
    pub equity: Vec<EquityPoint>,
    pub audit_events: Vec<AuditEvent>,
}

impl<S, D> BacktestRunner<S, D>
where
    S: Strategy,
    D: MarketDataSource,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: String,
        strategy: S,
        data: D,
        risk_limits: RiskLimits,
        initial_capital: f64,
        metrics_config: MetricsConfig,
        fee_bps: f64,
        slippage_bps: f64,
        symbol: String,
        size_mode: OrderSizeMode,
    ) -> Self {
        Self {
            run_id,
            strategy,
            data,
            portfolio: Portfolio::new_with_cash(initial_capital),
            risk_limits,
            metrics: MetricsState::new(metrics_config),
            pending_order: None,
            next_order_id: 1,
            fee_bps,
            slippage_bps,
            symbol,
            halt_trading: false,
            size_mode,
            audit_events: Vec::new(),
        }
    }

    pub fn run(&mut self) -> BacktestResults {
        self.audit_events.push(AuditEvent {
            run_id: self.run_id.clone(),
            timestamp: 0,
            stage: "engine".to_string(),
            symbol: Some(self.symbol.clone()),
            action: "start".to_string(),
            error: None,
            details: json!({
                "strategy": self.strategy.name(),
                "size_mode": match self.size_mode {
                    OrderSizeMode::Quantity => "qty",
                    OrderSizeMode::PctEquity => "pct_equity",
                }
            }),
        });

        while let Some(bar) = self.data.next_bar() {
            self.execute_pending_order(&bar);

            if !self.halt_trading {
                let action = self.strategy.on_bar(&bar, &self.portfolio);
                self.schedule_order(&bar, action);
            }

            self.record_equity(&bar);

            // Placeholder: extend with full risk/metrics/reporting.
        }

        let mut strategy_events = self.strategy.drain_audit_events();
        self.audit_events.append(&mut strategy_events);

        let (equity, trades, summary) = std::mem::take(&mut self.metrics).into_parts();
        self.audit_events.push(AuditEvent {
            run_id: self.run_id.clone(),
            timestamp: 0,
            stage: "engine".to_string(),
            symbol: Some(self.symbol.clone()),
            action: "complete".to_string(),
            error: None,
            details: json!({
                "bars_processed": summary.bars_processed,
                "trades": summary.trades,
                "net_profit": summary.net_profit,
                "sharpe": summary.sharpe,
                "max_drawdown": summary.max_drawdown,
                "halt_trading": self.halt_trading,
            }),
        });

        self.audit_events.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.stage.cmp(&b.stage))
                .then_with(|| a.action.cmp(&b.action))
        });

        BacktestResults {
            summary,
            trades,
            equity,
            audit_events: std::mem::take(&mut self.audit_events),
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

        self.audit_events.push(AuditEvent {
            run_id: self.run_id.clone(),
            timestamp: bar.timestamp,
            stage: "trade".to_string(),
            symbol: Some(self.symbol.clone()),
            action: format!("{:?}", order.side),
            error: None,
            details: json!({
                "qty": order.quantity,
                "price": price,
                "fee": fee,
                "slippage": slippage_cost,
                "order_id": order.id,
                "strategy_id": self.strategy.name(),
            }),
        });
    }

    fn schedule_order(&mut self, bar: &crate::types::Bar, action: crate::types::Action) {
        let requested_size = action.size;

        match action.action_type {
            ActionType::Hold => (),
            ActionType::Buy => {
                if action.size <= 0.0 {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "non_positive_size",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }
                let qty = match self.resolve_quantity(bar, action.action_type, action.size) {
                    Ok(qty) if qty > 0.0 => qty,
                    Ok(_) => {
                        self.audit_events.push(order_reject_event(
                            &self.run_id,
                            bar.timestamp,
                            &self.symbol,
                            self.strategy.name(),
                            "resolved_qty_non_positive",
                            action.action_type,
                            requested_size,
                            self.size_mode,
                        ));
                        return;
                    }
                    Err(reason) => {
                        self.audit_events.push(order_reject_event(
                            &self.run_id,
                            bar.timestamp,
                            &self.symbol,
                            self.strategy.name(),
                            &reason,
                            action.action_type,
                            requested_size,
                            self.size_mode,
                        ));
                        return;
                    }
                };
                if !self
                    .risk_limits
                    .allows_position(self.portfolio.position_qty(&bar.symbol), qty)
                {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "position_limit",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }
                let next_exposure = (self.portfolio.position_qty(&bar.symbol) + qty) * bar.close;
                let equity = self.portfolio.equity(&bar.symbol, bar.close);
                if !self.risk_limits.allows_exposure(equity, next_exposure) {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "exposure_limit",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }
                self.pending_order = Some(Order {
                    id: self.next_order_id,
                    side: Side::Buy,
                    quantity: qty,
                    limit_price: None,
                    timestamp: bar.timestamp,
                });
                self.next_order_id += 1;

                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "schedule".to_string(),
                    error: None,
                    details: json!({
                        "side": "BUY",
                        "requested_size": requested_size,
                        "resolved_qty": qty,
                        "size_mode": match self.size_mode {
                            OrderSizeMode::Quantity => "qty",
                            OrderSizeMode::PctEquity => "pct_equity",
                        },
                        "strategy_id": self.strategy.name(),
                    }),
                });
            }
            ActionType::Sell => {
                if action.size <= 0.0 {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "non_positive_size",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }
                let available = self.portfolio.position_qty(&bar.symbol);
                if available <= 0.0 {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "no_position",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }
                let resolved = match self.resolve_quantity(bar, action.action_type, action.size) {
                    Ok(qty) => qty,
                    Err(reason) => {
                        self.audit_events.push(order_reject_event(
                            &self.run_id,
                            bar.timestamp,
                            &self.symbol,
                            self.strategy.name(),
                            &reason,
                            action.action_type,
                            requested_size,
                            self.size_mode,
                        ));
                        return;
                    }
                };
                let qty = resolved.min(available);
                self.pending_order = Some(Order {
                    id: self.next_order_id,
                    side: Side::Sell,
                    quantity: qty,
                    limit_price: None,
                    timestamp: bar.timestamp,
                });
                self.next_order_id += 1;

                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "schedule".to_string(),
                    error: None,
                    details: json!({
                        "side": "SELL",
                        "requested_size": requested_size,
                        "resolved_qty": qty,
                        "size_mode": match self.size_mode {
                            OrderSizeMode::Quantity => "qty",
                            OrderSizeMode::PctEquity => "pct_equity",
                        },
                        "strategy_id": self.strategy.name(),
                    }),
                });
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
            if !self.halt_trading {
                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "risk".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "halt_drawdown".to_string(),
                    error: None,
                    details: json!({
                        "drawdown_pct": drawdown,
                        "max_drawdown_pct": self.risk_limits.max_drawdown_pct,
                    }),
                });
            }
            self.halt_trading = true;
        }
    }

    fn resolve_quantity(
        &self,
        bar: &crate::types::Bar,
        action_type: ActionType,
        size: f64,
    ) -> Result<f64, String> {
        if !size.is_finite() {
            return Err("size_not_finite".to_string());
        }
        match self.size_mode {
            OrderSizeMode::Quantity => Ok(size),
            OrderSizeMode::PctEquity => {
                if !(0.0..=1.0).contains(&size) {
                    return Err("pct_out_of_range".to_string());
                }
                let equity = self.portfolio.equity(&bar.symbol, bar.close);
                if equity <= 0.0 || !equity.is_finite() {
                    return Err("equity_not_positive".to_string());
                }
                match action_type {
                    ActionType::Buy => {
                        if bar.close <= 0.0 || !bar.close.is_finite() {
                            return Err("price_not_positive".to_string());
                        }
                        Ok((equity * size) / bar.close)
                    }
                    ActionType::Sell => {
                        let available = self.portfolio.position_qty(&bar.symbol);
                        Ok(available * size)
                    }
                    ActionType::Hold => Ok(0.0),
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn order_reject_event(
    run_id: &str,
    timestamp: i64,
    symbol: &str,
    strategy_id: &str,
    reason: &str,
    action_type: ActionType,
    requested_size: f64,
    size_mode: OrderSizeMode,
) -> AuditEvent {
    AuditEvent {
        run_id: run_id.to_string(),
        timestamp,
        stage: "order".to_string(),
        symbol: Some(symbol.to_string()),
        action: "reject".to_string(),
        error: Some(reason.to_string()),
        details: json!({
            "strategy_id": strategy_id,
            "action_type": format!("{:?}", action_type),
            "requested_size": requested_size,
            "size_mode": match size_mode {
                OrderSizeMode::Quantity => "qty",
                OrderSizeMode::PctEquity => "pct_equity",
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::BacktestRunner;
    use super::OrderSizeMode;
    use crate::market_data::MarketDataSource;
    use crate::metrics::MetricsConfig;
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
            "run1".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            1000.0,
            MetricsConfig::default(),
            0.0,
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
        );
        let result = runner.run();

        assert_eq!(result.summary.bars_processed, 2);
    }
}
