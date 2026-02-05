use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use kairos_domain::services::ohlcv::DataQualityReport;
use kairos_domain::value_objects::bar::Bar;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct OhlcvRecord {
    pub timestamp_utc: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

pub fn load_csv(path: &Path) -> Result<(Vec<Bar>, DataQualityReport), String> {
    load_csv_with_step(path, None)
}

pub fn load_csv_with_step(
    path: &Path,
    expected_step_seconds: Option<i64>,
) -> Result<(Vec<Bar>, DataQualityReport), String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open OHLCV CSV {}: {}", path.display(), err))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut bars_by_ts: BTreeMap<i64, Bar> = BTreeMap::new();
    let mut report = DataQualityReport::default();
    let mut last_seen_ts: Option<i64> = None;
    let mut max_gap: Option<i64> = None;
    let step = expected_step_seconds.unwrap_or(1).max(1);

    for result in reader.deserialize::<OhlcvRecord>() {
        let record = result.map_err(|err| format!("failed to parse CSV row: {}", err))?;
        let timestamp = parse_timestamp(&record.timestamp_utc)?;

        if !record.close.is_finite() || record.close <= 0.0 {
            report.invalid_close += 1;
            if report.first_invalid_close.is_none() {
                report.first_invalid_close = Some(timestamp);
            }
            continue;
        }

        if let Some(prev) = last_seen_ts {
            if timestamp < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(timestamp);
                }
            }
        }
        last_seen_ts = Some(timestamp);

        if bars_by_ts
            .insert(
                timestamp,
                Bar {
                    symbol: "UNKNOWN".to_string(),
                    timestamp,
                    open: record.open,
                    high: record.high,
                    low: record.low,
                    close: record.close,
                    volume: record.volume,
                },
            )
            .is_some()
        {
            report.duplicates += 1;
            if report.first_duplicate.is_none() {
                report.first_duplicate = Some(timestamp);
            }
        }
    }

    if bars_by_ts.is_empty() {
        return Ok((Vec::new(), report));
    }

    let mut bars = Vec::with_capacity(bars_by_ts.len());
    let mut last_unique_ts: Option<i64> = None;
    for (timestamp, bar) in bars_by_ts {
        if report.first_timestamp.is_none() {
            report.first_timestamp = Some(timestamp);
        }
        report.last_timestamp = Some(timestamp);

        if let Some(prev) = last_unique_ts {
            let diff = timestamp - prev;
            if diff > step {
                report.gaps += 1;
                report.gap_count += ((diff - 1) / step) as usize;
                if report.first_gap.is_none() {
                    report.first_gap = Some(timestamp);
                }
                max_gap = Some(max_gap.map_or(diff, |current| current.max(diff)));
            }
        }
        last_unique_ts = Some(timestamp);

        bars.push(bar);
    }

    report.max_gap_seconds = max_gap;
    Ok((bars, report))
}

pub use crate::persistence::postgres_ohlcv::load_postgres;

fn parse_timestamp(value: &str) -> Result<i64, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.timestamp());
    }
    if let Ok(dt) = DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%z") {
        return Ok(dt.timestamp());
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        let dt: DateTime<Utc> = Utc.from_utc_datetime(&naive);
        return Ok(dt.timestamp());
    }

    Err(format!("unsupported timestamp format: {}", value))
}

#[cfg(test)]
mod tests {
    use super::load_csv;
    use kairos_domain::services::ohlcv::{data_quality_from_bars, resample_bars};
    use kairos_domain::value_objects::bar::Bar;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_path(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("kairos_{name}_{}_{}", std::process::id(), now))
    }

    #[test]
    fn load_csv_detects_duplicates_and_gaps() {
        let tmp_path = unique_tmp_path("ohlcv_test.csv");
        let csv_data = "timestamp_utc,open,high,low,close,volume\n\
2026-01-01T00:00:00Z,1,1,1,1,1\n\
2026-01-01T00:00:00Z,1,1,1,1,1\n\
2026-01-01T00:00:02Z,1,1,1,1,1\n";
        fs::write(&tmp_path, csv_data).expect("write csv");

        let (bars, report) = load_csv(&tmp_path).expect("load csv");
        assert_eq!(bars.len(), 2);
        assert_eq!(report.duplicates, 1);
        assert_eq!(report.gaps, 1);
        assert_eq!(report.gap_count, 1);
        assert_eq!(report.invalid_close, 0);
    }

    #[test]
    fn load_csv_detects_non_adjacent_duplicates_and_out_of_order_and_canonicalizes() {
        let tmp_path = unique_tmp_path("ohlcv_test_non_adj.csv");
        let csv_data = "timestamp_utc,open,high,low,close,volume\n\
2026-01-01T00:00:00Z,1,1,1,1,1\n\
2026-01-01T00:00:02Z,1,1,1,1,1\n\
2026-01-01T00:00:01Z,1,1,1,1,1\n\
2026-01-01T00:00:00Z,2,2,2,2,2\n";
        fs::write(&tmp_path, csv_data).expect("write csv");

        let (bars, report) = load_csv(&tmp_path).expect("load csv");
        assert_eq!(report.out_of_order, 2);
        assert_eq!(report.duplicates, 1);
        assert_eq!(bars.len(), 3);
        assert!(bars.windows(2).all(|w| w[0].timestamp <= w[1].timestamp));
        let min_ts = bars.iter().map(|b| b.timestamp).min().expect("min ts");
        let min_bar = bars
            .iter()
            .find(|b| b.timestamp == min_ts)
            .expect("min bar");
        assert!((min_bar.close - 2.0).abs() < 1e-9);
    }

    #[test]
    fn resample_bars_aggregates_ohlcv() {
        let bars = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 0,
                open: 10.0,
                high: 11.0,
                low: 9.0,
                close: 10.5,
                volume: 1.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 60,
                open: 10.5,
                high: 12.0,
                low: 10.0,
                close: 11.0,
                volume: 2.0,
            },
        ];

        let resampled = resample_bars(&bars, 120).expect("resample");
        assert_eq!(resampled.len(), 1);
        assert_eq!(resampled[0].timestamp, 0);
        assert!((resampled[0].open - 10.0).abs() < 1e-9);
        assert!((resampled[0].high - 12.0).abs() < 1e-9);
        assert!((resampled[0].low - 9.0).abs() < 1e-9);
        assert!((resampled[0].close - 11.0).abs() < 1e-9);
        assert!((resampled[0].volume - 3.0).abs() < 1e-9);

        let report = data_quality_from_bars(&resampled, Some(120));
        assert_eq!(report.gaps, 0);
        assert_eq!(report.duplicates, 0);
        assert_eq!(report.out_of_order, 0);
    }
}
