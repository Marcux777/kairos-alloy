use crate::value_objects::side::Side;

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

