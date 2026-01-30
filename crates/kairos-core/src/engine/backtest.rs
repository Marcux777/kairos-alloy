//! Backtesting engine (bar-based).
//!
//! # What this simulator is (MVP-friendly)
//! - Deterministic, bar-based simulation over OHLCV candles.
//! - Long-only portfolio model (cash + single-asset position).
//! - Two execution modes:
//!   - `ExecutionModel::Simple`: applies a fixed market impact model (spread + slippage) and fills at a bar reference.
//!   - `ExecutionModel::Complete`: keeps a small order book (market/limit/stop), supports time-in-force, latency in bars,
//!     and volume caps (partial fills across bars).
//! - Fees and slippage are modeled as aggregated bps costs (not venue-accurate maker/taker tiers).
//!
//! # Important simplifications (not “real trading”)
//! Even with `ExecutionModel::Complete`, this is still a simplified simulator:
//! - No L2 order book / queue priority / matching engine; fills are derived from OHLCV constraints.
//! - No exchange microstructure (crossing, rebates, maker/taker, funding, borrow, liquidation).
//! - No realistic latency distribution (latency is modeled as an integer number of bars).
//! - No multi-asset portfolio, no hedging, no shorting, no leverage/margin.
//! - No complex order types (OCO, iceberg, post-only, reduce-only, etc.).
//! - Price references are bar-based (e.g., next bar open / within-bar touch), not tick-accurate.
use crate::engine::execution::{
    ExecutionConfig, ExecutionModel, OrderKind, PriceReference, TimeInForce,
};
use crate::market_data::MarketDataSource;
use crate::metrics::{MetricsConfig, MetricsState, MetricsSummary};
use crate::portfolio::Portfolio;
use crate::report::AuditEvent;
use crate::risk::RiskLimits;
use crate::strategy::Strategy;
use crate::types::{ActionType, EquityPoint, Side, Trade};
use serde_json::json;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy)]
pub enum OrderSizeMode {
    Quantity,
    PctEquity,
}

#[derive(Debug, Clone)]
struct SimOrder {
    id: u64,
    side: Side,
    remaining_qty: f64,

    kind: OrderKind,
    limit_price: Option<f64>,
    stop_price: Option<f64>,

