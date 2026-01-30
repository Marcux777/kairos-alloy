use crate::value_objects::side::Side;

#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: u64,
    pub side: Side,
    pub quantity: f64,
    pub limit_price: Option<f64>,
    pub timestamp: i64,
}

