#[derive(Debug, Clone, Copy)]
pub enum MissingValuePolicy {
    Error,
    ZeroFill,
    ForwardFill,
    DropRow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SentimentPoint {
    pub timestamp: i64,
    pub values: Vec<f64>,
}

#[derive(Debug, Default)]
pub struct SentimentReport {
    pub duplicates: usize,
    pub out_of_order: usize,
    pub missing_values: usize,
    pub invalid_values: usize,
    pub dropped_rows: usize,
    pub first_timestamp: Option<i64>,
    pub last_timestamp: Option<i64>,
    pub first_duplicate: Option<i64>,
    pub first_out_of_order: Option<i64>,
    pub schema: Vec<String>,
}
