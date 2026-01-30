#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub symbol: String,
    pub timestamp: i64,
    pub price: f64,
    pub size: f64,
}
