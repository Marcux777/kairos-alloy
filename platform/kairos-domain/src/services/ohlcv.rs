use crate::value_objects::bar::Bar;
use std::collections::HashSet;

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

    let mut min_ts = i64::MAX;
    let mut max_ts = i64::MIN;

    let mut seen = HashSet::<i64>::with_capacity(bars.len().saturating_mul(2));
    let mut unique_ts = Vec::with_capacity(bars.len());

    let mut last_ts_in_input: Option<i64> = None;

    for bar in bars {
        let ts = bar.timestamp;

        min_ts = min_ts.min(ts);
        max_ts = max_ts.max(ts);

        if !bar.close.is_finite() || bar.close <= 0.0 {
            report.invalid_close += 1;
            if report.first_invalid_close.is_none() {
                report.first_invalid_close = Some(ts);
            }
        }

        if let Some(prev) = last_ts_in_input {
            if ts < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(ts);
                }
            }
        }
        last_ts_in_input = Some(ts);

        if !seen.insert(ts) {
            report.duplicates += 1;
            if report.first_duplicate.is_none() {
                report.first_duplicate = Some(ts);
            }
        } else {
            unique_ts.push(ts);
        }
    }

    report.first_timestamp = Some(min_ts);
    report.last_timestamp = Some(max_ts);

    if unique_ts.is_empty() {
        return report;
    }

    unique_ts.sort();
    let mut max_gap: Option<i64> = None;
    let mut prev = unique_ts[0];
    for &ts in unique_ts.iter().skip(1) {
        let diff = ts - prev;
        if diff > step {
            report.gaps += 1;
            report.gap_count += ((diff - 1) / step) as usize;
            if report.first_gap.is_none() {
                report.first_gap = Some(ts);
            }
            max_gap = Some(max_gap.map_or(diff, |current| current.max(diff)));
        }
        prev = ts;
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

#[cfg(test)]
mod tests {
    use super::data_quality_from_bars;
    use crate::value_objects::bar::Bar;

    fn bar(ts: i64) -> Bar {
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: ts,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: 1.0,
        }
    }

    #[test]
    fn data_quality_counts_missing_bars_in_gap_count() {
        let bars = vec![bar(0), bar(300)];
        let report = data_quality_from_bars(&bars, Some(60));
        assert_eq!(report.gaps, 1);
        assert_eq!(report.gap_count, 4);
        assert_eq!(report.max_gap_seconds, Some(300));
        assert_eq!(report.first_gap, Some(300));
    }

    #[test]
    fn data_quality_first_and_last_timestamp_use_min_max_even_if_unsorted() {
        let bars = vec![bar(10), bar(5), bar(20)];
        let report = data_quality_from_bars(&bars, Some(1));
        assert_eq!(report.first_timestamp, Some(5));
        assert_eq!(report.last_timestamp, Some(20));
        assert_eq!(report.out_of_order, 1);
    }

    #[test]
    fn data_quality_detects_non_adjacent_duplicates() {
        let bars = vec![bar(0), bar(2), bar(0), bar(4)];
        let report = data_quality_from_bars(&bars, Some(1));
        assert_eq!(report.duplicates, 1);
        assert_eq!(report.first_duplicate, Some(0));
    }

    #[test]
    fn data_quality_counts_gaps_chronologically_even_if_input_unsorted() {
        let bars = vec![bar(10), bar(5), bar(20)];
        let report = data_quality_from_bars(&bars, Some(1));
        assert_eq!(report.gaps, 2);
        assert_eq!(report.gap_count, 13);
        assert_eq!(report.first_gap, Some(10));
        assert_eq!(report.max_gap_seconds, Some(10));
    }
}
