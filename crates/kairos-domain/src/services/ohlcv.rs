use crate::value_objects::bar::Bar;

#[derive(Debug, Default, Clone)]
pub struct DataQualityReport {
    pub duplicates: usize,
    pub gaps: usize,
    pub out_of_order: usize,
    pub invalid_close: usize,
    pub first_timestamp: Option<i64>,
    pub last_timestamp: Option<i64>,
    pub first_gap: Option<i64>,
    pub first_duplicate: Option<i64>,
    pub first_out_of_order: Option<i64>,
    pub first_invalid_close: Option<i64>,
    pub max_gap_seconds: Option<i64>,
    pub gap_count: usize,
}

pub fn data_quality_from_bars(
    bars: &[Bar],
    expected_step_seconds: Option<i64>,
) -> DataQualityReport {
    let mut report = DataQualityReport::default();
    if bars.is_empty() {
        return report;
    }

    let step = expected_step_seconds.unwrap_or(1).max(1);
    report.first_timestamp = Some(bars[0].timestamp);
    report.last_timestamp = Some(bars[bars.len() - 1].timestamp);

    let mut last_ts: Option<i64> = None;
    let mut max_gap: Option<i64> = None;

    for bar in bars {
        let ts = bar.timestamp;

        if let Some(prev) = last_ts {
            if ts == prev {
                report.duplicates += 1;
                if report.first_duplicate.is_none() {
                    report.first_duplicate = Some(ts);
                }
            } else if ts < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(ts);
                }
            } else {
                let diff = ts - prev;
                if diff > step {
                    report.gaps += 1;
                    report.gap_count += 1;
                    if report.first_gap.is_none() {
                        report.first_gap = Some(ts);
                    }
                    max_gap = Some(max_gap.map_or(diff, |current| current.max(diff)));
                }
            }
        }

        last_ts = Some(ts);
        report.last_timestamp = Some(ts);
    }

    report.max_gap_seconds = max_gap;
    report
}

pub fn resample_bars(bars: &[Bar], target_step_seconds: i64) -> Result<Vec<Bar>, String> {
    if target_step_seconds <= 0 {
        return Err("target_step_seconds must be > 0".to_string());
    }
    if bars.is_empty() {
        return Ok(Vec::new());
    }

    let mut output = Vec::new();
    let mut current_bucket_start: Option<i64> = None;
    let mut bucket: Option<Bar> = None;

    for bar in bars {
        let bucket_start = bar
            .timestamp
            .saturating_sub(bar.timestamp.rem_euclid(target_step_seconds));

        match current_bucket_start {
            None => {
                current_bucket_start = Some(bucket_start);
                bucket = Some(Bar {
                    symbol: bar.symbol.clone(),
                    timestamp: bucket_start,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                });
            }
            Some(active_start) if active_start == bucket_start => {
                if let Some(ref mut agg) = bucket {
                    agg.high = agg.high.max(bar.high);
                    agg.low = agg.low.min(bar.low);
                    agg.close = bar.close;
                    agg.volume += bar.volume;
                }
            }
            Some(_) => {
                if let Some(agg) = bucket.take() {
                    output.push(agg);
                }
                current_bucket_start = Some(bucket_start);
                bucket = Some(Bar {
                    symbol: bar.symbol.clone(),
                    timestamp: bucket_start,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                });
            }
        }
    }

    if let Some(agg) = bucket {
        output.push(agg);
    }

    Ok(output)
}
