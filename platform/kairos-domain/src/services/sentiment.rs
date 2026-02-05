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

pub fn align_with_bars(
    bar_timestamps: &[i64],
    sentiment: &[SentimentPoint],
    sentiment_lag_seconds: i64,
) -> Vec<Option<SentimentPoint>> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<i64, SentimentPoint> = BTreeMap::new();
    for point in sentiment {
        map.insert(point.timestamp, point.clone());
    }

    bar_timestamps
        .iter()
        .map(|ts| {
            let cutoff = ts.saturating_sub(sentiment_lag_seconds);
            map.range(..=cutoff)
                .next_back()
                .map(|(_, point)| point.clone())
        })
        .collect()
}
