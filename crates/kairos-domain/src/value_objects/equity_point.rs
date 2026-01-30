#[derive(Debug, Clone, PartialEq)]
pub struct EquityPoint {
    pub timestamp: i64,
    pub equity: f64,
    pub cash: f64,
    pub position_qty: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
}
