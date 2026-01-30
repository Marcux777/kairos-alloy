#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionModel {
    Simple,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderKind {
    Market,
    Limit,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceReference {
    Close,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
}

#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub model: ExecutionModel,
    pub latency_bars: u64,

    pub buy_kind: OrderKind,
    pub sell_kind: OrderKind,

    pub price_reference: PriceReference,
    pub limit_offset_bps: f64,
    pub stop_offset_bps: f64,

    pub spread_bps: f64,
    pub slippage_bps: f64,

    pub max_fill_pct_of_volume: f64,

    pub tif: TimeInForce,
    pub expire_after_bars: Option<u64>,
}

impl ExecutionConfig {
    pub fn simple(slippage_bps: f64) -> Self {
        Self {
            model: ExecutionModel::Simple,
            latency_bars: 1,
            buy_kind: OrderKind::Market,
            sell_kind: OrderKind::Market,
            price_reference: PriceReference::Close,
            limit_offset_bps: 0.0,
            stop_offset_bps: 0.0,
            spread_bps: 0.0,
            slippage_bps,
            max_fill_pct_of_volume: 1.0,
            tif: TimeInForce::Gtc,
            expire_after_bars: None,
        }
    }

    pub fn complete_defaults(slippage_bps: f64) -> Self {
        Self {
            model: ExecutionModel::Complete,
            latency_bars: 1,
            buy_kind: OrderKind::Market,
            sell_kind: OrderKind::Market,
            price_reference: PriceReference::Close,
            limit_offset_bps: 10.0,
            stop_offset_bps: 10.0,
            spread_bps: 0.0,
            slippage_bps,
            max_fill_pct_of_volume: 0.25,
            tif: TimeInForce::Gtc,
            expire_after_bars: None,
        }
    }
}