    submitted_bar_index: u64,
    ready_bar_index: u64,
    expires_bar_index: Option<u64>,
    tif: TimeInForce,
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
    execution: ExecutionConfig,
    bar_index: u64,
    open_orders: VecDeque<SimOrder>,
    next_order_id: u64,
    fee_bps: f64,
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
            execution: ExecutionConfig::simple(slippage_bps),
            bar_index: 0,
            open_orders: VecDeque::new(),
            next_order_id: 1,
            fee_bps,
            symbol,
            halt_trading: false,
            size_mode,
            audit_events: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_execution(
        run_id: String,
        strategy: S,
        data: D,
        risk_limits: RiskLimits,
        initial_capital: f64,
        metrics_config: MetricsConfig,
        fee_bps: f64,
        symbol: String,
        size_mode: OrderSizeMode,
        execution: ExecutionConfig,
    ) -> Self {
        Self {
            run_id,
            strategy,
            data,
            portfolio: Portfolio::new_with_cash(initial_capital),
            risk_limits,
            metrics: MetricsState::new(metrics_config),
            execution,
            bar_index: 0,
            open_orders: VecDeque::new(),
            next_order_id: 1,
            fee_bps,
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
                },
                "execution": {
                    "model": match self.execution.model {
                        ExecutionModel::Simple => "simple",
                        ExecutionModel::Complete => "complete",
                    },
                    "latency_bars": self.execution.latency_bars,
                    "buy_kind": format!("{:?}", self.execution.buy_kind).to_lowercase(),
                    "sell_kind": format!("{:?}", self.execution.sell_kind).to_lowercase(),
                    "tif": format!("{:?}", self.execution.tif).to_lowercase(),
                    "max_fill_pct_of_volume": self.execution.max_fill_pct_of_volume,
                    "spread_bps": self.execution.spread_bps,
                    "slippage_bps": self.execution.slippage_bps,
                },
            }),
        });

        while let Some(bar) = self.data.next_bar() {
            self.bar_index = self.bar_index.saturating_add(1);
            self.process_open_orders(&bar);

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

    fn process_open_orders(&mut self, bar: &crate::types::Bar) {
        let mut remaining_liquidity_qty = self.bar_liquidity_cap_qty(bar);
        let fee_rate = self.fee_bps / 10_000.0;
        let is_liquidity_infinite = !remaining_liquidity_qty.is_finite();

        let mut next_queue: VecDeque<SimOrder> = VecDeque::with_capacity(self.open_orders.len());
        while let Some(mut order) = self.open_orders.pop_front() {
            if let Some(expires) = order.expires_bar_index {
                if self.bar_index > expires {
                    self.audit_events.push(AuditEvent {
                        run_id: self.run_id.clone(),
                        timestamp: bar.timestamp,
                        stage: "order".to_string(),
                        symbol: Some(self.symbol.clone()),
                        action: "cancel".to_string(),
                        error: Some("expired".to_string()),
                        details: json!({
                            "order_id": order.id,
                            "side": format!("{:?}", order.side),
                            "kind": format!("{:?}", order.kind).to_lowercase(),
                            "remaining_qty": order.remaining_qty,
                            "submitted_bar_index": order.submitted_bar_index,
                            "ready_bar_index": order.ready_bar_index,
                            "expires_bar_index": order.expires_bar_index,
                        }),
                    });
                    continue;
                }
            }

            if self.bar_index < order.ready_bar_index {
                next_queue.push_back(order);
                continue;
            }

            let first_active_bar = self.bar_index == order.ready_bar_index;

            let (raw_price, price_reason) = match self.raw_fill_price(bar, &order) {
                Some(v) => v,
                None => {
                    if matches!(order.tif, TimeInForce::Ioc | TimeInForce::Fok) && first_active_bar
                    {
                        self.audit_events.push(AuditEvent {
                            run_id: self.run_id.clone(),
                            timestamp: bar.timestamp,
                            stage: "order".to_string(),
                            symbol: Some(self.symbol.clone()),
                            action: "cancel".to_string(),
                            error: Some(
                                match order.tif {
                                    TimeInForce::Ioc => "ioc_unfilled",
                                    TimeInForce::Fok => "fok_unfillable",
                                    TimeInForce::Gtc => "unreachable",
                                }
                                .to_string(),
                            ),
                            details: json!({
                                "order_id": order.id,
                                "side": format!("{:?}", order.side),
                                "kind": format!("{:?}", order.kind).to_lowercase(),
                                "remaining_qty": order.remaining_qty,
                                "reason": "not_triggered",
                            }),
                        });
                        continue;
                    }
                    next_queue.push_back(order);
                    continue;
                }
            };

            if raw_price <= 0.0 || !raw_price.is_finite() {
                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "cancel".to_string(),
                    error: Some("invalid_price".to_string()),
                    details: json!({
                        "order_id": order.id,
                        "side": format!("{:?}", order.side),
                        "kind": format!("{:?}", order.kind).to_lowercase(),
                        "raw_price": raw_price,
                    }),
                });
                continue;
            }

            if matches!(self.execution.model, ExecutionModel::Complete)
                && (bar.volume <= 0.0 || !bar.volume.is_finite())
            {
                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "cancel".to_string(),
                    error: Some("invalid_volume".to_string()),
                    details: json!({
                        "order_id": order.id,
                        "side": format!("{:?}", order.side),
                        "kind": format!("{:?}", order.kind).to_lowercase(),
                        "bar_volume": bar.volume,
                    }),
                });
                continue;
            }

            let impact_bps = (self.execution.spread_bps / 2.0) + self.execution.slippage_bps;
            let exec_price = match order.side {
                Side::Buy => raw_price * (1.0 + impact_bps / 10_000.0),
                Side::Sell => raw_price * (1.0 - impact_bps / 10_000.0),
            };

            if exec_price <= 0.0 || !exec_price.is_finite() {
                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "cancel".to_string(),
                    error: Some("invalid_exec_price".to_string()),
                    details: json!({
                        "order_id": order.id,
                        "side": format!("{:?}", order.side),
                        "kind": format!("{:?}", order.kind).to_lowercase(),
                        "raw_price": raw_price,
                        "exec_price": exec_price,
                    }),
                });
                continue;
            }

            let mut desired_qty = order.remaining_qty;
            if !is_liquidity_infinite {
                desired_qty = desired_qty.min(remaining_liquidity_qty.max(0.0));
            }

            let cash = self.portfolio.cash();
            let denom = exec_price * (1.0 + fee_rate);
            let max_qty_by_cash = if order.side == Side::Buy && denom > 0.0 && denom.is_finite() {
                if cash > 0.0 && cash.is_finite() {
                    cash / denom
                } else {
                    0.0
                }
            } else {
                f64::INFINITY
            };

            let can_fill_all_by_liquidity = desired_qty + 1e-12 >= order.remaining_qty;
            let can_fill_all_by_cash = max_qty_by_cash + 1e-12 >= order.remaining_qty;

            let max_qty_by_cash_json = if order.side == Side::Buy {
                json!(max_qty_by_cash)
            } else {
                json!(null)
            };

            if matches!(order.tif, TimeInForce::Fok)
                && first_active_bar
                && (!can_fill_all_by_liquidity
                    || (order.side == Side::Buy && !can_fill_all_by_cash))
            {
                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "cancel".to_string(),
                    error: Some("fok_unfillable".to_string()),
                    details: json!({
                        "order_id": order.id,
                        "side": format!("{:?}", order.side),
                        "kind": format!("{:?}", order.kind).to_lowercase(),
                        "remaining_qty": order.remaining_qty,
                        "max_qty_by_liquidity": desired_qty,
                        "max_qty_by_cash": max_qty_by_cash_json,
                        "price_reason": price_reason,
                    }),
                });
                continue;
            }

            let mut fill_qty = desired_qty;
            if order.side == Side::Buy && fill_qty.is_finite() {
                fill_qty = fill_qty.min(max_qty_by_cash).max(0.0);
            }

            if fill_qty <= 0.0 || !fill_qty.is_finite() {
                if matches!(order.tif, TimeInForce::Ioc) && first_active_bar {
                    self.audit_events.push(AuditEvent {
                        run_id: self.run_id.clone(),
                        timestamp: bar.timestamp,
                        stage: "order".to_string(),
                        symbol: Some(self.symbol.clone()),
                        action: "cancel".to_string(),
                        error: Some("ioc_unfilled".to_string()),
                        details: json!({
                            "order_id": order.id,
                            "side": format!("{:?}", order.side),
                            "kind": format!("{:?}", order.kind).to_lowercase(),
                            "remaining_qty": order.remaining_qty,
                            "reason": "no_fill_qty",
                        }),
                    });
                    continue;
                }
                next_queue.push_back(order);
                continue;
            }

            let fee = exec_price * fill_qty * fee_rate;
            let impact_cost = (exec_price - raw_price).abs() * fill_qty;

            self.portfolio
                .apply_fill(&self.symbol, order.side, fill_qty, exec_price, fee);

            self.metrics.record_trade(Trade {
                timestamp: bar.timestamp,
                symbol: self.symbol.clone(),
                side: order.side,
                quantity: fill_qty,
                price: exec_price,
                fee,
                slippage: impact_cost,
                strategy_id: self.strategy.name().to_string(),
                reason: "strategy".to_string(),
            });

            if !is_liquidity_infinite {
                remaining_liquidity_qty = (remaining_liquidity_qty - fill_qty).max(0.0);
            }

            let was_partial = fill_qty + 1e-12 < order.remaining_qty;
            self.audit_events.push(AuditEvent {
                run_id: self.run_id.clone(),
                timestamp: bar.timestamp,
                stage: "trade".to_string(),
                symbol: Some(self.symbol.clone()),
                action: format!("{:?}", order.side),
                error: None,
                details: json!({
                    "qty": fill_qty,
                    "price": exec_price,
                    "fee": fee,
                    "slippage": impact_cost,
                    "raw_price": raw_price,
                    "price_reason": price_reason,
                    "order_id": order.id,
                    "kind": format!("{:?}", order.kind).to_lowercase(),
                    "strategy_id": self.strategy.name(),
                    "tif": format!("{:?}", order.tif).to_lowercase(),
                }),
            });

            order.remaining_qty = (order.remaining_qty - fill_qty).max(0.0);

            if matches!(order.tif, TimeInForce::Ioc) && first_active_bar {
                if order.remaining_qty > 0.0 {
                    self.audit_events.push(AuditEvent {
                        run_id: self.run_id.clone(),
                        timestamp: bar.timestamp,
                        stage: "order".to_string(),
                        symbol: Some(self.symbol.clone()),
                        action: "cancel".to_string(),
                        error: Some("ioc_partial_cancel".to_string()),
                        details: json!({
                            "order_id": order.id,
                            "side": format!("{:?}", order.side),
                            "kind": format!("{:?}", order.kind).to_lowercase(),
                            "remaining_qty": order.remaining_qty,
                        }),
                    });
                }
                continue;
            }

            if was_partial {
                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "partial_fill".to_string(),
                    error: None,
                    details: json!({
                        "order_id": order.id,
                        "side": format!("{:?}", order.side),
                        "kind": format!("{:?}", order.kind).to_lowercase(),
                        "filled_qty": fill_qty,
                        "remaining_qty": order.remaining_qty,
                    }),
                });
            }

            if order.remaining_qty > 0.0 {
                next_queue.push_back(order);
            }
        }

        self.open_orders = next_queue;
    }

    fn raw_fill_price(
        &self,
        bar: &crate::types::Bar,
        order: &SimOrder,
    ) -> Option<(f64, &'static str)> {
        match order.kind {
            OrderKind::Market => Some((bar.open, "open")),
            OrderKind::Limit => match order.side {
                Side::Buy => {
                    let limit = order.limit_price?;
                    if bar.low <= limit {
                        if bar.open <= limit {
                            Some((bar.open, "open<=limit"))
                        } else {
                            Some((limit, "touch_limit"))
                        }
                    } else {
                        None
                    }
                }
                Side::Sell => {
                    let limit = order.limit_price?;
                    if bar.high >= limit {
                        if bar.open >= limit {
                            Some((bar.open, "open>=limit"))
                        } else {
                            Some((limit, "touch_limit"))
                        }
                    } else {
                        None
                    }
                }
            },
            OrderKind::Stop => match order.side {
                Side::Buy => {
                    let stop = order.stop_price?;
                    if bar.high >= stop {
                        if bar.open >= stop {
                            Some((bar.open, "open>=stop"))
                        } else {
                            Some((stop, "touch_stop"))
                        }
                    } else {
                        None
                    }
                }
                Side::Sell => {
                    let stop = order.stop_price?;
                    if bar.low <= stop {
                        if bar.open <= stop {
                            Some((bar.open, "open<=stop"))
                        } else {
                            Some((stop, "touch_stop"))
                        }
                    } else {
                        None
                    }
                }
            },
        }
    }

    fn bar_liquidity_cap_qty(&self, bar: &crate::types::Bar) -> f64 {
        match self.execution.model {
            ExecutionModel::Simple => f64::INFINITY,
            ExecutionModel::Complete => {
                if bar.volume <= 0.0 || !bar.volume.is_finite() {
                    0.0
                } else {
                    let pct = self.execution.max_fill_pct_of_volume;
                    if pct <= 0.0 || !pct.is_finite() {
                        0.0
                    } else {
                        bar.volume * pct.min(1.0)
                    }
                }
            }
        }
    }

    fn reserved_sell_qty(&self) -> f64 {
        self.open_orders
            .iter()
            .filter(|o| o.side == Side::Sell)
            .map(|o| o.remaining_qty)
            .sum()
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

                if self.portfolio.cash() <= 0.0 || !self.portfolio.cash().is_finite() {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "insufficient_cash",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }

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

                let kind = self.execution.buy_kind;
                let ref_price = match self.execution.price_reference {
                    PriceReference::Close => bar.close,
                    PriceReference::Open => bar.open,
                };
                if ref_price <= 0.0 || !ref_price.is_finite() {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "ref_price_not_positive",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }

                let limit_price = match kind {
                    OrderKind::Limit => {
                        Some(ref_price * (1.0 - self.execution.limit_offset_bps / 10_000.0))
                    }
                    _ => None,
                };
                let stop_price = match kind {
                    OrderKind::Stop => {
                        Some(ref_price * (1.0 + self.execution.stop_offset_bps / 10_000.0))
                    }
                    _ => None,
                };

                let latency = self.execution.latency_bars.max(1);
                let submitted = self.bar_index;
                let ready = submitted.saturating_add(latency);
                let expires = self
                    .execution
                    .expire_after_bars
                    .and_then(|n| (n > 0).then_some(ready.saturating_add(n.saturating_sub(1))));

                if matches!(self.execution.model, ExecutionModel::Simple) {
                    self.open_orders.clear();
                }

                let order = SimOrder {
                    id: self.next_order_id,
                    side: Side::Buy,
                    remaining_qty: qty,
                    kind,
                    limit_price,
                    stop_price,
                    submitted_bar_index: submitted,
                    ready_bar_index: ready,
                    expires_bar_index: expires,
                    tif: self.execution.tif,
                };
                self.next_order_id += 1;
                self.open_orders.push_back(order.clone());

                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "submit".to_string(),
                    error: None,
                    details: json!({
                        "order_id": order.id,
                        "side": "BUY",
                        "requested_size": requested_size,
                        "resolved_qty": qty,
                        "size_mode": match self.size_mode {
                            OrderSizeMode::Quantity => "qty",
                            OrderSizeMode::PctEquity => "pct_equity",
                        },
                        "kind": format!("{:?}", kind).to_lowercase(),
                        "tif": format!("{:?}", order.tif).to_lowercase(),
                        "ref_price": ref_price,
                        "limit_price": order.limit_price,
                        "stop_price": order.stop_price,
                        "latency_bars": latency,
                        "submitted_bar_index": submitted,
                        "ready_bar_index": ready,
                        "expires_bar_index": expires,
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

                let position_qty = self.portfolio.position_qty(&bar.symbol);
                if position_qty <= 0.0 {
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
                    Ok(resolved) if resolved > 0.0 => resolved,
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

                let reserved = self.reserved_sell_qty();
                let available = (position_qty - reserved).max(0.0);
                if available <= 0.0 {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "position_reserved",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }
                let qty = resolved.min(available);
                if qty <= 0.0 {
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

                let kind = self.execution.sell_kind;
                let ref_price = match self.execution.price_reference {
                    PriceReference::Close => bar.close,
                    PriceReference::Open => bar.open,
                };
                if ref_price <= 0.0 || !ref_price.is_finite() {
                    self.audit_events.push(order_reject_event(
                        &self.run_id,
                        bar.timestamp,
                        &self.symbol,
                        self.strategy.name(),
                        "ref_price_not_positive",
                        action.action_type,
                        requested_size,
                        self.size_mode,
                    ));
                    return;
                }

                let limit_price = match kind {
                    OrderKind::Limit => {
                        Some(ref_price * (1.0 + self.execution.limit_offset_bps / 10_000.0))
                    }
                    _ => None,
                };
                let stop_price = match kind {
                    OrderKind::Stop => {
                        Some(ref_price * (1.0 - self.execution.stop_offset_bps / 10_000.0))
                    }
                    _ => None,
                };

                let latency = self.execution.latency_bars.max(1);
                let submitted = self.bar_index;
                let ready = submitted.saturating_add(latency);
                let expires = self
                    .execution
                    .expire_after_bars
                    .and_then(|n| (n > 0).then_some(ready.saturating_add(n.saturating_sub(1))));

                if matches!(self.execution.model, ExecutionModel::Simple) {
                    self.open_orders.clear();
                }

                let order = SimOrder {
                    id: self.next_order_id,
                    side: Side::Sell,
                    remaining_qty: qty,
                    kind,
                    limit_price,
                    stop_price,
                    submitted_bar_index: submitted,
                    ready_bar_index: ready,
                    expires_bar_index: expires,
                    tif: self.execution.tif,
                };
                self.next_order_id += 1;
                self.open_orders.push_back(order.clone());

                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "order".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "submit".to_string(),
                    error: None,
                    details: json!({
                        "order_id": order.id,
                        "side": "SELL",
                        "requested_size": requested_size,
                        "resolved_qty": qty,
                        "size_mode": match self.size_mode {
                            OrderSizeMode::Quantity => "qty",
                            OrderSizeMode::PctEquity => "pct_equity",
                        },
                        "kind": format!("{:?}", kind).to_lowercase(),
                        "tif": format!("{:?}", order.tif).to_lowercase(),
                        "ref_price": ref_price,
                        "limit_price": order.limit_price,
                        "stop_price": order.stop_price,
                        "latency_bars": latency,
                        "submitted_bar_index": submitted,
                        "ready_bar_index": ready,
                        "expires_bar_index": expires,
                        "strategy_id": self.strategy.name(),
                        "reserved_sell_qty": reserved,
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
    use crate::engine::execution::{
        ExecutionConfig, ExecutionModel, OrderKind, PriceReference, TimeInForce,
    };
    use crate::market_data::MarketDataSource;
    use crate::metrics::MetricsConfig;
    use crate::risk::RiskLimits;
    use crate::strategy::Strategy;
    use crate::types::{Action, ActionType, Bar};

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

    struct BuyOnceStrategy {
        size: f64,
        used: bool,
    }

    impl BuyOnceStrategy {
        fn new(size: f64) -> Self {
            Self { size, used: false }
        }
    }

    impl Strategy for BuyOnceStrategy {
        fn name(&self) -> &str {
            "buy_once"
        }

        fn on_bar(&mut self, _bar: &Bar, _portfolio: &crate::portfolio::Portfolio) -> Action {
            if self.used {
                return Action::hold();
            }
            self.used = true;
            Action {
                action_type: ActionType::Buy,
                size: self.size,
            }
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

    #[test]
    fn buy_qty_never_makes_cash_negative() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1.0,
            },
        ];

        let data = DummyDataSource::new(bars);
        let strategy = BuyOnceStrategy::new(10_000.0);
        let mut runner = BacktestRunner::new(
            "run_cash".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            100.0,
            MetricsConfig::default(),
            0.0,
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
        );
        let result = runner.run();

        assert!(!result.equity.is_empty());
        assert!(result.equity.iter().all(|p| p.cash >= -1e-9));
    }

    #[test]
    fn buy_qty_with_zero_cash_is_rejected() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1.0,
            },
        ];

        let data = DummyDataSource::new(bars);
        let strategy = BuyOnceStrategy::new(1.0);
        let mut runner = BacktestRunner::new(
            "run_zero_cash".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            0.0,
            MetricsConfig::default(),
            0.0,
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
        );
        let result = runner.run();

        assert!(result.trades.is_empty());
        let has_insufficient_cash = result
            .audit_events
            .iter()
            .any(|e| e.error.as_deref() == Some("insufficient_cash"));
        if !has_insufficient_cash {
            for e in &result.audit_events {
                println!(
                    "audit_event: stage={} action={} error={:?}",
                    e.stage, e.action, e.error
                );
            }
        }
        assert!(has_insufficient_cash);
    }

    struct SequenceStrategy {
        actions: Vec<Action>,
        i: usize,
    }

    impl SequenceStrategy {
        fn new(actions: Vec<Action>) -> Self {
            Self { actions, i: 0 }
        }
    }

    impl Strategy for SequenceStrategy {
        fn name(&self) -> &str {
            "sequence"
        }

        fn on_bar(&mut self, _bar: &Bar, _portfolio: &crate::portfolio::Portfolio) -> Action {
            let action = self.actions.get(self.i).copied().unwrap_or(Action::hold());
            self.i = self.i.saturating_add(1);
            action
        }
    }

    #[test]
    fn complete_limit_buy_fills_on_touch_low() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 10_000.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 100.0,
                high: 100.0,
                low: 98.0,
                close: 100.0,
                volume: 10_000.0,
            },
        ];

        let execution = ExecutionConfig {
            model: ExecutionModel::Complete,
            latency_bars: 1,
            buy_kind: OrderKind::Limit,
            sell_kind: OrderKind::Market,
            price_reference: PriceReference::Close,
            limit_offset_bps: 100.0,
            stop_offset_bps: 100.0,
            spread_bps: 0.0,
            slippage_bps: 0.0,
            max_fill_pct_of_volume: 1.0,
            tif: TimeInForce::Gtc,
            expire_after_bars: None,
        };

        let data = DummyDataSource::new(bars);
        let strategy = BuyOnceStrategy::new(1.0);
        let mut runner = BacktestRunner::new_with_execution(
            "limit_buy".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            10_000.0,
            MetricsConfig::default(),
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
            execution,
        );
        let result = runner.run();

        assert_eq!(result.trades.len(), 1);
        let trade = &result.trades[0];
        assert_eq!(trade.side, crate::types::Side::Buy);
        assert!((trade.price - 99.0).abs() < 1e-9);
    }

    #[test]
    fn complete_limit_buy_does_not_fill_when_not_touched() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 10_000.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 100.0,
                high: 100.0,
                low: 99.5,
                close: 100.0,
                volume: 10_000.0,
            },
        ];

        let execution = ExecutionConfig {
            model: ExecutionModel::Complete,
            latency_bars: 1,
            buy_kind: OrderKind::Limit,
            sell_kind: OrderKind::Market,
            price_reference: PriceReference::Close,
            limit_offset_bps: 100.0,
            stop_offset_bps: 100.0,
            spread_bps: 0.0,
            slippage_bps: 0.0,
            max_fill_pct_of_volume: 1.0,
            tif: TimeInForce::Gtc,
            expire_after_bars: None,
        };

        let data = DummyDataSource::new(bars);
        let strategy = BuyOnceStrategy::new(1.0);
        let mut runner = BacktestRunner::new_with_execution(
            "limit_buy_no_touch".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            10_000.0,
            MetricsConfig::default(),
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
            execution,
        );
        let result = runner.run();
        assert!(result.trades.is_empty());
    }

    #[test]
    fn complete_stop_sell_triggers_on_touch_low() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 10_000.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 10_000.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 3,
                open: 100.0,
                high: 101.0,
                low: 98.0,
                close: 100.0,
                volume: 10_000.0,
            },
        ];

        let execution = ExecutionConfig {
            model: ExecutionModel::Complete,
            latency_bars: 1,
            buy_kind: OrderKind::Market,
            sell_kind: OrderKind::Stop,
            price_reference: PriceReference::Close,
            limit_offset_bps: 100.0,
            stop_offset_bps: 100.0,
            spread_bps: 0.0,
            slippage_bps: 0.0,
            max_fill_pct_of_volume: 1.0,
            tif: TimeInForce::Gtc,
            expire_after_bars: None,
        };

        let data = DummyDataSource::new(bars);
        let strategy = SequenceStrategy::new(vec![
            Action {
                action_type: ActionType::Buy,
                size: 1.0,
            },
            Action {
                action_type: ActionType::Sell,
                size: 1.0,
            },
        ]);
        let mut runner = BacktestRunner::new_with_execution(
            "stop_sell".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            10_000.0,
            MetricsConfig::default(),
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
            execution,
        );
        let result = runner.run();

        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[0].side, crate::types::Side::Buy);
        assert_eq!(result.trades[0].timestamp, 2);
        assert_eq!(result.trades[1].side, crate::types::Side::Sell);
        assert_eq!(result.trades[1].timestamp, 3);
        assert!((result.trades[1].price - 99.0).abs() < 1e-9);
    }

    #[test]
    fn complete_latency_delays_activation() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10_000.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10_000.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 3,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10_000.0,
            },
        ];

        let execution = ExecutionConfig {
            model: ExecutionModel::Complete,
            latency_bars: 2,
            buy_kind: OrderKind::Market,
            sell_kind: OrderKind::Market,
            price_reference: PriceReference::Close,
            limit_offset_bps: 0.0,
            stop_offset_bps: 0.0,
            spread_bps: 0.0,
            slippage_bps: 0.0,
            max_fill_pct_of_volume: 1.0,
            tif: TimeInForce::Gtc,
            expire_after_bars: None,
        };

        let data = DummyDataSource::new(bars);
        let strategy = BuyOnceStrategy::new(1.0);
        let mut runner = BacktestRunner::new_with_execution(
            "latency".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            10_000.0,
            MetricsConfig::default(),
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
            execution,
        );
        let result = runner.run();

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].timestamp, 3);
    }

    #[test]
    fn complete_volume_cap_partial_fill_across_bars() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 3,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 4,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10.0,
            },
        ];

        let execution = ExecutionConfig {
            model: ExecutionModel::Complete,
            latency_bars: 1,
            buy_kind: OrderKind::Market,
            sell_kind: OrderKind::Market,
            price_reference: PriceReference::Close,
            limit_offset_bps: 0.0,
            stop_offset_bps: 0.0,
            spread_bps: 0.0,
            slippage_bps: 0.0,
            max_fill_pct_of_volume: 0.1,
            tif: TimeInForce::Gtc,
            expire_after_bars: None,
        };

        let data = DummyDataSource::new(bars);
        let strategy = BuyOnceStrategy::new(3.0);
        let mut runner = BacktestRunner::new_with_execution(
            "vol_cap".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            10_000.0,
            MetricsConfig::default(),
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
            execution,
        );
        let result = runner.run();

        assert_eq!(result.trades.len(), 3);
        let total_qty: f64 = result.trades.iter().map(|t| t.quantity).sum();
        assert!((total_qty - 3.0).abs() < 1e-9);
    }

    #[test]
    fn complete_fok_cancels_if_volume_insufficient() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 1,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 2,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 10.0,
            },
        ];

        let execution = ExecutionConfig {
            model: ExecutionModel::Complete,
            latency_bars: 1,
            buy_kind: OrderKind::Market,
            sell_kind: OrderKind::Market,
            price_reference: PriceReference::Close,
            limit_offset_bps: 0.0,
            stop_offset_bps: 0.0,
            spread_bps: 0.0,
            slippage_bps: 0.0,
            max_fill_pct_of_volume: 0.1,
            tif: TimeInForce::Fok,
            expire_after_bars: None,
        };

        let data = DummyDataSource::new(bars);
        let strategy = BuyOnceStrategy::new(3.0);
        let mut runner = BacktestRunner::new_with_execution(
            "fok".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            10_000.0,
            MetricsConfig::default(),
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::Quantity,
            execution,
        );
        let result = runner.run();

        assert!(result.trades.is_empty());
        let has_fok_cancel = result
            .audit_events
            .iter()
            .any(|e| e.error.as_deref() == Some("fok_unfillable"));
        assert!(has_fok_cancel);
    }
}
