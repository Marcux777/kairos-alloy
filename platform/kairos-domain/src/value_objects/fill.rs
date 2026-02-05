#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub order_id: u64,
    pub quantity: f64,
    pub price: f64,
    pub fee: f64,
    pub slippage: f64,
    pub timestamp: i64,
}
