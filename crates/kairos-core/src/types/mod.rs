#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub symbol: String,
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub symbol: String,
    pub timestamp: i64,
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: u64,
    pub side: Side,
    pub quantity: f64,
    pub limit_price: Option<f64>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub order_id: u64,
    pub quantity: f64,
    pub price: f64,
    pub fee: f64,
    pub slippage: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub avg_price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Action {
    pub action_type: ActionType,
    pub size: f64,
}

impl Action {
    pub fn hold() -> Self {
        Self {
            action_type: ActionType::Hold,
            size: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub timestamp: i64,
    pub symbol: String,
    pub side: Side,
    pub quantity: f64,
    pub price: f64,
    pub fee: f64,
    pub slippage: f64,
    pub strategy_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquityPoint {
    pub timestamp: i64,
    pub equity: f64,
    pub cash: f64,
    pub position_qty: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
}
